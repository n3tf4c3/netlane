param(
    [string]$PackageDirectory = (Split-Path -Parent $PSScriptRoot),
    [ValidateRange(1,86400)][int]$StressSeconds = 10,
    [ValidateRange(4,4096)][int]$StressMiB = 64,
    [ValidateRange(100,50000)][int]$StressPacketsPerSecond = 20000
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'diagnostics-qa-common.ps1')
$package = [IO.Path]::GetFullPath($PackageDirectory)
$manifest = Read-QaPackage $package
$results = Get-QaFile $package ('results/offline-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $results | Out-Null
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = Get-QaFile $package 'application/netlane-qa.exe'
$start.Arguments = '--verify-fixtures "' + (Get-QaFile $package 'fixtures') + '" --output "' + (Join-Path $results 'checks') + '" --stress-seconds ' + $StressSeconds + ' --stress-mib ' + $StressMiB + ' --stress-pps ' + $StressPacketsPerSecond
$start.WorkingDirectory = $package
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
$start.RedirectStandardError = $true
$start.RedirectStandardOutput = $true
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
$clock = [Diagnostics.Stopwatch]::StartNew()
$writer = [IO.StreamWriter]::new((Join-Path $results 'resources.csv'), $false, [Text.UTF8Encoding]::new($false))
$maxPrivate = 0L; $maxWorking = 0L; $cpuSeconds = 0.0; $samples = 0; $exitCode = $null; $failure = $null
$started = $false; $stdout = $null; $stderr = $null
$collection = $null
try {
    $writer.WriteLine('ElapsedSeconds,PrivateBytes,WorkingSetBytes,CpuSeconds')
    if (-not $process.Start()) { throw 'O helper QA não iniciou.' }
    $started = $true
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    while (-not $process.HasExited) {
        $process.Refresh()
        if (-not $process.HasExited) {
            $private = $process.PrivateMemorySize64; $working = $process.WorkingSet64
            $cpuSeconds = $process.TotalProcessorTime.TotalSeconds
            $maxPrivate = [Math]::Max($maxPrivate, $private); $maxWorking = [Math]::Max($maxWorking, $working); $samples++
            $writer.WriteLine([string]::Format([Globalization.CultureInfo]::InvariantCulture,
                '{0:F3},{1},{2},{3:F3}', [object[]]@($clock.Elapsed.TotalSeconds,$private,$working,$cpuSeconds)))
        }
        if ($clock.Elapsed.TotalSeconds -gt $StressSeconds + 300) {
            # This PID is our offline helper; it has no real capture or inference adapter.
            $process.Kill(); $process.WaitForExit(); throw 'QA offline excedeu a duração mais 300 s de análise.'
        }
        $null = $process.WaitForExit(1000)
    }
    $process.WaitForExit()
    $exitCode = $process.ExitCode
    $cpuSeconds = $process.TotalProcessorTime.TotalSeconds
    [IO.File]::WriteAllText((Join-Path $results 'stdout.txt'), $stdout.Result)
    [IO.File]::WriteAllText((Join-Path $results 'stderr.txt'), $stderr.Result)
    $receiptPath = Join-Path $results 'checks/verification.json'
    if (-not (Test-Path -LiteralPath $receiptPath)) { throw 'Helper não produziu recibo final.' }
    $receipt = Get-Content -LiteralPath $receiptPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($exitCode -ne 0 -or $receipt.schema -ne 1 -or -not $receipt.passed -or -not $receipt.synthetic_only -or
        $receipt.real_capture_started -or $receipt.inference_sent -or @($receipt.checks | Where-Object { -not $_.passed }).Count) {
        throw 'Ensaio offline reprovado; consulte verification.json e stderr.txt.'
    }
    $collection = (Get-Content -LiteralPath (Join-Path $results 'checks/collection/offline-stress.json') -Raw -Encoding UTF8 | ConvertFrom-Json).receipt
    # Confirm that the package and fixture sources are unchanged after analysis.
    $null = Read-QaPackage $package
} catch {
    $failure = $_.Exception.Message
    if ($started) {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        if ($stdout) { [IO.File]::WriteAllText((Join-Path $results 'stdout.txt'), $stdout.Result) }
        if ($stderr) { [IO.File]::WriteAllText((Join-Path $results 'stderr.txt'), $stderr.Result) }
    }
}
finally { $writer.Dispose(); $clock.Stop(); $process.Dispose() }
Write-QaJson (Join-Path $results 'run.json') ([ordered]@{
    Schema=1;Passed=($null -eq $failure);PackageVersion=$manifest.Version;BaseCommit=$manifest.BaseCommit
    PackageManifestSha256=(Get-FileHash -LiteralPath (Join-Path $package 'qa-package.json')).Hash
    ExitCode=$exitCode;Error=$failure;ElapsedSeconds=$clock.Elapsed.TotalSeconds;Samples=$samples
    PeakSampledPrivateBytes=$maxPrivate;PeakSampledWorkingSetBytes=$maxWorking;CpuSeconds=$cpuSeconds
    ResourceSamplingSeconds=1;Scope='offline-synthetic-helper';GuiResourcesMeasured=$false
    OfflineCollection=$collection
    RealCaptureStarted=$false;InferenceSent=$false;ManualCasesApproved=$false;ExternalWiresharkInterop='pending'
})
Write-Output "Evidências: $results"
if ($failure) { throw $failure }
Write-Output "QA offline aprovado. RAM privada máxima amostrada: $([Math]::Round($maxPrivate/1MB,2)) MiB. Casos reais pendentes."
Write-Output "Carga sintética: vistos=$($collection.seen), salvos=$($collection.written), descartados na fila=$($collection.queue_dropped), não gravados=$($collection.unwritten). Motivo: $($collection.reason)."
