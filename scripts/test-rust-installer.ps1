[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BuildDirectory,
    [Parameter(Mandatory=$true)][string]$SevenZipPath
)
# Static extraction/validation only. Never execute an installer or bundled binary.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'rust-packaging-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$build = Get-ArtifactPath $root $BuildDirectory
$archiveTool = (Resolve-Path -LiteralPath $SevenZipPath).Path
Assert-RegularPath $archiveTool
$report = Get-Content -Raw -Encoding UTF8 (Get-PackageFile $build 'build-report.json') | ConvertFrom-Json
if ($report.Schema -ne 1 -or $report.Product -ne 'NetLaneRustPreview' -or $report.PublicReleaseReady -or $report.Signed) {
    throw 'Este verificador aceita somente a prévia Rust sem assinatura.'
}
if ($report.Installer -notmatch '^NetLane-Rust-\d+\.\d+\.\d+-preview-win-x64-setup\.exe$') { throw 'Nome de instalador inválido.' }
if ($report.PSObject.Properties['CompilerInputCharset'] -and $report.CompilerInputCharset -cne 'UTF8') {
    throw 'Charset da compilacao Rust deve ser UTF8.'
}
$setup = Get-PackageFile $build $report.Installer
if ((Get-FileHash -LiteralPath $setup).Hash -ne $report.Sha256) { throw 'Hash do instalador não confere.' }
$required = @('source/NetLaneRust.nsi','source/RustProcessGuard.nsh','source/build-rust-installer.ps1','source/rust-packaging-common.ps1','build-defines.nsh','install-files.nsh','uninstall-files.nsh','validate-payload.nsh','cargo-metadata.json')
$sources = @{}
foreach ($entry in $report.PackagingSources) {
    if ($entry.Path -notin $required -or $sources.ContainsKey($entry.Path)) { throw 'Inventário de fontes inválido.' }
    $sources[$entry.Path] = $true
    if ((Get-FileHash -LiteralPath (Get-PackageFile $build $entry.Path)).Hash -ne $entry.Sha256) { throw "Fonte alterada: $($entry.Path)" }
}
if ($sources.Count -ne $required.Count) { throw 'Fontes da compilação incompletas.' }
$payload = Get-PackageFile $build 'payload'
$manifestFile = Get-PackageFile $payload 'package-manifest.json'
if ((Get-FileHash -LiteralPath $manifestFile).Hash -ne $report.ManifestSha256) { throw 'Manifesto alterado.' }
$manifest = Get-Content -Raw -Encoding UTF8 $manifestFile | ConvertFrom-Json
if ($manifest.Schema -ne 1 -or $manifest.Product -ne $report.Product -or $manifest.Version -ne $report.Version -or
    $manifest.Runtime -ne 'win-x64' -or $manifest.Implementation -ne 'rust' -or $manifest.InstallScope -ne 'current-user' -or $manifest.PublicReleaseReady) {
    throw 'Metadados do pacote não conferem.'
}
$expected = @{'package-manifest.json'=[ordered]@{Sha256=$report.ManifestSha256;Bytes=(Get-Item -LiteralPath $manifestFile).Length}}
foreach ($entry in $manifest.Files) {
    if ($expected.ContainsKey($entry.Path) -or $entry.Sha256 -notmatch '^[A-Fa-f0-9]{64}$' -or $entry.Bytes -le 0) { throw 'Inventário de arquivos inválido/duplicado.' }
    $path = Get-PackageFile $payload $entry.Path
    if ($entry.Path -notin @('netlane.exe','netlane-agent.exe','netlane-rust-installed.layout','rust-package-version.txt','THIRD_PARTY.md','LEIA-ME.md') -and
        $entry.Path -notmatch '^licenses/(cargo-licenses\.json|[A-Za-z0-9_-]+\.txt|crates/[A-Za-z0-9_.+-]+/\d{3}\.txt)$') {
        throw "Arquivo inesperado ou dado pessoal: $($entry.Path)"
    }
    if ((Get-Item -LiteralPath $path).Length -ne $entry.Bytes -or (Get-FileHash -LiteralPath $path).Hash -ne $entry.Sha256) { throw "Payload alterado: $($entry.Path)" }
    $expected[$entry.Path] = $entry
}
foreach ($requiredFile in @('netlane.exe','netlane-agent.exe','netlane-rust-installed.layout','rust-package-version.txt','THIRD_PARTY.md','LEIA-ME.md','licenses/cargo-licenses.json','licenses/NSIS-COPYING.txt')) {
    if (-not $expected.ContainsKey($requiredFile)) { throw "Arquivo obrigatório ausente: $requiredFile" }
}
foreach ($file in Get-FileInventory $payload) { if (-not $expected.ContainsKey($file.Path)) { throw "Payload fora do manifesto: $($file.Path)" } }
if ($expected.Count -ne $report.PayloadFiles) { throw 'Contagem de payload diferente do build.' }
$extracted = Join-Path $build ('archive-check-' + [guid]::NewGuid().ToString('N').Substring(0,8))
& $archiveTool x $setup ('-o' + $extracted) -y -bso0 -bsp0
if ($LASTEXITCODE -ne 0) { throw 'Extração estática falhou.' }
foreach ($relative in $expected.Keys) {
    $path = Get-PackageFile $extracted $relative
    if ((Get-Item -LiteralPath $path).Length -ne $expected[$relative].Bytes -or (Get-FileHash -LiteralPath $path).Hash -ne $expected[$relative].Sha256) {
        throw "Conteúdo diferente no arquivo NSIS: $relative"
    }
}
$nsisExtras = @('$PLUGINSDIR/modern-wizard.bmp','$PLUGINSDIR/System.dll','$PLUGINSDIR/nsDialogs.dll','Uninstall.exe')
foreach ($file in Get-ChildItem -LiteralPath $extracted -Recurse -File) {
    Assert-RegularPath $file.FullName
    $relative = $file.FullName.Substring($extracted.Length+1).Replace('\','/')
    if (-not $expected.ContainsKey($relative) -and $relative -notin $nsisExtras) { throw "Arquivo extra no NSIS: $relative" }
}
function Read-Pe([string]$Path, [int]$Machine) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or [BitConverter]::ToUInt16($bytes,0) -ne 0x5A4D) { throw 'Cabeçalho DOS inválido.' }
    $offset = [BitConverter]::ToInt32($bytes,0x3C)
    if ($offset -lt 64 -or $offset -gt $bytes.Length-26 -or [BitConverter]::ToUInt32($bytes,$offset) -ne 0x4550 -or
        [BitConverter]::ToUInt16($bytes,$offset+4) -ne $Machine) { throw "Arquitetura PE inesperada: $Path" }
    return [Text.Encoding]::UTF8.GetString($bytes)
}
foreach ($name in @('netlane.exe','netlane-agent.exe')) {
    $text = Read-Pe (Get-PackageFile $extracted $name) 0x8664
    if ($name -eq 'netlane.exe' -and $text -notmatch 'requestedExecutionLevel\s+level="asInvoker"') { throw 'Manifesto da UI não é asInvoker.' }
    if ($text -match 'requestedExecutionLevel\s+level="(requireAdministrator|highestAvailable)"') { throw 'Executável pede elevação automática.' }
}
$pe = Get-PeSummary (Get-PackageFile $extracted 'netlane.exe')
if ((@($pe.Imports | Sort-Object) -join ',') -ne (@($manifest.Prerequisites.ImportedDlls | Sort-Object) -join ',') -or
    $manifest.Prerequisites.VisualCppX64Minimum -ne $report.Prerequisites.VisualCppX64Minimum -or
    $manifest.Prerequisites.RuntimeBundled -or $manifest.Prerequisites.RuntimeInstalled) { throw 'Pré-requisitos CRT inconsistentes.' }
foreach ($path in @($setup,(Get-PackageFile $extracted 'Uninstall.exe'))) {
    if ((Read-Pe $path 0x14c) -notmatch 'requestedExecutionLevel\s+level="asInvoker"') { throw 'Instalador/desinstalador não é por usuário.' }
}
if ((Get-Content -Raw -LiteralPath (Get-PackageFile $extracted 'netlane-rust-installed.layout')) -cne '1' -or
    (Get-Content -Raw -LiteralPath (Get-PackageFile $extracted 'rust-package-version.txt')) -cne $report.Version) { throw 'Marcador/versão diferentes.' }
$licenses = Get-Content -Raw -Encoding UTF8 (Get-PackageFile $extracted 'licenses/cargo-licenses.json') | ConvertFrom-Json
if ($licenses.Schema -ne 1 -or $licenses.Target -ne 'x86_64-pc-windows-msvc' -or $licenses.PublicReleaseReady -or
    $licenses.Packages.Count -ne $report.LicenseReview.Packages) { throw 'Inventário de licenças inconsistente.' }
$licensePackages = @{}
foreach ($package in $licenses.Packages) {
    $key = "$($package.Name)@$($package.Version)"
    if ($licensePackages.ContainsKey($key)) { throw 'Crate duplicada nas licenças.' }
    $licensePackages[$key] = $true
    foreach ($notice in $package.Notices) {
        $relative = 'licenses/' + $notice.Path
        if (-not $expected.ContainsKey($relative) -or $expected[$relative].Sha256 -ne $notice.Sha256) { throw 'Aviso de licença ausente/diferente.' }
    }
}
$signature = Get-AuthenticodeSignature -LiteralPath $setup
if ($signature.Status -ne 'NotSigned') { throw 'Assinatura inesperada; este build não autoriza assinatura.' }
$result = [ordered]@{
    Passed=$true; VerifiedPayloadFiles=$expected.Count; PackagingSourcesVerified=$true; X64Executables=2
    InstallerSha256=$report.Sha256; ManifestSha256=$report.ManifestSha256
    InstallerPerUser=$true; UiAsInvoker=$true; SignatureStatus=$signature.Status.ToString()
    LicensePackages=$licenses.Packages.Count; LicenseIssues=@($licenses.Issues).Count
    VisualCppX64Minimum=$report.Prerequisites.VisualCppX64Minimum
    InstallerExecuted=$false; RuntimeQaVerified=$false; PublicReleaseReady=$false
}
Write-PackageJson (Join-Path $extracted 'verification.json') $result
$result | ConvertTo-Json
