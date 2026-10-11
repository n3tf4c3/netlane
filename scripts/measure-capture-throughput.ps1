param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [int]$Seconds = 5,
    [long]$PacketsPerSecond = 833333,
    [int]$PacketBytes = 1500,
    [switch]$SkipBuild,
    # Synthetic segments are removed once hashes/counters are verified; receipts stay.
    [switch]$KeepSegments
)
# Offline synthetic source only; production capture writer, SHA, sync and analysis.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'diagnostics-qa-common.ps1')
$output = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
$artifactRoot = [IO.Path]::GetFullPath((Join-Path $root 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $output.StartsWith($artifactRoot, [StringComparison]::OrdinalIgnoreCase) -or $output.Contains('"')) { throw 'Use uma pasta nova local sob artifacts.' }
if (Test-Path -LiteralPath $output) { throw 'Evidências anteriores são preservadas; escolha uma pasta nova.' }
Assert-QaLocalPath $output
if ($Seconds -lt 1 -or $Seconds -gt 60 -or $PacketsPerSecond -lt 1 -or $PacketsPerSecond -gt 20000000 -or $PacketBytes -lt 64 -or $PacketBytes -gt 65535) { throw 'Carga inválida.' }
$required = [Math]::Min(8GB, [double]$Seconds * $PacketsPerSecond * ($PacketBytes + 16))
$volume = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($output))
if ($volume.AvailableFreeSpace -lt $required + 24GB) { throw 'Preserve 24 GiB livres além do orçamento do teste.' }
Push-Location $root
try {
    if (-not $SkipBuild) {
        & cargo build -p netlane-capture --example throughput --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Build do benchmark falhou.' }
    }
    New-Item -ItemType Directory -Path $output | Out-Null
    $binary = Join-Path $output 'throughput.exe'
    Copy-Item -LiteralPath (Join-Path $root 'artifacts/rust-target/release/examples/throughput.exe') -Destination $binary
    $hardware = [ordered]@{
        Host=$env:COMPUTERNAME; StartedUtc=[DateTime]::UtcNow.ToString('o')
        Synthetic=$true; NicCapacityProven=$false; LogicalProcessors=[Environment]::ProcessorCount
        Cpu=(Get-CimInstance Win32_Processor | Select-Object -ExpandProperty Name)
        Disks=@(Get-PhysicalDisk | Select-Object FriendlyName,MediaType,Size)
        Adapters=@(Get-NetAdapter | Select-Object Name,InterfaceDescription,Status,LinkSpeed)
        FreeBytesBefore=$volume.AvailableFreeSpace; BinarySha256=(Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
        Seconds=$Seconds; PacketsPerSecond=$PacketsPerSecond; PacketBytes=$PacketBytes
    }
    $hardware | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'scope.json') -Encoding UTF8
    $capture = Join-Path $output 'capture'
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary
    $start.Arguments = '"' + $capture + '" ' + $Seconds + ' ' + $PacketsPerSecond + ' ' + $PacketBytes
    $start.UseShellExecute = $false
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) { throw 'Benchmark não iniciou.' }
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $samples = [Collections.Generic.List[object]]::new()
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while (-not $process.WaitForExit(250)) {
        $process.Refresh()
        if (-not $process.HasExited) {
            $samples.Add([pscustomobject]@{
                ElapsedSeconds=$timer.Elapsed.TotalSeconds; PrivateBytes=$process.PrivateMemorySize64
                WorkingSetBytes=$process.WorkingSet64; CpuSeconds=$process.TotalProcessorTime.TotalSeconds
            })
        }
    }
    $stdout.Result | Set-Content -LiteralPath (Join-Path $output 'stdout.log') -Encoding UTF8
    $stderr.Result | Set-Content -LiteralPath (Join-Path $output 'stderr.log') -Encoding UTF8
    $samples | Export-Csv -LiteralPath (Join-Path $output 'resources.csv') -NoTypeInformation -Encoding UTF8
    $metrics = [ordered]@{
        ExitCode=$process.ExitCode; WallSeconds=$timer.Elapsed.TotalSeconds; Samples=$samples.Count
        PeakPrivateMiB=($samples | Measure-Object PrivateBytes -Maximum).Maximum / 1MB
        PeakWorkingSetMiB=($samples | Measure-Object WorkingSetBytes -Maximum).Maximum / 1MB
        CpuSeconds=$process.TotalProcessorTime.TotalSeconds
        FreeBytesAfter=$volume.AvailableFreeSpace
    }
    $metrics | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'resources.json') -Encoding UTF8
    if ($process.ExitCode -ne 0) { throw ('Benchmark falhou: ' + $stderr.Result) }
    $measurement = Get-Content -LiteralPath (Join-Path $capture 'throughput.json') -Raw | ConvertFrom-Json
    # The benchmark rereads every segment against its SHA-256 before writing throughput.json.
    # Only then are the synthetic PCAPs disposable; capture-final.json keeps their hashes.
    $segments = @(Get-ChildItem -LiteralPath $capture -Filter '*.pcap' -File)
    $retention = [ordered]@{
        KeepSegments=[bool]$KeepSegments; HashesVerified=($measurement.hashes_verified -eq $true)
        Files=$segments.Count; Bytes=[long]($segments | Measure-Object Length -Sum).Sum; Removed=$false
    }
    if (-not $KeepSegments -and $retention.HashesVerified) {
        $segments | Remove-Item -Force
        $retention.Removed = $true
    }
    $retention | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'segments.json') -Encoding UTF8
    [pscustomobject]@{
        RequestedGbps=$measurement.requested_payload_gbps; PublishedGbps=$measurement.published_payload_gbps
        ObservedPps=$measurement.observed_pps; PublishedPps=$measurement.published_pps
        QueueDropped=$measurement.receipt.queue_dropped; Unwritten=$measurement.receipt.unwritten
        SourceBacklog=$measurement.source_backlog_not_polled; Sustained=$measurement.requested_rate_sustained
        PeakPrivateMiB=$metrics.PeakPrivateMiB; NicCapacityProven=$false
        SegmentsRemoved=$retention.Removed; SegmentBytes=$retention.Bytes
    } | ConvertTo-Json -Compress
} finally { Pop-Location }
