param(
    [string]$PackageDirectory = (Split-Path -Parent $PSScriptRoot),
    [ValidateSet('Demo','Lab')][string]$Mode = 'Demo',
    [string]$TargetLabel
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'diagnostics-qa-common.ps1')
$package = [IO.Path]::GetFullPath($PackageDirectory)
$manifest = Read-QaPackage $package
if ($Mode -eq 'Lab' -and [string]::IsNullOrWhiteSpace($TargetLabel)) { throw 'Identifique o PC autorizado com -TargetLabel no modo Lab.' }
$session = Get-QaFile $package ('results/' + $Mode.ToLowerInvariant() + '-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $session | Out-Null
Copy-Item -LiteralPath (Get-QaFile $package 'manual-cases.json') -Destination (Join-Path $session 'manual-cases.json')
Write-QaJson (Join-Path $session 'session.json') ([ordered]@{
    Schema=1;Version=$manifest.Version;PackageManifestSha256=(Get-FileHash -LiteralPath (Join-Path $package 'qa-package.json')).Hash
    Mode=$Mode;TargetLabel=$TargetLabel;CreatedUtc=[DateTime]::UtcNow.ToString('o');Profile='profile';ManualCasesApproved=$false
})
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = Get-QaFile $package 'application/netlane.exe'
$start.WorkingDirectory = $package
$start.Arguments = '--profile "' + (Join-Path $session 'profile') + '"'
if ($Mode -eq 'Demo') { $start.Arguments += ' --demo' }
$start.UseShellExecute = $false
$process = [Diagnostics.Process]::Start($start)
try { Write-Output "NetLane iniciado (PID $($process.Id)), sessão $session" } finally { $process.Dispose() }
Write-Output 'Perfil exclusivo. Capture/login/envio exigem ações explícitas na interface; registre os casos manuais nesta sessão.'
