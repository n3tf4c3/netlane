[CmdletBinding()]
param([ValidateSet('simd_helpers','seahash')][string]$CrateName='simd_helpers')
# Offline fixtures for license provenance. No Cargo build, installer or guest execution.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'rust-packaging-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$metadataRaw = & cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata falhou.' }
$metadata = ($metadataRaw -join "`n") | ConvertFrom-Json
$version = if ($CrateName -eq 'seahash') { '4.1.0' } else { '0.1.0' }
$package = @($metadata.packages | Where-Object { $_.name -eq $CrateName -and $_.version -eq $version })
if ($package.Count -ne 1) { throw 'Crate da fixture nao encontrada.' }
# Keep the real restored crate and publisher revision; isolate just its inventory graph.
$fixtureMetadataJson = [ordered]@{
    packages=$package; workspace_members=@($package[0].id)
    resolve=[ordered]@{nodes=@([ordered]@{id=$package[0].id;deps=@()})}
} | ConvertTo-Json -Depth 25 -Compress
function cargo { $fixtureMetadataJson }
$directory = Get-ArtifactPath $root ('artifacts/rust-license-guards-' + $CrateName + '-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $directory | Out-Null
$entry = (Get-Content -Raw -Encoding UTF8 (Join-Path $root 'assets/licenses/rust-supplements.json') | ConvertFrom-Json).Packages | Where-Object { $_.Name -eq $CrateName -and $_.Version -eq $version }
$results = [Collections.Generic.List[object]]::new()
function New-Fixture([string]$Name) {
    $fixture = Join-Path $directory $Name
    New-Item -ItemType Directory -Path (Join-Path $fixture 'build') -Force | Out-Null
    $supplement = $entry | ConvertTo-Json -Depth 15 | ConvertFrom-Json
    foreach ($relative in @($supplement.Notices[0].Path,$supplement.LicenseOnlyFollowup.EvidencePath)) {
        $target = Join-Path $fixture ('assets/licenses/' + $relative)
        New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $root ('assets/licenses/' + $relative)) -Destination $target
    }
    foreach ($relative in @('Cargo.lock','assets/themes/LICENSES','assets/themes/one/LICENSE','assets/themes/one-dark-pro/LICENSE','assets/themes/ayu/LICENSE','assets/themes/gruvbox/LICENSE','assets/icons/LICENSE','assets/fonts/jetbrains-mono/OFL.txt')) {
        $target = Join-Path $fixture $relative
        New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $root $relative) -Destination $target
    }
    Write-PackageJson (Join-Path $fixture 'assets/licenses/rust-supplements.json') ([ordered]@{Schema=1;Packages=@($supplement)})
    return $fixture
}
function Change-Proof([string]$Fixture,[scriptblock]$Edit) {
    $jsonPath = Join-Path $Fixture 'assets/licenses/rust-supplements.json'
    $data = Get-Content -Raw -Encoding UTF8 $jsonPath | ConvertFrom-Json
    $proofPath = Join-Path $Fixture ('assets/licenses/' + $data.Packages[0].LicenseOnlyFollowup.EvidencePath)
    $proof = Get-Content -Raw -Encoding UTF8 $proofPath | ConvertFrom-Json
    & $Edit $proof
    Write-PackageJson $proofPath $proof
    $data.Packages[0].LicenseOnlyFollowup.EvidenceSha256 = (Get-FileHash -LiteralPath $proofPath).Hash
    Write-PackageJson $jsonPath $data
}
function Expect-Rejection([string]$Name,[string]$Fixture,[string]$Pattern) {
    $caught = $null
    try { $null = Export-RustLicenses $Fixture (Join-Path $Fixture 'build') (Join-Path $Fixture 'licenses') }
    catch { $caught = $_.Exception.Message }
    if (-not $caught -or $caught -notmatch $Pattern) { throw "Recusa incorreta em $Name : $caught" }
    $results.Add([ordered]@{Case=$Name;Rejected=$true;Reason=$caught})
}
$fixture = New-Fixture 'publisher-license-only-child'
$valid = Export-RustLicenses $fixture (Join-Path $fixture 'build') (Join-Path $fixture 'licenses')
if ($valid.Packages -ne 1 -or $valid.Notices -ne 1 -or @($valid.Issues).Count -ne 0 -or $valid.PublicReleaseReady) { throw 'Suplemento valido nao conferiu.' }
$results.Add([ordered]@{Case='publisher-license-only-child';Accepted=$true;PublicReleaseReady=$false})
$fixture = New-Fixture 'evidence-changed'
Add-Content -LiteralPath (Join-Path $fixture ('assets/licenses/' + $entry.LicenseOnlyFollowup.EvidencePath)) -Value ' '
Expect-Rejection 'evidence-changed' $fixture 'Evidência da licença posterior alterada'
$fixture = New-Fixture 'wrong-parent'
Change-Proof $fixture { param($proof) $proof.parents[0].sha = '0000000000000000000000000000000000000000' }
Expect-Rejection 'wrong-parent' $fixture 'única adição no filho direto'
$fixture = New-Fixture 'source-changed'
Change-Proof $fixture { param($proof) $proof.files += [pscustomobject]@{filename='src/lib.rs';status='modified';sha='0000000000000000000000000000000000000000'} }
Expect-Rejection 'source-changed' $fixture 'única adição no filho direto'
$fixture = New-Fixture 'wrong-git-blob'
Change-Proof $fixture { param($proof) $proof.files[0].sha = '0000000000000000000000000000000000000000' }
Expect-Rejection 'wrong-git-blob' $fixture 'Texto difere do blob|Adição de LICENSE não confere'
$fixture = New-Fixture 'notice-changed'
Add-Content -LiteralPath (Join-Path $fixture ('assets/licenses/' + $entry.Notices[0].Path)) -Value 'changed fixture'
Expect-Rejection 'notice-changed' $fixture 'Texto suplementar alterado'
if ($CrateName -eq 'seahash') {
    foreach ($objectName in @('ChildCommitBase64','ParentCommitBase64','ChildTreeBase64','ParentTreeBase64')) {
        $fixture = New-Fixture ('raw-object-' + $objectName)
        Change-Proof $fixture {
            param($proof)
            $bytes = [Convert]::FromBase64String($proof.RawObjects.$objectName)
            $bytes[$bytes.Length-1] = $bytes[$bytes.Length-1] -bxor 1
            $proof.RawObjects.$objectName = [Convert]::ToBase64String($bytes)
        }
        Expect-Rejection ('raw-object-' + $objectName) $fixture 'Objeto (commit|tree) Git difere'
    }
    $fixture = New-Fixture 'wrong-source-url'
    Change-Proof $fixture { param($proof) $proof.SourceUrl = 'https://example.invalid/commit' }
    Expect-Rejection 'wrong-source-url' $fixture 'Origem do recibo Git inválida'
}
$result = [ordered]@{Passed=$true;Crate=$CrateName;Cases=$results.ToArray();CargoBuildExecuted=$false;InstallerExecuted=$false;NetworkChanged=$false;PublicReleaseReady=$false}
Write-PackageJson (Join-Path $directory 'result.json') $result
Write-Output "$($results.Count) casos de proveniencia aprovados para $CrateName : $directory"
