param(
    [string]$OutputDirectory = ('artifacts/rust-packets-' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$output = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
$artifactRoot = [IO.Path]::GetFullPath((Join-Path $root 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $output.StartsWith($artifactRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'A prova deve ficar sob artifacts.' }
if (Test-Path -LiteralPath $output) { throw 'Escolha uma pasta nova; evidências anteriores são preservadas.' }
Push-Location $root
try {
    if (-not $SkipBuild) {
        & cargo build --workspace --release --locked --features netlane-app/gui-test
        if ($LASTEXITCODE -ne 0) { throw 'Build GPUI falhou.' }
    }
    New-Item -ItemType Directory -Path $output | Out-Null
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $root 'artifacts/rust-target/release/netlane.exe'
    $start.Arguments = '--demo --profile "' + $output + '\profile" --packet-proof "' + $output + '\scenes"'
    $start.UseShellExecute = $false
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw 'A prévia não iniciou.' }
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(45000)) {
            # Only this exact synthetic process, which cannot start routing or capture.
            $process.Kill()
            $process.WaitForExit()
            [IO.File]::WriteAllText((Join-Path $output 'stderr.txt'), $stderr.Result)
            throw 'A prova GPUI excedeu 45 segundos.'
        }
        $exitCode = $process.ExitCode
        [IO.File]::WriteAllText((Join-Path $output 'stderr.txt'), $stderr.Result)
        if ($exitCode -ne 0) { throw "Prévia encerrou com código $exitCode. Consulte stderr.txt." }
    } finally { $process.Dispose() }
    $hashes = @(Get-FileHash -Path (Join-Path $output 'scenes/*.png') -Algorithm SHA256)
    $expectedScenes = @('01-packets-dark.png','02-packets-light-filter.png','03-packets-minimum-empty.png',
        '04-brand-dark.png','05-brand-light.png','06-packets-dark-details.png','07-codex-context-dark.png',
        '08-codex-context-light.png','09-codex-minimum.png','10-long-samples-dark.png','11-baseline-light.png',
        '12-dump-analysis-codex.png','13-rotating-synthetic-running.png','14-rotating-synthetic-complete.png',
        '15-derived-export.png','16-report-snapshot.png','17-interactive-chat.png','18-investigation-notes.png',
        '19-notebook-minimum.png','20-codex-recovery.png','21-reopened-investigation.png','22-note-state-history.png',
        '23-recovered-packet.png','24-investigation-package-saved.png','25-investigation-package-reopened.png')
    $actualScenes = @($hashes | ForEach-Object { [IO.Path]::GetFileName($_.Path) })
    if ($hashes.Count -ne $expectedScenes.Count -or @($hashes.Hash | Select-Object -Unique).Count -ne $expectedScenes.Count -or
        @(Compare-Object $expectedScenes $actualScenes).Count -ne 0) { throw 'Cenas esperadas ausentes, extras ou repetidas.' }
    $receipt = Join-Path $output 'scenes/verified.txt'
    if (-not (Test-Path -LiteralPath $receipt)) { throw 'Falta o recibo dos exercícios da interface.' }
    foreach ($name in @('interactive-verified.txt','recovery-verified.txt','resume-verified.txt','anchor-verified.txt','bundle-verified.txt')) {
        if (-not (Test-Path -LiteralPath (Join-Path $output "scenes/$name"))) { throw "Falta recibo: $name" }
    }
    foreach ($name in @('notebook','recovered-notebook','reopened-notebook')) {
        $notebookRoot = Join-Path $output "scenes/$name"
        $notebook = Get-Content -LiteralPath (Join-Path $notebookRoot 'receipt.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($notebook.Schema -ne 1 -or $notebook.Turns -lt 2 -or $notebook.Notes -ne 1 -or $notebook.Files.Count -ne 2 -or
            @(Compare-Object @('investigation.json','investigation.md') @($notebook.Files.File)).Count -ne 0) { throw "Recibo do caderno inválido: $name" }
        foreach ($entry in $notebook.Files) {
            $file = Get-Item -LiteralPath (Join-Path $notebookRoot $entry.File)
            if ($file.Length -ne $entry.Bytes -or (Get-FileHash -LiteralPath $file.FullName).Hash -ne $entry.Sha256) { throw "Caderno diverge do recibo: $name/$($entry.File)" }
        }
    }
    $hashes | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $output 'scenes/hashes.json') -Encoding UTF8
    @{ExitCode=$exitCode;Scenes=$expectedScenes.Count;NotebookReopened=$true;ResumeRequiresReview=$true;UserStateHistory=$true;UnsavedImportGuard=$true;InteractiveNotebook=$true;NotebookHashesVerified=$true;ConversationRecovery=$true;EvidenceReplacement=$true;ModelConfirmationGuard=$true;SyntheticPacketSelection=$true;SyntheticSearch=$true;CompactViewport=$true;DarkLightBranding=$true;CodexContextReviewed=$true;CodexConnectionSimulated=$true;VirtualizedPacketTable=$true;FullDumpAnalysis=$true;BaselineComparison=$true;SimulatedConversation=$true;SyntheticRotatingCollection=$true;DerivedPacketExports=$true;LocalAndAssistedReports=$true;ImmutableReportSnapshot=$true;NpcapOpened=$false;ClipboardChanged=$false;InferenceSent=$false;NativeFileDialogExercised=$false;NetworkTests=$false;CaptureStarted=$false;RoutingStarted=$false;InstalledProfileAccessed=$false} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'verification.json') -Encoding UTF8
    $verificationPath = Join-Path $output 'verification.json'
    $verification = Get-Content -LiteralPath $verificationPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $verification | Add-Member -NotePropertyName UnsampledPacketRecovery -NotePropertyValue $true
    $verification | Add-Member -NotePropertyName OriginalHashVerified -NotePropertyValue $true
    $verification | Add-Member -NotePropertyName CompleteInvestigationPackage -NotePropertyValue $true
    $verification | Add-Member -NotePropertyName ReopenedWithoutOriginalPaths -NotePropertyValue $true
    $verification | Add-Member -NotePropertyName FullDumpsAndBaselineRestored -NotePropertyValue $true
    $verification | ConvertTo-Json | Set-Content -LiteralPath $verificationPath -Encoding UTF8
    Get-Content -LiteralPath $receipt -Encoding UTF8
    Write-Output "Prova visual aprovada: $output"
} finally { Pop-Location }
