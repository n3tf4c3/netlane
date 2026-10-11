[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$NsisCompilerPath,
    [string]$VcRuntimeReferencePath,
    [string]$OutputDirectory = ('artifacts/rust-installer-' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
    [switch]$SkipChecks,
    # Optional Authenticode signing through the certificate exposed by SimplySign Desktop.
    [string]$CertificateThumbprint,
    [string]$SignToolPath,
    [ValidatePattern('^https?://[A-Za-z0-9.-]+(:[0-9]+)?(/[A-Za-z0-9./_-]*)?$')][string]$TimestampServer = 'http://time.certum.pl'
)
# Compile/package only. Never runs the installer, apps, UAC or network probes. Signing only
# when a certificate thumbprint is given explicitly; there is no implicit certificate.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'rust-packaging-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$output = Get-ArtifactPath $root $OutputDirectory
if (Test-Path -LiteralPath $output) { throw 'Escolha uma pasta nova; builds anteriores são preservados.' }
$signing = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
$signingScript = Join-Path $PSScriptRoot 'sign-windows-artifacts.ps1'
$signingMetadata = $null
if ($signing) {
    if ($CertificateThumbprint -notmatch '^[A-Fa-f0-9]{40}$' -or -not $SignToolPath) { throw 'Informe thumbprint de 40 dígitos hexadecimais e SignToolPath.' }
    $CertificateThumbprint = $CertificateThumbprint.ToUpperInvariant()
    $SignToolPath = (Resolve-Path -LiteralPath $SignToolPath).Path
    $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
    if (-not $certificate.HasPrivateKey -or $certificate.NotAfter -le (Get-Date)) { throw 'Certificado indisponível ou expirado.' }
    $signingMetadata = [ordered]@{ CertificateThumbprint=$CertificateThumbprint; Publisher=$certificate.Subject; FileDigest='SHA256'; TimestampDigest='SHA256'; TimestampServer=$TimestampServer }
} elseif ($SignToolPath) { throw 'SignToolPath exige CertificateThumbprint; não há seleção automática de certificado.' }
$compiler = (Resolve-Path -LiteralPath $NsisCompilerPath).Path
Assert-RegularPath $compiler
$compilerVersion = (& $compiler /VERSION | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $compilerVersion -ne 'v3.12') { throw 'Use a distribuição portátil NSIS 3.12 conferida.' }
$cargo = Get-Content -Raw -Encoding UTF8 (Join-Path $root 'Cargo.toml')
$match = [regex]::Match($cargo, '(?m)^version = "(\d+\.\d+\.\d+)"\r?$')
if (-not $match.Success) { throw 'Versão do workspace inválida.' }
$version = $match.Groups[1].Value
foreach ($part in $version.Split('.')) { if ([int]$part -gt 65535) { throw 'Versão incompatível com NSIS.' } }
Push-Location $root
try {
    if (-not $SkipChecks) {
        & cargo fmt --all -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatação falhou.' }
        & cargo test --workspace --locked
        if ($LASTEXITCODE -ne 0) { throw 'Testes Rust falharam.' }
        & cargo clippy --workspace --all-targets --locked -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy falhou.' }
    }
    # Explicitly rebuild the common binaries after any gui-test feature build.
    & cargo build --workspace --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Build Rust falhou.' }
    $revision = (& git rev-parse HEAD | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Não foi possível identificar HEAD.' }
} finally { Pop-Location }
$payload = Join-Path $output 'payload'
New-Item -ItemType Directory -Path $payload | Out-Null
foreach ($name in @('netlane.exe','netlane-agent.exe')) {
    $binary = Join-Path $root ('artifacts/rust-target/release/' + $name)
    Assert-RegularPath $binary
    Copy-Item -LiteralPath $binary -Destination $payload
}
if ($signing) {
    # Sign the payload copies before any hash/manifest is computed.
    & $signingScript -FilePath @((Join-Path $payload 'netlane.exe'),(Join-Path $payload 'netlane-agent.exe')) -CertificateThumbprint $CertificateThumbprint -SignToolPath $SignToolPath -TimestampServer $TimestampServer
    if (-not $?) { throw 'Falha na assinatura dos executáveis.' }
}
$pe = Get-PeSummary (Join-Path $payload 'netlane.exe')
if ($pe.Machine -ne 0x8664 -or 'VCRUNTIME140.dll' -notin $pe.Imports) { throw 'Reavalie os pré-requisitos: o layout atual espera CRT dinâmico MSVC x64.' }
if (-not $VcRuntimeReferencePath) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vsPath = (& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $vsPath) { throw 'Informe VcRuntimeReferencePath da redistribuição correspondente ao toolchain.' }
    $redistVersion = (Get-Content -Raw -LiteralPath (Join-Path $vsPath 'VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt')).Trim()
    if ($redistVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Versão VC Redist inválida.' }
    $runtimeFiles = @(Get-ChildItem -LiteralPath (Join-Path $vsPath ('VC/Redist/MSVC/' + $redistVersion + '/x64')) -Filter vcruntime140.dll -Recurse -File)
    if ($runtimeFiles.Count -ne 1) { throw 'Informe VcRuntimeReferencePath explicitamente.' }
    $VcRuntimeReferencePath = $runtimeFiles[0].FullName
}
$VcRuntimeReferencePath = (Resolve-Path -LiteralPath $VcRuntimeReferencePath).Path
Assert-RegularPath $VcRuntimeReferencePath
$runtimeVersion = [version](Get-Item -LiteralPath $VcRuntimeReferencePath).VersionInfo.FileVersion
if ((Get-PeSummary $VcRuntimeReferencePath).Machine -ne 0x8664 -or $runtimeVersion.Major -ne $pe.LinkerMajor -or $runtimeVersion.Minor -lt $pe.LinkerMinor) {
    throw 'A referência CRT deve ser x64 e pelo menos da série do linker usado.'
}
$prerequisites = [ordered]@{ VisualCppX64Minimum=$runtimeVersion.ToString(); ImportedDlls=$pe.Imports; RuntimeBundled=$false; RuntimeInstalled=$false }
$licenseReview = Export-RustLicenses $root $output (Join-Path $payload 'licenses')
Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $compiler) 'COPYING') -Destination (Join-Path $payload 'licenses/NSIS-COPYING.txt')
Copy-Item -LiteralPath (Join-Path $root 'THIRD_PARTY.md') -Destination $payload
Copy-Item -LiteralPath (Join-Path $root 'docs/instalador-rust.md') -Destination (Join-Path $payload 'LEIA-ME.md')
# Created only in the intended installer payload, never in a portable preview.
[IO.File]::WriteAllText((Join-Path $payload 'netlane-rust-installed.layout'), '1', [Text.Encoding]::ASCII)
[IO.File]::WriteAllText((Join-Path $payload 'rust-package-version.txt'), $version, [Text.Encoding]::ASCII)
$manifest = [ordered]@{
    Schema=1; Product='NetLaneRustPreview'; Version=$version; Runtime='win-x64'; Implementation='rust'
    InstallScope='per-machine'; GitCommit=$revision; Files=(Get-FileInventory $payload)
    Prerequisites=$prerequisites
    PublicReleaseReady=$false; LicenseReview=$licenseReview
}
Write-PackageJson (Join-Path $payload 'package-manifest.json') $manifest
$files = Get-FileInventory $payload
$source = Join-Path $output 'source'
New-Item -ItemType Directory -Path $source | Out-Null
foreach ($name in @('NetLaneRust.nsi','RustProcessGuard.nsh')) {
    Copy-Item -LiteralPath (Join-Path $root ('packaging/windows/' + $name)) -Destination $source
}
foreach ($name in @('build-rust-installer.ps1','rust-packaging-common.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $source }
$utf8 = [Text.UTF8Encoding]::new($false)
function Write-Nsis([string]$Name, [string[]]$Lines) { [IO.File]::WriteAllLines((Join-Path $output $Name), $Lines, $utf8) }
function Nsis-Literal([string]$Value) {
    if ($Value.Contains('"') -or $Value.Contains("`r") -or $Value.Contains("`n")) { throw 'Texto incompatível com NSIS.' }
    return $Value.Replace('$', '$$')
}
$installer = 'NetLane-Rust-' + $version + '-preview-win-x64-setup.exe'
$setup = Join-Path $output $installer
Write-Nsis 'build-defines.nsh' @(
    '!define PRODUCT_VERSION "' + $version + '"'
    '!define VC_RUNTIME_MIN "' + $runtimeVersion.ToString() + '"'
    '!define SETUP_OUTPUT "' + (Nsis-Literal $setup) + '"'
    '!define INSTALL_FILES "' + (Nsis-Literal (Join-Path $output 'install-files.nsh')) + '"'
    '!define UNINSTALL_FILES "' + (Nsis-Literal (Join-Path $output 'uninstall-files.nsh')) + '"'
    '!define VALIDATE_PAYLOAD "' + (Nsis-Literal (Join-Path $output 'validate-payload.nsh')) + '"'
    '!define PAYLOAD_KIB ' + [math]::Ceiling(($files | Measure-Object -Property Bytes -Sum).Sum / 1024)
)
if ($signing) {
    $powershell = Join-Path $PSHOME 'powershell.exe'
    foreach ($commandPath in @($powershell, $signingScript, $SignToolPath)) {
        if ($commandPath.IndexOfAny([char[]]@([char]39, [char]34, [char]37, [char]13, [char]10)) -ge 0) { throw 'Caminho incompatível com o comando de assinatura NSIS.' }
    }
    $uninstallerCommand = '"' + $powershell + '" -NoProfile -NonInteractive -File "' + $signingScript + '" -CertificateThumbprint ' + $CertificateThumbprint + ' -SignToolPath "' + $SignToolPath + '" -TimestampServer ' + $TimestampServer + ' -FilePath "%1"'
    [IO.File]::AppendAllText((Join-Path $output 'build-defines.nsh'), ("!define SIGN_UNINSTALLER_COMMAND '" + $uninstallerCommand.Replace('$', '$$') + "'`r`n"), $utf8)
    # Plug-ins extracted to %TEMP% at run time must be signed too (Smart App Control).
    $signedPlugins = Join-Path $output 'signed-plugins'
    New-Item -ItemType Directory -Path $signedPlugins | Out-Null
    $pluginFiles = @(foreach ($plugin in @('System.dll','nsDialogs.dll')) {
        $pluginSource = Join-Path (Split-Path -Parent $compiler) ('Plugins\x86-unicode\' + $plugin)
        Assert-RegularPath $pluginSource
        Copy-Item -LiteralPath $pluginSource -Destination $signedPlugins
        Join-Path $signedPlugins $plugin
    })
    & $signingScript -FilePath $pluginFiles -CertificateThumbprint $CertificateThumbprint -SignToolPath $SignToolPath -TimestampServer $TimestampServer
    if (-not $?) { throw 'Falha na assinatura dos plug-ins NSIS.' }
    [IO.File]::AppendAllText((Join-Path $output 'build-defines.nsh'), ('!define SIGNED_PLUGIN_DIR "' + (Nsis-Literal $signedPlugins) + '"' + "`r`n"), $utf8)
}
$installLines = [Collections.Generic.List[string]]::new()
$uninstallLines = [Collections.Generic.List[string]]::new()
$validateLines = [Collections.Generic.List[string]]::new()
$directories = @('')
foreach ($file in $files) {
    $relative = $file.Path.Replace('/','\')
    $directory = [IO.Path]::GetDirectoryName($relative)
    while ($directory) { $directories += $directory; $directory = [IO.Path]::GetDirectoryName($directory) }
}
$directories = @($directories | Sort-Object -Unique)
foreach ($directory in $directories | Where-Object { $_ }) { $validateLines.Add('!insertmacro CheckPayloadDirectory "' + $directory + '"') }
foreach ($file in $files) { $validateLines.Add('!insertmacro CheckPayloadFile "' + $file.Path.Replace('/','\') + '"') }
# Write repair marker first. Interrupted installations retain their identity.
$orderedFiles = @($files | Sort-Object @{Expression={if ($_.Path -eq 'netlane-rust-installed.layout') {0} else {1}}},Path)
foreach ($file in $orderedFiles) {
    $relative = $file.Path.Replace('/','\')
    $directory = [IO.Path]::GetDirectoryName($relative)
    $destination = if ($directory) { '$INSTDIR\' + $directory } else { '$INSTDIR' }
    $installLines.Add('ClearErrors')
    $installLines.Add('SetOutPath "' + $destination + '"')
    $installLines.Add('IfErrors install_failed')
    $installLines.Add('File "' + (Nsis-Literal (Get-PackageFile $payload $file.Path)) + '"')
    $installLines.Add('IfErrors install_failed')
    if ($relative -notin @('netlane-rust-installed.layout','rust-package-version.txt')) {
        $uninstallLines.Add('ClearErrors')
        $uninstallLines.Add('Delete "$INSTDIR\' + $relative + '"')
        $uninstallLines.Add('IfErrors uninstall_failed')
    }
}
foreach ($directory in $directories | Where-Object { $_ } | Sort-Object Length -Descending) {
    # Nonrecursive RMDir preserves unknown files and is allowed to leave a directory.
    $uninstallLines.Add('RMDir "$INSTDIR\' + $directory + '"')
}
Write-Nsis 'install-files.nsh' $installLines.ToArray()
Write-Nsis 'uninstall-files.nsh' $uninstallLines.ToArray()
Write-Nsis 'validate-payload.nsh' $validateLines.ToArray()
# NSIS defaults BOM-less files to the Windows codepage; our sources/includes are UTF-8.
& $compiler /INPUTCHARSET UTF8 /V2 ('/DBUILD_DEFINES=' + (Join-Path $output 'build-defines.nsh')) (Join-Path $source 'NetLaneRust.nsi')
if ($LASTEXITCODE -ne 0) { throw 'Compilação NSIS falhou; nenhum instalador foi executado.' }
if ($signing) {
    & $signingScript -FilePath $setup -CertificateThumbprint $CertificateThumbprint -SignToolPath $SignToolPath -TimestampServer $TimestampServer
    if (-not $?) { throw 'Falha na assinatura do instalador.' }
}
$inputPaths = @('source/NetLaneRust.nsi','source/RustProcessGuard.nsh','source/build-rust-installer.ps1','source/rust-packaging-common.ps1','build-defines.nsh','install-files.nsh','uninstall-files.nsh','validate-payload.nsh','cargo-metadata.json')
$inputs = @($inputPaths | ForEach-Object { [ordered]@{Path=$_;Sha256=(Get-FileHash -LiteralPath (Join-Path $output $_)).Hash} })
Write-PackageJson (Join-Path $output 'build-report.json') ([ordered]@{
    Schema=1; Product='NetLaneRustPreview'; Version=$version; Installer=$installer
    Sha256=(Get-FileHash -LiteralPath $setup).Hash; ManifestSha256=(Get-FileHash -LiteralPath (Join-Path $payload 'package-manifest.json')).Hash
    PackagingSources=$inputs; PayloadFiles=$files.Count; ChecksSkipped=[bool]$SkipChecks; ReleaseBuild=$true
    CompilerVersion=$compilerVersion; CompilerSha256=(Get-FileHash -LiteralPath $compiler).Hash; CompilerInputCharset='UTF8'
    Prerequisites=$prerequisites; VcRuntimeReferenceSha256=(Get-FileHash -LiteralPath $VcRuntimeReferencePath).Hash
    InstallerExecuted=$false; RuntimeQaVerified=$false; Signed=$signing; Signing=$signingMetadata; PublicReleaseReady=$false; LicenseReview=$licenseReview
})
Write-Output "Instalador Rust compilado para revisão estática: $setup"
Write-Output "Licenças: $($licenseReview.Packages) crates; $($licenseReview.Issues.Count) lacunas registradas. QA runtime e distribuição permanecem pendentes."
