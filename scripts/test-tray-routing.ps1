# Prepares an isolated rule and baseline. Opening the review never starts a service or answers UAC.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BuildDirectory,
    [ValidateSet(5,30)][int]$ActiveSessionLimitMinutes = 5,
    [switch]$OpenReview
)
$ErrorActionPreference = 'Stop'
$repoDirectory = Split-Path -Parent $PSScriptRoot
$build = (Resolve-Path -LiteralPath $BuildDirectory).Path
$artifactPrefix = (Join-Path $repoDirectory 'artifacts') + [IO.Path]::DirectorySeparatorChar
if (-not $build.StartsWith($artifactPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use um build isolado em artifacts.' }
if (@(Get-Process -Name NetLane.UI,NetLane.Service,NetLane.QuicProbe,NetLane.QuicRoutingCheck,NetLane.TrayRoutingReview -ErrorAction SilentlyContinue).Count -ne 0) {
    throw 'Outro painel/servico/probe/ensaio esta ativo. Nao encerrar processos automaticamente.'
}
$hostDirectory = Join-Path $build 'bin\NetLane.TrayRoutingReview\release'
$serviceDirectory = Join-Path $build 'bin\NetLane.Service\release'
$probeDirectory = Join-Path $build 'bin\NetLane.QuicProbe\release'
$reviewExe = Join-Path $hostDirectory 'NetLane.TrayRoutingReview.exe'
$serviceExe = Join-Path $serviceDirectory 'NetLane.Service.exe'
foreach ($binary in @($reviewExe,$serviceExe,(Join-Path $probeDirectory 'NetLane.QuicProbe.exe'))) {
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Build incompleto: $binary" }
}
$runId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$runDirectory = Join-Path $repoDirectory ('artifacts\tray-routing\' + $runId)
[void](New-Item -ItemType Directory -Path $runDirectory)
$copyDirectory = Join-Path $runDirectory 'probe'
[void](New-Item -ItemType Directory -Path $copyDirectory)
$probeHashes = [ordered]@{}
foreach ($name in @('NetLane.QuicProbe.exe','NetLane.QuicProbe.dll','NetLane.QuicProbe.runtimeconfig.json','NetLane.QuicProbe.deps.json')) {
    $target = Join-Path $copyDirectory $name
    Copy-Item -LiteralPath (Join-Path $probeDirectory $name) -Destination $target -ErrorAction Stop
    $probeHashes[$name] = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
}
$protectedHashes = [ordered]@{}
foreach ($directory in @($hostDirectory,$serviceDirectory)) {
    foreach ($file in Get-ChildItem -LiteralPath $directory -File) {
        if ($file.Extension -in @('.exe','.dll','.json')) { $protectedHashes[$file.FullName] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash }
    }
}
$snapshotScript = Join-Path $PSScriptRoot 'quic-network-snapshot.ps1'
$rulesPath = Join-Path $repoDirectory 'src\NetLane.Service\netlane-rules.json'
$reference = & $snapshotScript -RulesPath $rulesPath
$network = ConvertFrom-Json -InputObject $reference
$ethernet = @($network.Interfaces | Where-Object { $_.Name -eq 'Ethernet' })
$wifi = @($network.Interfaces | Where-Object { $_.Name -eq 'Wi-Fi' })
if ($ethernet.Count -ne 1 -or $wifi.Count -ne 1) { throw 'Interfaces ambiguas.' }
$request = [ordered]@{
    Trial = [ordered]@{
        RepositoryPath = $repoDirectory; ProbePath = (Join-Path $copyDirectory 'NetLane.QuicProbe.exe')
        OutputDirectory = $runDirectory; SnapshotScriptPath = $snapshotScript
        SnapshotScriptSha256 = (Get-FileHash -LiteralPath $snapshotScript -Algorithm SHA256).Hash
        RulesPath = $rulesPath; ReferenceNetworkJson = $reference
        EthernetId = ([Guid]$ethernet[0].Guid).ToString('D'); WiFiId = ([Guid]$wifi[0].Guid).ToString('D')
        Host = 'www.cloudflare.com'; ProbeHashes = $probeHashes; CreatedAtUtc = [DateTime]::UtcNow.ToString('o')
        Concurrent = $false; PeerProbePath = $null
    }
    BuildRoot = $build; ServiceExecutable = $serviceExe; ProtectedHashes = $protectedHashes
    ActiveSessionLimitMinutes = $ActiveSessionLimitMinutes
}
$requestPath = Join-Path $runDirectory 'request.json'
[IO.File]::WriteAllText($requestPath, (ConvertTo-Json -InputObject $request -Depth 10), [Text.UTF8Encoding]::new($false))
$requestHash = (Get-FileHash -LiteralPath $requestPath -Algorithm SHA256).Hash
foreach ($mode in @('--prepare','--measure')) {
    $arguments = $mode + ' "' + $requestPath + '" ' + $requestHash
    if ($mode -eq '--measure') { $arguments += ' baseline' }
    $process = Start-Process -FilePath $reviewExe -ArgumentList $arguments -WindowStyle Hidden -PassThru -Wait
    $code = $process.ExitCode
    $process.Dispose()
    if ($code -ne 0) { throw "Preparacao/controle recusado (codigo $code). Consulte error-*.json em $runDirectory" }
}
Write-Output "Controle sem politicas concluido. Recibos: $runDirectory"
if ($OpenReview) {
    $arguments = '--review "' + $requestPath + '" ' + $requestHash
    # This is the specifically requested interactive test window. No RunAs and no service start here.
    $process = Start-Process -FilePath $reviewExe -ArgumentList $arguments -WindowStyle Normal -PassThru
    [IO.File]::WriteAllText((Join-Path $runDirectory 'window-launch.json'), (ConvertTo-Json -InputObject ([ordered]@{
        AtUtc = [DateTime]::UtcNow.ToString('o'); OwnerId = $process.Id; RequestSha256 = $requestHash
        ReviewExecutable = $reviewExe; ServiceStartedByLauncher = $false
        ActiveSessionLimitMinutes = $ActiveSessionLimitMinutes
    })), [Text.UTF8Encoding]::new($false))
    Write-Output "Janela do ensaio solicitada, PID $($process.Id). Inicie a sessao somente pelo painel e UAC manual."
    $process.Dispose()
}
