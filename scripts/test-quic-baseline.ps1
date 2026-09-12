# Baseline only: this script has no elevation, WFP, service or network-setting commands.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ProbePath,
    [switch]$Handshake,
    [ValidatePattern('^[a-zA-Z0-9.-]+$')][string]$ServerName = 'www.cloudflare.com',
    [ValidatePattern('^[0-9.]+$')][string]$DestinationIpv4,
    [ValidateRange(1, 30)][int]$TimeoutSeconds = 15
)

$ErrorActionPreference = 'Stop'
$repoDirectory = Split-Path -Parent $PSScriptRoot
$resolvedProbe = (Resolve-Path -LiteralPath $ProbePath).Path
if ([IO.Path]::GetFileName($resolvedProbe) -ne 'NetLane.QuicProbe.exe' -or
    -not $resolvedProbe.StartsWith($repoDirectory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Use somente o NetLane.QuicProbe.exe compilado dentro deste checkout.'
}
if (-not $Handshake -and ($PSBoundParameters.ContainsKey('ServerName') -or $PSBoundParameters.ContainsKey('DestinationIpv4'))) {
    throw 'Destino somente pode ser informado com -Handshake.'
}
$policyFile = Join-Path $repoDirectory 'src\NetLane.Service\netlane-rules.json'
$runId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$outputDirectory = Join-Path $repoDirectory ('artifacts\quic-baseline\' + $runId)
[void](New-Item -ItemType Directory -Path $outputDirectory)

function Save-Receipt([string]$Name, $Value) {
    $json = ConvertTo-Json -InputObject $Value -Depth 12
    [IO.File]::WriteAllText((Join-Path $outputDirectory $Name), $json, [Text.UTF8Encoding]::new($false))
}

function Read-NetworkState {
    $adapters = @(Get-NetAdapter -Name Ethernet,Wi-Fi | Sort-Object InterfaceGuid)
    if ($adapters.Count -ne 2) { throw 'Ethernet/Wi-Fi não puderam ser identificadas. Reavalie os alvos antes de continuar.' }
    $interfaces = @(foreach ($adapter in $adapters) {
        $index = $adapter.ifIndex
        [ordered]@{
            Name = $adapter.Name
            Guid = $adapter.InterfaceGuid.ToString()
            Index = $index
            Status = $adapter.Status.ToString()
            Bindings = @(Get-NetAdapterBinding -Name $adapter.Name -ComponentID ms_tcpip,ms_tcpip6 |
                Sort-Object ComponentID | Select-Object ComponentID,Enabled)
            Addresses = @(Get-NetIPAddress -InterfaceIndex $index |
                Sort-Object AddressFamily,IPAddress | Select-Object AddressFamily,IPAddress,PrefixLength,AddressState)
            DefaultRoutes = @(Get-NetRoute -InterfaceIndex $index |
                Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0','::/0') } |
                Sort-Object AddressFamily,NextHop,RouteMetric | Select-Object AddressFamily,DestinationPrefix,NextHop,RouteMetric)
            Metrics = @(Get-NetIPInterface -InterfaceIndex $index |
                Sort-Object AddressFamily | Select-Object AddressFamily,AutomaticMetric,InterfaceMetric,ConnectionState)
            Dns = @(Get-DnsClientServerAddress -InterfaceIndex $index |
                Sort-Object AddressFamily | Select-Object AddressFamily,ServerAddresses)
        }
    })
    $flags = @(foreach ($family in @('ipv4','ipv6')) {
        $lines = @(& netsh interface $family show global)
        if ($LASTEXITCODE -ne 0) { throw "Falha de leitura netsh $family." }
        $matching = @($lines | Where-Object { $_ -match '(?i)(route\s*polic|pol.ticas\s+de\s+rota)' })
        if ($matching.Count -ne 1 -or $matching[0] -notmatch '(?i):\s*(enabled|disabled)\s*$') {
            throw "Estado routepolicies $family não verificável. Nenhuma política será ativada."
        }
        [ordered]@{ Family = $family; State = $Matches[1].ToLowerInvariant() }
    })
    [ordered]@{
        Interfaces = $interfaces
        RoutePolicies = $flags
        RealRulesSha256 = (Get-FileHash -LiteralPath $policyFile -Algorithm SHA256).Hash
        ServiceProcessIds = @(Get-Process -Name NetLane.Service -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
    }
}

function Invoke-Probe([string]$Name, [string]$Arguments) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $resolvedProbe
    $start.Arguments = $Arguments
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw 'Não foi possível iniciar o probe.' }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(($TimeoutSeconds + 15) * 1000)) {
            # Only this child: it has no policies to restore. Never terminate UI/service/other apps.
            $process.Kill()
            $process.WaitForExit()
            throw 'Probe excedeu o limite externo. Somente o filho desta execução foi encerrado.'
        }
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        [IO.File]::WriteAllText((Join-Path $outputDirectory ($Name + '.stdout.json')), $stdout, [Text.UTF8Encoding]::new($false))
        $report = if ([string]::IsNullOrWhiteSpace($stdout)) { $null } else { ConvertFrom-Json -InputObject $stdout }
        $receipt = [ordered]@{ ExitCode = $process.ExitCode; StandardError = $stderr; Report = $report }
        Save-Receipt ($Name + '.json') $receipt
        if ($null -eq $report -or $report.SchemaVersion -ne 1 -or $report.ProcessId -ne $process.Id -or
            $report.ProcessPath -ne $resolvedProbe -or $report.RoutingChangedByProbe -ne $false) {
            throw 'Recibo ausente ou incompatível com o executável de prova isolado.'
        }
        return $receipt
    }
    finally { $process.Dispose() }
}

$before = $null
$result = [ordered]@{
    StartedAtUtc = [DateTime]::UtcNow.ToString('o')
    CompletedAtUtc = $null
    HandshakeRequested = [bool]$Handshake
    ProbePath = $resolvedProbe
    ProbeSha256 = (Get-FileHash -LiteralPath $resolvedProbe -Algorithm SHA256).Hash
    ProbeDllSha256 = (Get-FileHash -LiteralPath (Join-Path (Split-Path $resolvedProbe) 'NetLane.QuicProbe.dll') -Algorithm SHA256).Hash
    Status = 'NotStarted'
    SettingsUnchanged = $false
    Error = $null
    Scope = 'Support/baseline only. No routing policy, HTTP response or OneDrive proof.'
}
try {
    $before = Read-NetworkState
    Save-Receipt 'before.json' $before
    if ($before.ServiceProcessIds.Count -ne 0) { throw 'Serviço real em execução: baseline isolado recusado.' }
    if (@($before.RoutePolicies | Where-Object { $_.State -ne 'disabled' }).Count -ne 0) {
        throw 'Baseline exige routepolicies já desativado. Este script não modifica as opções.'
    }
    $support = Invoke-Probe 'support' '--check-support'
    $result.Status = $support.Report.Status
    if ($support.ExitCode -eq 0 -and $Handshake) {
        $arguments = "--handshake --host $ServerName --timeout-seconds $TimeoutSeconds"
        if ($DestinationIpv4) { $arguments += " --ipv4 $DestinationIpv4" }
        $baseline = Invoke-Probe 'baseline' $arguments
        $result.Status = $baseline.Report.Status
    }
}
catch {
    $result.Status = 'Failed'
    $result.Error = $_.Exception.Message
}
finally {
    try {
        $after = Read-NetworkState
        Save-Receipt 'after.json' $after
        $result.SettingsUnchanged = $null -ne $before -and
            (ConvertTo-Json -InputObject $before -Depth 12 -Compress) -ceq (ConvertTo-Json -InputObject $after -Depth 12 -Compress)
    }
    catch { $result.Error = "Falha na conferência final: $($_.Exception.Message). Erro anterior: $($result.Error)" }
    $result.CompletedAtUtc = [DateTime]::UtcNow.ToString('o')
    Save-Receipt 'summary.json' $result
}
Write-Output "Recibos: $outputDirectory"
Write-Output (ConvertTo-Json -InputObject $result -Depth 6)
if (-not $result.SettingsUnchanged -or $result.Status -notin @('Supported','HandshakeObserved')) { exit 1 }
