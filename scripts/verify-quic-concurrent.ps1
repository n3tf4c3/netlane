# Independent read-only verification. Only writes a fresh receipt in the completed trial directory.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$RunDirectory)
$ErrorActionPreference = 'Stop'
$repoDirectory = Split-Path -Parent $PSScriptRoot
$directory = (Resolve-Path -LiteralPath $RunDirectory).Path
$allowed = (Join-Path $repoDirectory 'artifacts\quic-routing') + [IO.Path]::DirectorySeparatorChar
if (-not $directory.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw 'Diretório fora dos ensaios deste checkout.' }
function Read-Receipt([string]$Name) { Get-Content -LiteralPath (Join-Path $directory $Name) -Encoding UTF8 -Raw | ConvertFrom-Json }
$request = Read-Receipt 'request.json'
$result = Read-Receipt 'result.json'
$launcher = Read-Receipt 'launcher.json'
$checks = [Collections.Generic.List[object]]::new()
function Check([string]$Name, [bool]$Passed) { $checks.Add([pscustomobject][ordered]@{ Name = $Name; Passed = $Passed }) }
Check 'concurrent scope requested and launched' ($request.Concurrent -eq $true -and $launcher.Concurrent -eq $true -and $result.Scenario -eq 'Concurrent')
Check 'successful result and four pairs' ($result.Passed -eq $true -and @($result.Errors).Count -eq 0 -and @($result.Pairs).Count -eq 4)
Check 'all cleanup receipts confirmed' ($result.PoliciesRemoved -eq $true -and $result.SessionDisposed -eq $true -and $result.FlagsRestored -eq $true -and $result.SettingsUnchanged -eq $true -and $result.CleanupConfirmed -eq $true)
Check 'two distinct isolated executable paths' ($request.ProbePath -ine $request.PeerProbePath -and (Split-Path $request.ProbePath -Parent) -eq (Join-Path $directory 'probe-a') -and (Split-Path $request.PeerProbePath -Parent) -eq (Join-Path $directory 'probe-b'))
$allPids = [Collections.Generic.List[int]]::new()
$reports = @{}
$overlaps = [Collections.Generic.List[object]]::new()
foreach ($stage in @('baseline','routed','swapped','final-control')) {
    foreach ($role in @('A','B')) {
        $raw = Read-Receipt ('raw-' + $stage + '-' + $role + '.json')
        $report = ConvertFrom-Json -InputObject $raw.StandardOutput
        $ready = ConvertFrom-Json -InputObject $raw.Ready
        $reports[$stage + $role] = $report
        $allPids.Add($report.ProcessId)
        $expectedPath = if ($role -eq 'A') { $request.ProbePath } else { $request.PeerProbePath }
        Check ($stage + '/' + $role + ': own child ready before traffic') ($ready.Kind -eq 'Ready' -and $ready.NetworkAttempted -eq $false -and $ready.ProcessId -eq $raw.ProcessId -and $ready.ProcessPath -eq $expectedPath -and $report.ProcessId -eq $raw.ProcessId -and $report.ProcessPath -eq $expectedPath)
        Check ($stage + '/' + $role + ': QUIC h3 complete and disposed') ($raw.ExitCode -eq 0 -and $report.SchemaVersion -eq 1 -and $report.Mode -eq 'ConcurrentChild' -and $report.Status -eq 'HandshakeObserved' -and $report.Handshake.Alpn -eq 'h3' -and $report.Handshake.RemotePort -eq 443 -and $report.ConnectionDisposed -eq $true -and $report.NetworkAttempted -eq $true)
        Check ($stage + '/' + $role + ': same remote IPv4 and no source override') ($report.Host -eq $request.Host -and $report.Handshake.RemoteAddress -eq $result.Pairs[0].A.Probe.RemoteAddress -and $report.TargetIpv4 -eq $report.Handshake.RemoteAddress -and ([IPAddress]$report.Handshake.RemoteAddress).AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork -and $report.ManualSourceBinding -eq $false -and $report.TcpFallback -eq $false -and $report.RoutingChangedByProbe -eq $false -and $report.HttpResponseVerified -eq $false)
        $window = $report.ConcurrentTiming
        $duration = ([decimal]$window.ObservedUntilTimestamp - [decimal]$window.ConnectedTimestamp) * 1000 / [decimal]$window.Frequency
        Check ($stage + '/' + $role + ': open interval and no observed peer closure') ($window.PeerClosureObserved -eq $false -and $window.Frequency -eq [Diagnostics.Stopwatch]::Frequency -and $window.ConnectedTimestamp -gt 0 -and $window.ObservedUntilTimestamp -le [Diagnostics.Stopwatch]::GetTimestamp() -and $duration -ge 2900)
        if ($stage -in @('routed','swapped')) {
            $ethernetRole = ($stage -eq 'routed' -and $role -eq 'A') -or ($stage -eq 'swapped' -and $role -eq 'B')
            $expectedId = if ($ethernetRole) { $request.EthernetId } else { $request.WiFiId }
            $policy = Read-Receipt ('policy-' + $stage + '-' + $role + '.json')
            Check ($stage + '/' + $role + ': own policy and observed source agree') ($policy.Result.Applied -eq $true -and $policy.Role -eq $role -and ([Guid]$policy.Adapter.Id) -eq ([Guid]$expectedId) -and @($report.ObservedInterfaces).Count -eq 1 -and ([Guid]$report.ObservedInterfaces[0].Id) -eq ([Guid]$expectedId) -and $report.Handshake.LocalAddress -eq $policy.Adapter.Address)
        }
    }
    $a = $reports[$stage + 'A'].ConcurrentTiming
    $b = $reports[$stage + 'B'].ConcurrentTiming
    $overlap = ([Math]::Min([decimal]$a.ObservedUntilTimestamp, [decimal]$b.ObservedUntilTimestamp) - [Math]::Max([decimal]$a.ConnectedTimestamp, [decimal]$b.ConnectedTimestamp)) * 1000 / [decimal]$a.Frequency
    $pair = @($result.Pairs | Where-Object { $_.Stage -eq $stage })
    Check ($stage + ': independently recomputed overlap at least 1 second') ($pair.Count -eq 1 -and $a.Frequency -eq $b.Frequency -and $overlap -ge 1000 -and [Math]::Abs($overlap - [decimal]$pair[0].OverlapMilliseconds) -lt 0.01)
    $overlaps.Add([pscustomobject]@{ Stage = $stage; Milliseconds = $overlap })
}
Check 'eight distinct child processes' (@($allPids | Sort-Object -Unique).Count -eq 8)
foreach ($role in @('A','B')) {
    Check ($role + ': final source matches its baseline') ($reports['final-control' + $role].Handshake.LocalAddress -eq $reports['baseline' + $role].Handshake.LocalAddress -and ([Guid]$reports['final-control' + $role].ObservedInterfaces[0].Id) -eq ([Guid]$reports['baseline' + $role].ObservedInterfaces[0].Id))
}
$current = & (Join-Path $PSScriptRoot 'quic-network-snapshot.ps1') -RulesPath (Join-Path $repoDirectory 'src\NetLane.Service\netlane-rules.json')
Check 'live network/rules/service matches reference' ($current -ceq $request.ReferenceNetworkJson)
$currentParsed = ConvertFrom-Json -InputObject $current
Check 'both IPv6 bindings still disabled' (@($currentParsed.Interfaces | ForEach-Object { $_.Bindings | Where-Object { $_.ComponentID -eq 'ms_tcpip6' -and $_.Enabled -eq $false } }).Count -eq 2)
foreach ($family in @('ipv4','ipv6')) {
    $lines = @(& netsh interface $family show global)
    $matching = @($lines | Where-Object { $_ -match '(?i)(route\s*polic|pol.{1,2}ticas\s+de\s+rota)' })
    Check ($family + ': routepolicies currently disabled') ($LASTEXITCODE -eq 0 -and $matching.Count -eq 1 -and $matching[0] -match '(?i):\s*(disabled|desabilitado)\s*$')
}
Check 'no controller, probe or real service remains' (@(Get-Process -Name NetLane.QuicRoutingCheck,NetLane.QuicProbe,NetLane.Service -ErrorAction SilentlyContinue).Count -eq 0)
foreach ($path in @($request.ProbePath,$request.PeerProbePath)) {
    foreach ($entry in $request.ProbeHashes.PSObject.Properties) {
        Check ((Split-Path (Split-Path $path) -Leaf) + ': hash unchanged ' + $entry.Name) ((Get-FileHash -LiteralPath (Join-Path (Split-Path $path) $entry.Name) -Algorithm SHA256).Hash -eq $entry.Value)
    }
}
Check 'controller DLL unchanged since launch' ((Get-FileHash -LiteralPath (Join-Path (Split-Path $launcher.ControllerPath) 'NetLane.QuicRoutingCheck.dll') -Algorithm SHA256).Hash -eq $launcher.ControllerDllSha256)
$verification = [ordered]@{
    CheckedAtUtc = [DateTime]::UtcNow.ToString('o')
    Passed = @($checks | Where-Object { -not $_.Passed }).Count -eq 0
    CheckCount = $checks.Count
    Checks = $checks.ToArray()
    Overlaps = $overlaps.ToArray()
    CurrentNetworkJson = $current
    Scope = 'Independent verification of overlapping observed connections and cleanup, not sustained payload traffic or global WFP enumeration.'
}
$receipt = Join-Path $directory ('concurrent-verification-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '.json')
$stream = [IO.FileStream]::new($receipt, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
try {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $verification -Depth 10))
    $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true)
}
finally { $stream.Dispose() }
Write-Output "Verificação: $receipt"
$checks | Format-Table -AutoSize
$overlaps | Format-Table -AutoSize
if (-not $verification.Passed) { exit 1 }
