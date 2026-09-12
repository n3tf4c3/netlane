# Opt-in elevated trial. Without -RunAuthorized, preparation/preflight only, with no traffic/policies.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ControllerPath,
    [Parameter(Mandatory = $true)][string]$ProbePath,
    [switch]$RunAuthorized,
    [switch]$Concurrent,
    [switch]$BaselinePair
)
$ErrorActionPreference = 'Stop'
if ($BaselinePair -and (-not $Concurrent -or $RunAuthorized)) { throw '-BaselinePair exige -Concurrent sem -RunAuthorized.' }
$repoDirectory = Split-Path -Parent $PSScriptRoot
$controller = (Resolve-Path -LiteralPath $ControllerPath).Path
$probe = (Resolve-Path -LiteralPath $ProbePath).Path
$artifactPrefix = (Join-Path $repoDirectory 'artifacts') + [IO.Path]::DirectorySeparatorChar
foreach ($path in @($controller,$probe)) {
    if (-not $path.StartsWith($artifactPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use os binários isolados em artifacts deste checkout.' }
}
if ([IO.Path]::GetFileName($controller) -ne 'NetLane.QuicRoutingCheck.exe' -or [IO.Path]::GetFileName($probe) -ne 'NetLane.QuicProbe.exe') {
    throw 'Executável inesperado; nenhum processo elevado foi iniciado.'
}
if (@(Get-Process -Name NetLane.Service,NetLane.QuicProbe,NetLane.QuicRoutingCheck -ErrorAction SilentlyContinue).Count -ne 0) {
    throw 'Outro serviço/probe/controlador está ativo. Não iniciar ensaios concorrentes.'
}
$snapshotScript = Join-Path $PSScriptRoot 'quic-network-snapshot.ps1'
$rulesPath = Join-Path $repoDirectory 'src\NetLane.Service\netlane-rules.json'
$reference = & $snapshotScript -RulesPath $rulesPath
$parsedReference = ConvertFrom-Json -InputObject $reference
$ethernet = @($parsedReference.Interfaces | Where-Object { $_.Name -eq 'Ethernet' })
$wifi = @($parsedReference.Interfaces | Where-Object { $_.Name -eq 'Wi-Fi' })
if ($ethernet.Count -ne 1 -or $wifi.Count -ne 1) { throw 'Referência de interfaces ambígua.' }
$runId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$outputDirectory = Join-Path $repoDirectory ('artifacts\quic-routing\' + $runId)
[void](New-Item -ItemType Directory -Path $outputDirectory)
$peer = $null
if ($Concurrent) {
    $sourceProbe = $probe
    foreach ($role in @('a','b')) {
        $copyDirectory = Join-Path $outputDirectory ('probe-' + $role)
        [void](New-Item -ItemType Directory -Path $copyDirectory)
        foreach ($file in @('NetLane.QuicProbe.exe','NetLane.QuicProbe.dll','NetLane.QuicProbe.runtimeconfig.json','NetLane.QuicProbe.deps.json')) {
            Copy-Item -LiteralPath (Join-Path (Split-Path $sourceProbe) $file) -Destination (Join-Path $copyDirectory $file) -ErrorAction Stop
        }
    }
    $probe = Join-Path $outputDirectory 'probe-a\NetLane.QuicProbe.exe'
    $peer = Join-Path $outputDirectory 'probe-b\NetLane.QuicProbe.exe'
}
$hashes = [ordered]@{}
foreach ($file in @('NetLane.QuicProbe.exe','NetLane.QuicProbe.dll','NetLane.QuicProbe.runtimeconfig.json','NetLane.QuicProbe.deps.json')) {
    $hashes[$file] = (Get-FileHash -LiteralPath (Join-Path (Split-Path $probe) $file) -Algorithm SHA256).Hash
}
$request = [ordered]@{
    RepositoryPath = $repoDirectory
    ProbePath = $probe
    OutputDirectory = $outputDirectory
    SnapshotScriptPath = $snapshotScript
    SnapshotScriptSha256 = (Get-FileHash -LiteralPath $snapshotScript -Algorithm SHA256).Hash
    RulesPath = $rulesPath
    ReferenceNetworkJson = $reference
    EthernetId = ([Guid]($ethernet[0].Guid)).ToString('D')
    WiFiId = ([Guid]($wifi[0].Guid)).ToString('D')
    Host = 'www.cloudflare.com'
    ProbeHashes = $hashes
    CreatedAtUtc = [DateTime]::UtcNow.ToString('o')
    Concurrent = [bool]$Concurrent
    PeerProbePath = $peer
}
$requestPath = Join-Path $outputDirectory 'request.json'
[IO.File]::WriteAllText($requestPath, (ConvertTo-Json -InputObject $request -Depth 8), [Text.UTF8Encoding]::new($false))
$requestHash = (Get-FileHash -LiteralPath $requestPath -Algorithm SHA256).Hash
$preflight = & $controller --preflight --request $requestPath --request-sha256 $requestHash
if ($LASTEXITCODE -ne 0) { throw "Preflight recusado. Referência: $outputDirectory" }
[IO.File]::WriteAllText((Join-Path $outputDirectory 'preflight.json'), ($preflight -join [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
if ($BaselinePair) {
    & $controller --concurrent-baseline --request $requestPath --request-sha256 $requestHash
    if ($LASTEXITCODE -ne 0) { throw "Baseline concorrente falhou sem ativar políticas. Recibos: $outputDirectory" }
    Write-Output "Baseline concorrente concluído sem políticas. Recibos: $outputDirectory"
    return
}
if (-not $RunAuthorized) {
    Write-Output "Preflight aprovado, sem tráfego/políticas. Referência: $outputDirectory"
    return
}

$launcher = [ordered]@{
    RequestedAtUtc = [DateTime]::UtcNow.ToString('o')
    ManualUacRequested = $true
    Concurrent = [bool]$Concurrent
    AuthorizedScope = 'Temporary active routepolicies IPv4/IPv6; only explicitly listed isolated probe AppIds; no IPv6 binding or real rules changes.'
    ControllerPath = $controller
    ControllerSha256 = (Get-FileHash -LiteralPath $controller -Algorithm SHA256).Hash
    ControllerDllSha256 = (Get-FileHash -LiteralPath (Join-Path (Split-Path $controller) 'NetLane.QuicRoutingCheck.dll') -Algorithm SHA256).Hash
    ProcessId = $null
    Error = $null
}
try {
    $arguments = '--run --request "' + $requestPath + '" --request-sha256 ' + $requestHash + ' --allow-temporary-routepolicies'
    # UAC is manual. No SendKeys, bypass, hidden confirmation response or unattended retry.
    $process = Start-Process -FilePath $controller -ArgumentList $arguments -Verb RunAs -WindowStyle Hidden -PassThru
    $launcher.ProcessId = $process.Id
    $process.Dispose()
}
catch { $launcher.Error = $_.Exception.Message }
[IO.File]::WriteAllText((Join-Path $outputDirectory 'launcher.json'), (ConvertTo-Json -InputObject $launcher -Depth 5), [Text.UTF8Encoding]::new($false))
Write-Output "Recibos: $outputDirectory"
Write-Output (ConvertTo-Json -InputObject $launcher -Depth 5)
if ($launcher.Error) { exit 1 }
Write-Output 'Aguardar result.json e conferir flags/rede/processos. Não encerrar à força o controlador. Para parada segura, criar stop.request neste diretório.'
