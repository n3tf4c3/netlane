param(
    [ValidateRange(30, 900)]
    [int]$DurationSeconds = 180,
    [switch]$CheckOnly,
    [switch]$UntilStopped,
    [switch]$Stop
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$reportRoot = Join-Path $repoRoot 'artifacts\onedrive-wifi'
$controlPath = Join-Path $reportRoot 'active-session.json'
if ($Stop) {
    if ($UntilStopped -or $CheckOnly) { throw 'Use -Stop sozinho.' }
    if (-not (Test-Path -LiteralPath $controlPath -PathType Leaf)) {
        Write-Host 'Nenhuma sessao controlada foi encontrada.'
        return
    }
    $control = Get-Content -LiteralPath $controlPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($control.State -ne 'Running') { Write-Host 'A sessao ja esta encerrada.'; return }
    $controlledRun = [IO.Path]::GetFullPath([string]$control.RunDirectory)
    if ([IO.Path]::GetDirectoryName($controlledRun) -ine [IO.Path]::GetFullPath($reportRoot) -or
        [IO.Path]::GetFileName($controlledRun) -notmatch '^\d{8}-\d{6}-[0-9a-f]{8}$' -or
        -not (Test-Path -LiteralPath $controlledRun -PathType Container)) {
        throw 'O diretorio da sessao nao pertence aos relatorios do OneDrive.'
    }
    # The unelevated caller can request only cleanup, never supply a command or target PID.
    New-Item -ItemType File -Path (Join-Path $controlledRun 'stop.request') -Force | Out-Null
    Write-Host 'Parada solicitada. O controlador encerrara seu servico e restaurara as opcoes de rede.'
    return
}
if ($UntilStopped -and $PSBoundParameters.ContainsKey('DurationSeconds')) {
    throw 'Escolha -UntilStopped ou -DurationSeconds, sem combinar os modos.'
}
$mode = if ($UntilStopped) { 'UntilStopped' } else { 'Timed' }
$policyPath = Join-Path $repoRoot 'src\NetLane.Service\netlane-rules.json'
$serviceDirectory = Join-Path $repoRoot 'src\NetLane.Service\bin\Release\net8.0-windows'
$servicePath = Join-Path $serviceDirectory 'NetLane.Service.exe'
$policyHash = (Get-FileHash -LiteralPath $policyPath -Algorithm SHA256).Hash
$policies = Get-Content -LiteralPath $policyPath -Raw -Encoding UTF8 | ConvertFrom-Json
$active = @($policies | Where-Object { $_.enabled -and $_.routeMode -ne 'Automatic' })
if ($active.Count -ne 1 -or $active[0].applicationId -ine 'OneDrive.exe' -or
    $active[0].routeMode -ne 'WiFi' -or $active[0].includeRelatedExecutables -or
    [IO.Path]::GetFileName($active[0].executablePath) -ine 'OneDrive.exe') {
    throw 'O teste exige somente OneDrive.exe ativo em WiFi, sem herdar auxiliares.'
}
if (-not (Test-Path -LiteralPath $active[0].executablePath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $servicePath -PathType Leaf)) {
    throw 'Executavel ausente. Compile NetLane.Service em Release e confira o caminho do OneDrive.'
}
$adapter = @(Get-NetAdapter | Where-Object { [guid]$_.InterfaceGuid -eq [guid]$active[0].interfaceId })
if ($adapter.Count -ne 1 -or $adapter[0].Status -ne 'Up') { throw 'A Wi-Fi selecionada nao esta conectada.' }
$wifiAddresses = @(Get-NetIPAddress -InterfaceIndex $adapter[0].ifIndex -AddressFamily IPv4 |
    Where-Object { $_.AddressState -eq 'Preferred' } | Select-Object -ExpandProperty IPAddress)
if ($wifiAddresses.Count -eq 0) { throw 'A Wi-Fi selecionada nao tem IPv4 disponivel.' }
$otherServices = @(Get-CimInstance Win32_Process | Where-Object {
    $_.Name -eq 'NetLane.Service.exe' -or ($_.Name -eq 'dotnet.exe' -and $_.CommandLine -match 'NetLane.Service')
})
if ($otherServices.Count -gt 0) { throw 'Ja existe um NetLane.Service ativo. Encerre essa instancia antes do teste.' }

function Read-RoutePolicies([string]$Family) {
    $output = & netsh.exe interface $Family show global
    if ($LASTEXITCODE -ne 0) { throw "Falha ao consultar routepolicies $Family." }
    $match = [regex]::Matches(($output -join "`n"),
        '(?im)^\s*(?:route\s*policies|pol.{1,2}ticas\s+de\s+rota)\s*[:=]\s*(enabled|disabled|habilitado|desabilitado)\s*$')
    if ($match.Count -ne 1) { throw "Nao foi possivel determinar routepolicies $Family; nenhuma suposicao sera feita." }
    return $match[0].Groups[1].Value.ToLowerInvariant() -in @('enabled', 'habilitado')
}

function Set-RoutePolicies([string]$Family, [bool]$Enabled) {
    $value = if ($Enabled) { 'enabled' } else { 'disabled' }
    $output = & netsh.exe interface $Family set global "routepolicies=$value" store=active
    if ($LASTEXITCODE -ne 0) { throw "Falha ao definir routepolicies $Family=$value : $output" }
    if ((Read-RoutePolicies $Family) -ne $Enabled) { throw "routepolicies $Family nao confirmou $value." }
}

function Read-OneDriveConnections {
    $oneDriveIds = @(Get-Process -Name OneDrive -ErrorAction SilentlyContinue).Id
    if ($oneDriveIds.Count -eq 0) { return }
    Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue |
        Where-Object { $_.OwningProcess -in $oneDriveIds -and $_.LocalAddress -notmatch '^(127\.|::1$)' } |
        Select-Object OwningProcess, LocalAddress, LocalPort, RemoteAddress, RemotePort,
            @{ Name = 'CreatedAtUtc'; Expression = { $_.CreationTime.ToUniversalTime().ToString('o') } }
}

$before = @{ ipv4 = (Read-RoutePolicies 'ipv4'); ipv6 = (Read-RoutePolicies 'ipv6') }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$isAdmin = ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($CheckOnly) {
    [pscustomobject]@{ Administrator = $isAdmin; Target = $active[0].executablePath;
        Wifi = $adapter[0].Name; WifiAddresses = $wifiAddresses; RoutePolicies = $before;
        Mode = $mode; DurationSeconds = if ($UntilStopped) { $null } else { $DurationSeconds };
        Connections = @(Read-OneDriveConnections) } | ConvertTo-Json -Depth 5
    return
}
if (-not $isAdmin) { throw 'Execute este script em PowerShell como Administrador. Nenhuma opcao de rede foi alterada.' }

New-Item -ItemType Directory -Path $reportRoot -Force | Out-Null
$runnerLock = [IO.FileStream]::new((Join-Path $reportRoot 'controller.lock'), [IO.FileMode]::OpenOrCreate,
    [IO.FileAccess]::ReadWrite, [IO.FileShare]::None, 1, [IO.FileOptions]::DeleteOnClose)
$runDirectory = Join-Path $reportRoot ((Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $runDirectory | Out-Null
$stopPath = Join-Path $runDirectory 'stop.request'
$changedFamilies = [Collections.Generic.List[string]]::new()
$samples = [Collections.Generic.List[object]]::new()
$sampleCount = 0
$service = $null
$receipt = $null
$acceptedAt = $null
$failure = $null
$cleanupErrors = [Collections.Generic.List[string]]::new()
$baseline = @(Read-OneDriveConnections)
$baselineRoutes = @(Get-NetRoute -DestinationPrefix '0.0.0.0/0' |
    Select-Object InterfaceIndex, NextHop, RouteMetric)
$startedAt = [DateTimeOffset]::UtcNow
try {
    [pscustomobject]@{ State = 'Running'; Mode = $mode; RunnerPid = $PID;
        RunDirectory = $runDirectory; StartedAtUtc = $startedAt.ToString('o') } | ConvertTo-Json |
        Set-Content -LiteralPath $controlPath -Encoding UTF8
    foreach ($family in @('ipv4', 'ipv6')) {
        if (-not $before[$family]) {
            # Remember the rollback before invoking netsh, including a partial failure.
            $changedFamilies.Add($family)
            Set-RoutePolicies $family $true
        }
    }
    if ((Get-FileHash -LiteralPath $policyPath -Algorithm SHA256).Hash -ne $policyHash) {
        throw 'As regras mudaram durante a preparacao. Teste cancelado.'
    }
    $service = Start-Process -FilePath $servicePath -WorkingDirectory $serviceDirectory -WindowStyle Hidden -PassThru `
        -ArgumentList @('--NetLane:Routing:PolicyFilePath', ('"' + $policyPath + '"'), '--NetLane:Routing:PollIntervalSeconds', '5') `
        -RedirectStandardOutput (Join-Path $runDirectory 'service.log') -RedirectStandardError (Join-Path $runDirectory 'service-error.log')
    $deadline = if ($UntilStopped) { $null } else { [DateTimeOffset]::UtcNow.AddSeconds($DurationSeconds) }
    while (($UntilStopped -or [DateTimeOffset]::UtcNow -lt $deadline) -and -not (Test-Path -LiteralPath $stopPath)) {
        $service.Refresh()
        if ($service.HasExited) { throw "O servico encerrou com codigo $($service.ExitCode). Consulte service.log." }
        if ((Get-FileHash -LiteralPath $policyPath -Algorithm SHA256).Hash -ne $policyHash) {
            throw 'As regras foram alteradas durante o teste. Encerrando esta sessao.'
        }
        $runtimePath = $policyPath + '.runtime.json'
        if (Test-Path -LiteralPath $runtimePath) {
            $candidate = Get-Content -LiteralPath $runtimePath -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($candidate.ProcessId -eq $service.Id -and $candidate.PolicyRevision -eq $policyHash) {
                if ($candidate.State -ne 'Ready' -or @($candidate.Rules).Count -ne 1 -or
                    $candidate.Rules[0].ApplicationId -ine 'OneDrive.exe' -or -not $candidate.Rules[0].Applied) {
                    throw "A politica OneDrive nao foi aceita: $($candidate | ConvertTo-Json -Compress -Depth 5)"
                }
                $receipt = $candidate
                if ($null -eq $acceptedAt) { $acceptedAt = [DateTimeOffset]::Parse($candidate.UpdatedAtUtc) }
            }
        }
        $connections = @(Read-OneDriveConnections)
        $sampleCount++
        # Keep memory bounded for a synchronization session without a fixed deadline.
        if ($samples.Count -ge 1800) { $samples.RemoveAt(0) }
        $samples.Add([pscustomobject]@{ AtUtc = [DateTimeOffset]::UtcNow.ToString('o'); Connections = $connections })
        [pscustomobject]@{ State = 'Observing'; Mode = $mode; RunnerPid = $PID; ServicePid = $service.Id;
            AcceptedAtUtc = if ($null -ne $acceptedAt) { $acceptedAt.ToString('o') } else { $null };
            EndsAtUtc = if ($null -ne $deadline) { $deadline.ToString('o') } else { $null };
            WifiAddresses = $wifiAddresses;
            Connections = $connections } | ConvertTo-Json -Depth 6 |
            Set-Content -LiteralPath (Join-Path $runDirectory 'progress.json') -Encoding UTF8
        if ($null -eq $acceptedAt -and ([DateTimeOffset]::UtcNow - $startedAt).TotalSeconds -gt 20) {
            throw 'O servico nao confirmou a politica dentro de 20 segundos.'
        }
        Start-Sleep -Seconds 2
    }
}
catch { $failure = $_.Exception.Message }
finally {
    # Stop only the service process created above. Closing its dynamic WFP session removes its policies.
    if ($null -ne $service) {
        try {
            $service.Refresh()
            if (-not $service.HasExited) { $service.Kill(); $service.WaitForExit() }
        }
        catch { $cleanupErrors.Add("Encerramento do servico: $($_.Exception.Message)") }
    }
    foreach ($family in $changedFamilies) {
        try { Set-RoutePolicies $family $before[$family] }
        catch { $cleanupErrors.Add($_.Exception.Message) }
    }
    [pscustomobject]@{ Mode = $mode; StartedAtUtc = $startedAt.ToString('o'); FinishedAtUtc = [DateTimeOffset]::UtcNow.ToString('o');
        AcceptedAtUtc = if ($null -ne $acceptedAt) { $acceptedAt.ToString('o') } else { $null };
        PolicyPath = $policyPath; PolicyHash = $policyHash; AcceptedReceipt = $receipt;
        WifiAddresses = $wifiAddresses; RoutePoliciesBefore = $before;
        BaselineConnections = $baseline; BaselineRoutes = $baselineRoutes; Samples = $samples.ToArray();
        TotalSamples = $sampleCount; SamplesTruncated = ($sampleCount -gt $samples.Count);
        Failure = $failure; CleanupErrors = $cleanupErrors.ToArray() } | ConvertTo-Json -Depth 10 |
        Set-Content -LiteralPath (Join-Path $runDirectory 'result.json') -Encoding UTF8
    [pscustomobject]@{ State = 'Finished'; FinishedAtUtc = [DateTimeOffset]::UtcNow.ToString('o');
        Failure = $failure; CleanupErrors = $cleanupErrors.ToArray();
        ResultPath = (Join-Path $runDirectory 'result.json') } | ConvertTo-Json -Depth 5 |
        Set-Content -LiteralPath (Join-Path $runDirectory 'progress.json') -Encoding UTF8
    [pscustomobject]@{ State = 'Finished'; Mode = $mode; RunDirectory = $runDirectory;
        FinishedAtUtc = [DateTimeOffset]::UtcNow.ToString('o'); CleanupErrors = $cleanupErrors.ToArray() } |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $controlPath -Encoding UTF8
    $runnerLock.Dispose()
    Write-Host "Relatorio: $runDirectory"
}
if ($failure -or $cleanupErrors.Count -gt 0) { throw "Teste incompleto. $failure $($cleanupErrors -join '; ')" }
Write-Host 'Teste encerrado e opcoes globais restauradas. Confira as conexoes medidas em result.json.'
