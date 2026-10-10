[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BuildDirectory,
    [Parameter(Mandatory=$true)][string]$SevenZipPath
)
# Negative static fixtures. No product, installer, UAC, registry or network execution.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'rust-packaging-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$build = Get-ArtifactPath $root $BuildDirectory
$report = Get-Content -Raw -Encoding UTF8 (Join-Path $build 'build-report.json') | ConvertFrom-Json
$resultsDirectory = Get-ArtifactPath $root ('artifacts/rust-package-negative-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $resultsDirectory | Out-Null
$results = [Collections.Generic.List[object]]::new()
function New-Fixture([string]$Name) {
    $directory = Join-Path $resultsDirectory $Name
    New-Item -ItemType Directory -Path $directory | Out-Null
    Copy-Item -LiteralPath (Join-Path $build 'build-report.json') -Destination $directory
    Copy-Item -LiteralPath (Join-Path $build $report.Installer) -Destination $directory
    foreach ($entry in $report.PackagingSources) {
        $target = Get-PackageFile $directory $entry.Path
        New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
        Copy-Item -LiteralPath (Get-PackageFile $build $entry.Path) -Destination $target
    }
    return $directory
}
function Expect-Rejection([string]$Name, [string]$Directory, [string]$Pattern) {
    $caught = $null
    try { & (Join-Path $PSScriptRoot 'test-rust-installer.ps1') -BuildDirectory $Directory -SevenZipPath $SevenZipPath | Out-Null }
    catch { $caught = $_.Exception.Message }
    if (-not $caught -or $caught -notmatch $Pattern) { throw "Recusa incorreta em $Name : $caught" }
    if (@(Get-ChildItem -LiteralPath $Directory -Directory -Filter 'archive-check-*').Count) { throw 'Caso inválido iniciou a extração.' }
    $results.Add([ordered]@{Case=$Name;Rejected=$true;Reason=$caught;ExtractionStarted=$false})
}
function Save-ManifestFixture([string]$Directory, $Manifest) {
    $payload = Join-Path $Directory 'payload'
    New-Item -ItemType Directory -Path $payload | Out-Null
    $manifestPath = Join-Path $payload 'package-manifest.json'
    Write-PackageJson $manifestPath $Manifest
    $fixtureReport = Get-Content -Raw -Encoding UTF8 (Join-Path $Directory 'build-report.json') | ConvertFrom-Json
    $fixtureReport.ManifestSha256 = (Get-FileHash -LiteralPath $manifestPath).Hash
    Write-PackageJson (Join-Path $Directory 'build-report.json') $fixtureReport
}
$directory = New-Fixture 'source-changed'
Add-Content -LiteralPath (Join-Path $directory 'source/RustProcessGuard.nsh') -Value '; altered fixture'
Expect-Rejection 'source-changed' $directory 'Fonte alterada'
$directory = New-Fixture 'setup-changed'
$stream = [IO.File]::Open((Join-Path $directory $report.Installer),[IO.FileMode]::Append)
try { $stream.WriteByte(0) } finally { $stream.Dispose() }
Expect-Rejection 'setup-changed' $directory 'Hash do instalador'
$directory = New-Fixture 'source-duplicate'
$fixtureReport = Get-Content -Raw -Encoding UTF8 (Join-Path $directory 'build-report.json') | ConvertFrom-Json
$fixtureReport.PackagingSources += $fixtureReport.PackagingSources[0]
Write-PackageJson (Join-Path $directory 'build-report.json') $fixtureReport
Expect-Rejection 'source-duplicate' $directory 'Inventário de fontes'
$directory = New-Fixture 'manifest-external-path'
$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $build 'payload/package-manifest.json') | ConvertFrom-Json
$manifest.Files[0].Path = '../outside.txt'
Save-ManifestFixture $directory $manifest
Expect-Rejection 'manifest-external-path' $directory 'Caminho de pacote inválido'
$directory = New-Fixture 'manifest-duplicate'
$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $build 'payload/package-manifest.json') | ConvertFrom-Json
$first = $manifest.Files[0]
$manifest.Files = @($first,$first)
Save-ManifestFixture $directory $manifest
Copy-Item -LiteralPath (Get-PackageFile (Join-Path $build 'payload') $first.Path) -Destination (Get-PackageFile (Join-Path $directory 'payload') $first.Path)
Expect-Rejection 'manifest-duplicate' $directory 'Inventário de arquivos inválido/duplicado'
$directory = New-Fixture 'personal-data'
$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $build 'payload/package-manifest.json') | ConvertFrom-Json
$manifest.Files[0].Path = 'netlane-rules.json'
Save-ManifestFixture $directory $manifest
Expect-Rejection 'personal-data' $directory 'Arquivo inesperado ou dado pessoal'
$directory = New-Fixture 'manifest-ads'
$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $build 'payload/package-manifest.json') | ConvertFrom-Json
$manifest.Files[0].Path = 'netlane.exe:stream'
Save-ManifestFixture $directory $manifest
Expect-Rejection 'manifest-ads' $directory 'Caminho de pacote inválido'
$directory = New-Fixture 'payload-junction'
$target = Join-Path $resultsDirectory 'junction-target'
New-Item -ItemType Directory -Path $target | Out-Null
New-Item -ItemType Junction -Path (Join-Path $directory 'payload') -Target $target | Out-Null
Expect-Rejection 'payload-junction' $directory 'Link/junção recusado'
$result = [ordered]@{Passed=$true;Cases=$results.ToArray();InstallerExecuted=$false;RuntimeQaVerified=$false}
Write-PackageJson (Join-Path $resultsDirectory 'result.json') $result
Write-Output "Oito recusas estáticas aprovadas: $resultsDirectory"
