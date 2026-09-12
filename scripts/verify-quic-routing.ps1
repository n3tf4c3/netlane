# Read-only verification of a completed trial; writes only a new evidence receipt in that trial directory.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$RunDirectory)
$ErrorActionPreference = 'Stop'
$repoDirectory = Split-Path -Parent $PSScriptRoot
$directory = (Resolve-Path -LiteralPath $RunDirectory).Path
$allowed = (Join-Path $repoDirectory 'artifacts\quic-routing') + [IO.Path]::DirectorySeparatorChar
if (-not $directory.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw 'Diretório fora dos ensaios QUIC deste checkout.' }
function Read-Receipt([string]$Name) { Get-Content -LiteralPath (Join-Path $directory $Name) -Encoding UTF8 -Raw | ConvertFrom-Json }
$request = Read-Receipt 'request.json'
$result = Read-Receipt 'result.json'
$launcher = Read-Receipt 'launcher.json'
$checks = [Collections.Generic.List[object]]::new()
function Check([string]$Name, [bool]$Passed) { $checks.Add([pscustomobject][ordered]@{ Name = $Name; Passed = $Passed }) }
Check 'controller reported pass without errors' ($result.Passed -eq $true -and @($result.Errors).Count -eq 0)
Check 'controller reported complete cleanup' ($result.PoliciesRemoved -eq $true -and $result.SessionDisposed -eq $true -and $result.FlagsRestored -eq $true -and $result.SettingsUnchanged -eq $true -and $result.CleanupConfirmed -eq $true)
Check 'four distinct probe processes' (@($result.Steps).Count -eq 4 -and @($result.Steps | ForEach-Object { $_.Probe.ProcessId } | Sort-Object -Unique).Count -eq 4)
$reports = @{}
foreach ($stage in @('baseline','Ethernet','WiFi','final-control')) {
    $raw = Read-Receipt ('raw-' + $stage + '.json')
    $report = ConvertFrom-Json -InputObject $raw.StandardOutput
    $reports[$stage] = $report
    Check ($stage + ': actual child identity and successful exit') ($raw.ExitCode -eq 0 -and $raw.ProcessId -eq $report.ProcessId -and $report.ProcessPath -eq $request.ProbePath -and $report.Host -eq $request.Host -and $report.SchemaVersion -eq 1)
    Check ($stage + ': completed QUIC h3 handshake and disposal') ($report.Status -eq 'HandshakeObserved' -and $report.Handshake.Alpn -eq 'h3' -and $report.Handshake.RemotePort -eq 443 -and $report.ConnectionDisposed -eq $true -and $report.NetworkAttempted -eq $true)
    Check ($stage + ': no manual binding, policy writes, TCP fallback or HTTP claim') ($report.ManualSourceBinding -eq $false -and $report.RoutingChangedByProbe -eq $false -and $report.TcpFallback -eq $false -and $report.HttpResponseVerified -eq $false)
    Check ($stage + ': same remote IPv4') ($report.Handshake.RemoteAddress -eq $result.Steps[0].Probe.RemoteAddress -and $report.TargetIpv4 -eq $report.Handshake.RemoteAddress -and ([IPAddress]$report.Handshake.RemoteAddress).AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork)
}
foreach ($stage in @('Ethernet','WiFi')) {
    $policy = Read-Receipt ('policy-' + $stage + '.json')
    $expectedGuid = if ($stage -eq 'Ethernet') { $request.EthernetId } else { $request.WiFiId }
    $observed = @($reports[$stage].ObservedInterfaces)
    Check ($stage + ': policy receipt and observed interface agree') ($policy.Result.Applied -eq $true -and $observed.Count -eq 1 -and ([Guid]$observed[0].Id) -eq ([Guid]$expectedGuid) -and $reports[$stage].Handshake.LocalAddress -eq $policy.Adapter.Address)
}
Check 'Wi-Fi source differs from Ethernet' ($reports['WiFi'].Handshake.LocalAddress -ne $reports['Ethernet'].Handshake.LocalAddress)
Check 'final control returned to initial source' ($reports['final-control'].Handshake.LocalAddress -eq $reports['baseline'].Handshake.LocalAddress -and ([Guid]$reports['final-control'].ObservedInterfaces[0].Id) -eq ([Guid]$reports['baseline'].ObservedInterfaces[0].Id))
$current = & (Join-Path $PSScriptRoot 'quic-network-snapshot.ps1') -RulesPath (Join-Path $repoDirectory 'src\NetLane.Service\netlane-rules.json')
Check 'live network/rules/service snapshot matches pre-UAC reference' ($current -ceq $request.ReferenceNetworkJson)
$currentParsed = ConvertFrom-Json -InputObject $current
Check 'IPv6 bindings still disabled on both adapters' (@($currentParsed.Interfaces | ForEach-Object { $_.Bindings | Where-Object { $_.ComponentID -eq 'ms_tcpip6' -and $_.Enabled -eq $false } }).Count -eq 2)
foreach ($family in @('ipv4','ipv6')) {
    $lines = @(& netsh interface $family show global)
    $matching = @($lines | Where-Object { $_ -match '(?i)(route\s*polic|pol.{1,2}ticas\s+de\s+rota)' })
    Check ($family + ': routepolicies currently disabled') ($LASTEXITCODE -eq 0 -and $matching.Count -eq 1 -and $matching[0] -match '(?i):\s*(disabled|desabilitado)\s*$')
}
Check 'no controller, probe or real service left running' (@(Get-Process -Name NetLane.QuicRoutingCheck,NetLane.QuicProbe,NetLane.Service -ErrorAction SilentlyContinue).Count -eq 0)
foreach ($entry in $request.ProbeHashes.PSObject.Properties) {
    Check ('probe artifact unchanged: ' + $entry.Name) ((Get-FileHash -LiteralPath (Join-Path (Split-Path $request.ProbePath) $entry.Name) -Algorithm SHA256).Hash -eq $entry.Value)
}
Check 'controller DLL unchanged since launch' ((Get-FileHash -LiteralPath (Join-Path (Split-Path $launcher.ControllerPath) 'NetLane.QuicRoutingCheck.dll') -Algorithm SHA256).Hash -eq $launcher.ControllerDllSha256)
$verification = [ordered]@{
    CheckedAtUtc = [DateTime]::UtcNow.ToString('o')
    TrialDirectory = $directory
    Passed = @($checks | Where-Object { -not $_.Passed }).Count -eq 0
    CheckCount = $checks.Count
    Checks = $checks.ToArray()
    CurrentNetworkJson = $current
    Scope = 'Independent read-only verification; no global WFP enumeration, HTTP/3 download or real-application proof.'
}
$receiptPath = Join-Path $directory ('verification-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '.json')
$stream = [IO.FileStream]::new($receiptPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
try {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $verification -Depth 10))
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
}
finally { $stream.Dispose() }
Write-Output "Verificação: $receiptPath"
$checks | Format-Table -AutoSize
if (-not $verification.Passed) { exit 1 }
