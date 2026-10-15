param(
    [string]$OutputDirectory = ('artifacts/rust-preview-' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
    [switch]$SkipChecks,
    # Optional Authenticode signing (SimplySign); never an implicit certificate.
    [string]$CertificateThumbprint,
    [string]$SignToolPath,
    [ValidatePattern('^https?://[A-Za-z0-9.-]+(:[0-9]+)?(/[A-Za-z0-9./_-]*)?$')][string]$TimestampServer = 'http://time.certum.pl',
    # Official portable: signed, all CI checks, committed tree, user-facing LEIA-ME, no demo shortcut.
    [switch]$Release
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cargoManifest = Get-Content -Raw -Encoding UTF8 (Join-Path $root 'Cargo.toml')
$versionMatch = [regex]::Match($cargoManifest, '(?m)^version = "(\d+\.\d+\.\d+)"\r?$')
if (-not $versionMatch.Success) { throw 'Versão do workspace inválida.' }
$version = $versionMatch.Groups[1].Value
$commit = $null
if ($Release) {
    if ($SkipChecks) { throw 'O portátil oficial não pula verificações.' }
    if ([string]::IsNullOrWhiteSpace($CertificateThumbprint)) { throw 'O portátil oficial exige assinatura (CertificateThumbprint e SignToolPath).' }
    $pending = & git -C $root status --porcelain
    if ($LASTEXITCODE -ne 0) { throw 'Não foi possível ler o estado do Git.' }
    if ($pending) { throw 'O portátil oficial exige árvore limpa: faça commit ou descarte as alterações antes.' }
    $commit = (& git -C $root rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Não foi possível ler o commit atual.' }
    if (-not $PSBoundParameters.ContainsKey('OutputDirectory')) { $OutputDirectory = "artifacts/NetLane-$version-portatil-win-x64" }
}
$output = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
$artifactRoot = [IO.Path]::GetFullPath((Join-Path $root 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $output.StartsWith($artifactRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'A prévia deve ficar sob artifacts.' }
if (Test-Path -LiteralPath $output) { throw 'Escolha uma pasta nova; builds anteriores são preservados.' }
$archive = $output + '.zip'
if (Test-Path -LiteralPath $archive) { throw 'Arquivo ZIP já existe; builds anteriores são preservados.' }
$signing = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
if ($signing) {
    if ($CertificateThumbprint -notmatch '^[A-Fa-f0-9]{40}$' -or -not $SignToolPath) { throw 'Informe thumbprint de 40 dígitos hexadecimais e SignToolPath.' }
    $CertificateThumbprint = $CertificateThumbprint.ToUpperInvariant()
} elseif ($SignToolPath) { throw 'SignToolPath exige CertificateThumbprint; não há seleção automática de certificado.' }
Push-Location $root
try {
    if (-not $SkipChecks) {
        & cargo fmt --all -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatação falhou.' }
        & cargo test --workspace --locked
        if ($LASTEXITCODE -ne 0) { throw 'Testes Rust falharam.' }
        & cargo clippy --workspace --all-targets --locked -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy falhou.' }
        if ($Release) {
            # The CI steps that the preview flow does not run.
            & cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
            if ($LASTEXITCODE -ne 0) { throw 'Clippy com todas as features falhou.' }
            & cargo test -p netlane-app --locked --features portable portable_build
            if ($LASTEXITCODE -ne 0) { throw 'Teste do build portátil falhou.' }
        }
    }
    # Portable edition: the UI is compiled without elevation and the agent is not shipped.
    # It runs from user-writable folders, where an elevated helper could be replaced or
    # receive a planted DLL; routing and default-connection changes need the installer.
    & cargo build -p netlane-app --release --locked --features portable
    if ($LASTEXITCODE -ne 0) { throw 'Build Rust portátil falhou.' }
    New-Item -ItemType Directory -Path $output | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'artifacts/rust-target/release/netlane.exe') -Destination (Join-Path $output 'netlane.exe')
    # Restore the shared release binaries so a later installer build never packages the portable UI.
    & cargo build --workspace --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Build Rust falhou.' }
    $readme = if ($Release) { 'docs/leia-me-portatil.md' } else { 'docs/migracao-rust.md' }
    Copy-Item -LiteralPath (Join-Path $root $readme) -Destination (Join-Path $output 'LEIA-ME.md')
    Copy-Item -LiteralPath (Join-Path $root 'THIRD_PARTY.md') -Destination $output
    New-Item -ItemType Directory -Path (Join-Path $output 'licenses') | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'assets/themes/LICENSES') -Destination (Join-Path $output 'licenses/themes.txt')
    Copy-Item -LiteralPath (Join-Path $root 'assets/icons/LICENSE') -Destination (Join-Path $output 'licenses/material-symbols.txt')
    Copy-Item -LiteralPath (Join-Path $root 'assets/fonts/jetbrains-mono/OFL.txt') -Destination (Join-Path $output 'licenses/jetbrains-mono.txt')
    Copy-Item -LiteralPath (Join-Path $root 'assets/licenses/GPUI-APACHE.txt') -Destination (Join-Path $output 'licenses/gpui.txt')
    Copy-Item -LiteralPath (Join-Path $root 'assets/licenses/native-tls-MIT.txt') -Destination (Join-Path $output 'licenses/native-tls.txt')
    Copy-Item -LiteralPath (Join-Path $root 'assets/licenses/schannel-MIT.txt') -Destination (Join-Path $output 'licenses/schannel.txt')
    Copy-Item -LiteralPath (Join-Path $root 'assets/licenses/httparse-MIT.txt') -Destination (Join-Path $output 'licenses/httparse.txt')
    if (-not $Release) {
        [IO.File]::WriteAllText((Join-Path $output 'Abrir demonstracao.cmd'), '@echo off' + "`r`n" + 'start "" "%~dp0netlane.exe" --demo' + "`r`n", [Text.Encoding]::ASCII)
    }
    $signature = $null
    if ($signing) {
        # Sign before hashing; the receipt and the ZIP describe the signed executable.
        & (Join-Path $PSScriptRoot 'sign-windows-artifacts.ps1') -FilePath (Join-Path $output 'netlane.exe') -CertificateThumbprint $CertificateThumbprint -SignToolPath $SignToolPath -TimestampServer $TimestampServer
        if (-not $?) { throw 'Falha na assinatura do netlane.exe portátil.' }
        $signature = Get-AuthenticodeSignature -LiteralPath (Join-Path $output 'netlane.exe')
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $CertificateThumbprint -or $null -eq $signature.TimeStamperCertificate) { throw 'Assinatura do netlane.exe portátil não confirmada.' }
    }
    $hashes = @(Get-FileHash -Path (Join-Path $output '*.exe') -Algorithm SHA256 | Select-Object @{Name='Name';Expression={Split-Path -Leaf $_.Path}},Hash)
    @{Version=$version;Release=[bool]$Release;Commit=$commit;ChecksSkipped=[bool]$SkipChecks;Installer=$false;Signed=$signing;CertificateThumbprint=$(if ($signing) { $CertificateThumbprint });TimestampServer=$(if ($signing) { $TimestampServer });Portable=$true;Elevation=$false;Files=$hashes} |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'build-receipt.json') -Encoding UTF8
    Compress-Archive -Path (Join-Path $output '*') -DestinationPath $archive
    $archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    [IO.File]::WriteAllText($archive + '.sha256', $archiveHash + '  ' + [IO.Path]::GetFileName($archive) + "`r`n", [Text.Encoding]::ASCII)
    Write-Output $(if ($Release) { "Portátil oficial $version ($commit) criado: $output" } else { "Prévia portátil criada: $output" })
    Write-Output "ZIP: $archive (SHA-256 $archiveHash)$(if ($signing) { '; netlane.exe assinado' } else { '; sem assinatura' })"
    Write-Output 'Sem agente elevado: roteamento por aplicativo e conexão padrão exigem a versão instalada.'
    if (-not $Release) { Write-Output 'Abra Abrir demonstracao.cmd para explorar com dados fictícios.' }
} finally { Pop-Location }
