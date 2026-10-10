[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$BundleDirectory)
# Validate local bundle hashes and prove both collector modes refuse the host before querying it.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'rust-packaging-common.ps1')
$root = Split-Path -Parent $PSScriptRoot
$bundle = Get-ArtifactPath $root $BundleDirectory
$inventory = Get-Content -Raw -Encoding UTF8 (Join-Path $bundle 'bundle-manifest.json') | ConvertFrom-Json
if ($inventory.Schema -ne 1 -or $inventory.Product -ne 'NetLaneRustPreview' -or $inventory.InstallerExecuted -or
    $inventory.FilesTransferred -or $inventory.GuestIdentityVerified -or $inventory.SnapshotVerified) { throw 'Bundle nao corresponde a preparacao local.' }
$known = @{}
foreach ($file in $inventory.Files) {
    $path = Get-PackageFile $bundle $file.Path
    if ($known.ContainsKey($file.Path) -or (Get-FileHash -LiteralPath $path).Hash -ne $file.Sha256 -or
        (Get-Item -LiteralPath $path).Length -ne $file.Bytes) { throw 'Inventario do bundle alterado/duplicado.' }
    $known[$file.Path] = $true
}
foreach ($file in Get-FileInventory $bundle) {
    if ($file.Path -ne 'bundle-manifest.json' -and -not $known.ContainsKey($file.Path)) { throw 'Arquivo extra no bundle.' }
}
$planPath = Join-Path $bundle 'plan.json'
$plan = Get-Content -Raw -Encoding UTF8 $planPath | ConvertFrom-Json
if ($plan.HostComputer -ne $env:COMPUTERNAME -or $plan.RuntimeExecutionAuthorized -or $plan.Target.Snapshot.Verified) { throw 'Plano de preparacao nao esta incompleto/host correto.' }
$collector = Join-Path $bundle 'scripts/capture-rust-installer-lab-state.ps1'
if ((Get-FileHash -LiteralPath $collector).Hash -ne (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'capture-rust-installer-lab-state.ps1')).Hash) { throw 'Coletor do bundle difere da fonte atual.' }
function Get-CimInstance { throw 'QUERY_REACHED_BEFORE_HOST_GUARD' }
function Get-NetAdapter { throw 'QUERY_REACHED_BEFORE_HOST_GUARD' }
$before = @(Get-FileInventory $bundle) | ConvertTo-Json -Depth 5 -Compress
$cases = @()
foreach ($identify in @($true,$false)) {
    $caught = $null
    try { & $collector -PlanPath $planPath -IdentifyOnly:$identify | Out-Null }
    catch { $caught = $_.Exception.Message }
    if ($caught -cne 'Execucao no host recusada.') { throw "Recusa do host incorreta: $caught" }
    $cases += [ordered]@{IdentifyOnly=$identify;Rejected=$true;Reason=$caught;GuestQueryAttempted=$false}
}
$after = @(Get-FileInventory $bundle) | ConvertTo-Json -Depth 5 -Compress
if ($before -cne $after) { throw 'A recusa do host alterou o bundle.' }
$output = Get-ArtifactPath $root ('artifacts/rust-lab-host-guard-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $output | Out-Null
Write-PackageJson (Join-Path $output 'result.json') ([ordered]@{
    Passed=$true;BundleFilesVerified=$known.Count;Cases=$cases;BundlePreserved=$true
    InstallerExecuted=$false;GuestQueryAttempted=$false;FilesTransferred=$false;RuntimeQaVerified=$false
})
Write-Output "Dois modos recusaram o host antes de consultar o convidado: $output"
