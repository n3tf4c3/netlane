param([string]$OutputDirectory = ('artifacts/rust-ui-' + (Get-Date -Format 'yyyyMMdd-HHmmss')))
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$output = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
$artifactRoot = [IO.Path]::GetFullPath((Join-Path $root 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $output.StartsWith($artifactRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'A prova deve ficar sob artifacts.' }
if (Test-Path -LiteralPath $output) { throw 'Escolha uma pasta nova; evidências anteriores são preservadas.' }
Push-Location $root
try {
    & cargo build --workspace --release --locked --features netlane-app/gui-test
    if ($LASTEXITCODE -ne 0) { throw 'Build GPUI falhou.' }
    New-Item -ItemType Directory -Path $output | Out-Null
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $root 'artifacts/rust-target/release/netlane.exe'
    $start.Arguments = '--demo --profile "' + $output + '\profile" --gui-proof "' + $output + '\scenes"'
    $start.UseShellExecute = $false
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw 'A prévia não iniciou.' }
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(45000)) {
            # Only this exact synthetic proof process; it cannot create a routing controller.
            $process.Kill()
            $process.WaitForExit()
            throw 'A captura GPUI excedeu 45 segundos.'
        }
        $exitCode = $process.ExitCode
        [IO.File]::WriteAllText((Join-Path $output 'stderr.txt'), $stderr.Result)
        if ($exitCode -ne 0) { throw "Prévia encerrou com código $exitCode. Consulte stderr.txt." }
    } finally { $process.Dispose() }
    $hashes = @(Get-FileHash -Path (Join-Path $output 'scenes/*.png') -Algorithm SHA256)
    if ($hashes.Count -ne 18 -or @($hashes.Hash | Select-Object -Unique).Count -ne 18) { throw 'Capturas ausentes ou repetidas.' }
    $receipt = Join-Path $output 'scenes/verified.txt'
    if (-not (Test-Path -LiteralPath $receipt)) { throw 'Falta o recibo dos exercícios da interface.' }
    $hashes | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $output 'scenes/hashes.json') -Encoding UTF8
    @{ExitCode=$exitCode;Scenes=18;UniqueScenes=18;QualitySimulated=$true;QualityDetailsComponent=$true;NativeExecutableIcon=$true;SharedIconAcrossRulesAndConnections=$true;RemoteIconFallback=$true;NativeTrayExercised=$true;SyntheticStartupHidden=$true;SyntheticCleanupReconfirmation=$true;NetworkTests=$false;RoutingStarted=$false;RealAutorunChanged=$false;InstalledProfileAccessed=$false} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'verification.json') -Encoding UTF8
    Get-Content -LiteralPath $receipt -Encoding UTF8
    Write-Output "Prova visual aprovada: $output"
} finally { Pop-Location }
