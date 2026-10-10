[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BuildDirectory,
    [Parameter(Mandatory=$true)][string]$SevenZipPath,
    [string]$OutputDirectory=('artifacts/rust-installer-lab-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
)
# Static validation and local bundle only. Never connects to a guest or executes setup.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'rust-packaging-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$build = Get-ArtifactPath $root $BuildDirectory
$output = Get-ArtifactPath $root $OutputDirectory
if (Test-Path -LiteralPath $output) { throw 'Preserve a rodada anterior; use uma pasta nova.' }
$verification = & (Join-Path $PSScriptRoot 'test-rust-installer.ps1') -BuildDirectory $build -SevenZipPath $SevenZipPath | ConvertFrom-Json
$report = Get-Content -Raw -Encoding UTF8 (Join-Path $build 'build-report.json') | ConvertFrom-Json
if (-not $verification.Passed -or $verification.InstallerSha256 -ne $report.Sha256 -or
    $verification.ManifestSha256 -ne $report.ManifestSha256 -or $verification.RuntimeQaVerified) { throw 'Verificacao estatica incompleta.' }
$collector = Join-Path $PSScriptRoot 'capture-rust-installer-lab-state.ps1'
$tokens = $null; $errors = $null
$null = [Management.Automation.Language.Parser]::ParseFile($collector,[ref]$tokens,[ref]$errors)
if ($errors) { throw 'Sintaxe do coletor invalida.' }
New-Item -ItemType Directory -Path $output | Out-Null
New-Item -ItemType Directory -Path (Join-Path $output 'input'),(Join-Path $output 'scripts') | Out-Null
Copy-Item -LiteralPath (Join-Path $build $report.Installer) -Destination (Join-Path $output ('input/' + $report.Installer))
Copy-Item -LiteralPath (Join-Path $build 'payload/package-manifest.json') -Destination (Join-Path $output 'input/package-manifest.json')
Copy-Item -LiteralPath (Join-Path $build 'build-report.json') -Destination (Join-Path $output 'input/build-report.json')
Copy-Item -LiteralPath $collector -Destination (Join-Path $output 'scripts/capture-rust-installer-lab-state.ps1')
Copy-Item -LiteralPath (Join-Path $root 'docs/qa-laboratorio-rust-2026-10-10.md') -Destination (Join-Path $output 'LEIA-ME.md')
Write-PackageJson (Join-Path $output 'input/static-verification.json') $verification
$plan = [ordered]@{
    Schema=1; Product='NetLaneRustPreview'; HostComputer=$env:COMPUTERNAME
    Target=[ordered]@{
        VmDisplayName='Win11 - Work'; VmPath='C:\VMs\Win11 - Work\Win11 - Work.vmx'; ExpectedMac='00:0c:29:2f:4a:d0'
        Computer=$null; UserSid=$null; Uuid=$null; LabRoot=$null
        Snapshot=[ordered]@{Name=$null;Verified=$false;RecordPath=$null;RecordSha256=$null}
    }
    Package=[ordered]@{
        Version=$report.Version; Installer=('input/' + $report.Installer); Sha256=$report.Sha256
        Manifest='input/package-manifest.json'; ManifestSha256=$report.ManifestSha256
        BuildReport='input/build-report.json'; BuildReportSha256=(Get-FileHash -LiteralPath (Join-Path $build 'build-report.json')).Hash
    }
    ProposedCases=@('cancel-before-copy','clean-install','same-version-reinstall','uninstall')
    RuntimeExecutionAuthorized=$false; InstallerExecuted=$false; RuntimeQaVerified=$false; PublicReleaseReady=$false
}
Write-PackageJson (Join-Path $output 'plan.json') $plan
$files = @(Get-FileInventory $output)
Write-PackageJson (Join-Path $output 'bundle-manifest.json') ([ordered]@{
    Schema=1; Product=$plan.Product; Files=$files; InstallerSha256=$report.Sha256
    GuestIdentityVerified=$false; SnapshotVerified=$false; FilesTransferred=$false
    InstallerExecuted=$false; RuntimeQaVerified=$false; PublicReleaseReady=$false
})
[ordered]@{Bundle=$output;Files=$files.Count;Plan=(Join-Path $output 'plan.json');InstallerSha256=$report.Sha256;InstallerExecuted=$false;FilesTransferred=$false;RuntimeQaVerified=$false}|ConvertTo-Json
