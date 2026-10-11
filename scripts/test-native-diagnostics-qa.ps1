param(
    [Parameter(Mandatory)][string]$TargetLabel,
    [int]$Seconds = 1200,
    [string]$OutputDirectory = ('artifacts/native-diagnostics-' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
    [switch]$SkipBuild
)
# Explicit native QA only. Fixed loopback/dummy UDP; no routing, installer or AI.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if ($TargetLabel -ine $env:COMPUTERNAME) { throw 'O alvo precisa corresponder a este PC.' }
if ($Seconds -lt 60 -or $Seconds -gt 3600) { throw 'Duração: 60 a 3600 segundos.' }
$output = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
$artifactRoot = [IO.Path]::GetFullPath((Join-Path $root 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $output.StartsWith($artifactRoot, [StringComparison]::OrdinalIgnoreCase) -or $output.Contains('"')) { throw 'A evidência deve ficar sob artifacts local.' }
if (Test-Path -LiteralPath $output) { throw 'Escolha uma pasta nova; evidências anteriores são preservadas.' }
Push-Location $root
try {
    if (-not $SkipBuild) {
        & cargo build -p netlane-app --release --locked --features gui-test
        if ($LASTEXITCODE -ne 0) { throw 'Build do ensaio nativo falhou.' }
    }
    New-Item -ItemType Directory -Path $output | Out-Null
    $binary = Join-Path $output 'netlane-native-qa.exe'
    Copy-Item -LiteralPath (Join-Path $root 'artifacts/rust-target/release/netlane.exe') -Destination $binary
    $nativeOutput = Join-Path $output 'capture'
    $hardware = [ordered]@{
        Host=$env:COMPUTERNAME; LogicalProcessors=[Environment]::ProcessorCount
        TotalPhysicalMemory=(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
        Cpu=(Get-CimInstance Win32_Processor | Select-Object -ExpandProperty Name)
        Os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber)
        PlannedSeconds=$Seconds; MaximumPacketsPerSecond=100; MaximumDiskMiB=64
        MaximumPrivateMiB=512; MaximumMeanCpuPercent=25; MaximumUiHeartbeatGapSeconds=3
        QueueDropsAllowed=0; DriverDropsAllowed=0; BoundaryUnwrittenAllowed=129
        BoundaryNote='At most the bounded writer queue plus the current record at duration/stop/disk cutoff; every record remains accounted in unwritten.'
        BinarySha256=(Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
        CapturedInterface='Npcap loopback (modern name preferred)'; Bpf='udp and host 127.0.0.1 and port <owned ephemeral test port>; actual value in capture/scope.json'
        Inference=$false; Routing=$false; ExistingProfilesUsed=$false
    }
    $hardware | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'hardware-and-criteria.json') -Encoding UTF8
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary
    $start.Arguments = '--loopback-qa "' + $nativeOutput + '" --qa-target "' + $TargetLabel + '" --qa-seconds ' + $Seconds + ' --profile "' + $nativeOutput + '\profile"'
    $start.UseShellExecute = $false
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    $resourceWriter = $null
    try {
        if (-not $process.Start()) { throw 'O aplicativo QA não iniciou.' }
        $stderr = $process.StandardError.ReadToEndAsync()
        @{Pid=$process.Id;StartedUtc=[DateTime]::UtcNow.ToString('o');Arguments=$start.Arguments} |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'process.json') -Encoding UTF8
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $resources = Join-Path $output 'resources.csv'
        $resourceStream = [IO.File]::Open($resources, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
        $resourceWriter = [IO.StreamWriter]::new($resourceStream, [Text.Encoding]::ASCII)
        $resourceWriter.AutoFlush = $true
        $resourceWriter.WriteLine('ElapsedSeconds,PrivateBytes,WorkingSetBytes,CpuSeconds,CpuPercent,Responding,FreeDiskBytes')
        $previousCpu=0.0; $previousTime=0.0; $maxPrivate=0L; $maxWorkingSet=0L; $samples=0; $notResponding=0
        while (-not $process.HasExited) {
            $process.Refresh()
            if ($process.HasExited) { break }
            $elapsed=$timer.Elapsed.TotalSeconds
            $cpu=$process.TotalProcessorTime.TotalSeconds
            $cpuPercent=100.0*($cpu-$previousCpu)/([Math]::Max(0.001,$elapsed-$previousTime)*[Environment]::ProcessorCount)
            $maxPrivate=[Math]::Max($maxPrivate,$process.PrivateMemorySize64)
            $maxWorkingSet=[Math]::Max($maxWorkingSet,$process.WorkingSet64)
            $responding=$process.Responding
            if (-not $responding) { $notResponding++ }
            $freeDisk=(Get-PSDrive -Name ([IO.Path]::GetPathRoot($output).Substring(0,1))).Free
            $resourceWriter.WriteLine([String]::Format([Globalization.CultureInfo]::InvariantCulture, '{0:F3},{1},{2},{3:F6},{4:F3},{5},{6}', [object[]]@($elapsed,$process.PrivateMemorySize64,$process.WorkingSet64,$cpu,$cpuPercent,$responding,$freeDisk)))
            $previousCpu=$cpu; $previousTime=$elapsed; $samples++
            if ($timer.Elapsed.TotalSeconds -gt $Seconds + 150) {
                # Do not kill an active collector or discard pending writes.
                throw "QA excedeu o prazo. Processo $($process.Id) preservado para inspeção; capture/ conserva os recibos disponíveis."
            }
            Start-Sleep -Milliseconds 1000
        }
        $process.WaitForExit()
        [IO.File]::WriteAllText((Join-Path $output 'stderr.txt'), $stderr.Result)
        $verificationPath=Join-Path $nativeOutput 'verification.json'
        if (-not (Test-Path -LiteralPath $verificationPath)) { throw 'Ensaio nativo falhou; consulte capture/failure.json e stderr.txt.' }
        $native=Get-Content -LiteralPath $verificationPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $meanCpu=100.0*$previousCpu/([Math]::Max(0.001,$previousTime)*[Environment]::ProcessorCount)
        $maxGap=($native.cases | Measure-Object -Property maximum_ui_gap_seconds -Maximum).Maximum
        $result=[ordered]@{
            Passed=($process.ExitCode -eq 0 -and $native.passed -and $maxPrivate -le 512MB -and $meanCpu -le 25 -and $maxGap -le 3 -and $notResponding -eq 0)
            ExitCode=$process.ExitCode; ElapsedSeconds=$timer.Elapsed.TotalSeconds; Samples=$samples
            MaximumPrivateMiB=$maxPrivate/1MB; MaximumWorkingSetMiB=$maxWorkingSet/1MB
            MeanCpuPercent=$meanCpu; NotRespondingSamples=$notResponding; MaximumUiHeartbeatGapSeconds=$maxGap
            NativeCases=@($native.cases.case); NativeReceipt=$verificationPath
            GuiCaptureAndAnalysis=$true; ManualUiInteraction=$false; NativeFilePicker=$false; Installer=$false; AccountQa=$false
        }
        $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'verification.json') -Encoding UTF8
        $result | ConvertTo-Json -Depth 5
        if (-not $result.Passed) { throw 'Critérios de recursos/responsividade não atendidos. Consulte verification.json.' }
    } finally {
        if ($null -ne $resourceWriter) { $resourceWriter.Dispose() }
        $process.Dispose()
    }
} finally { Pop-Location }
