# Only fresh QUIC observations or final read-only verification; no policies, UI inputs or elevation.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RunDirectory,
    [Parameter(Mandatory = $true)][ValidateSet('visible','hidden-1','hidden-2','restored','final','verify')][string]$Stage
)
$ErrorActionPreference = 'Stop'
$repoDirectory = Split-Path -Parent $PSScriptRoot
$run = (Resolve-Path -LiteralPath $RunDirectory).Path
$prefix = (Join-Path $repoDirectory 'artifacts\tray-routing') + [IO.Path]::DirectorySeparatorChar
if (-not $run.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) { throw 'Diretorio fora dos ensaios de bandeja.' }
$requestPath = Join-Path $run 'request.json'
$requestHash = (Get-FileHash -LiteralPath $requestPath -Algorithm SHA256).Hash
$request = Get-Content -LiteralPath $requestPath -Raw -Encoding UTF8 | ConvertFrom-Json
$launch = Get-Content -LiteralPath (Join-Path $run 'window-launch.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($launch.RequestSha256 -ne $requestHash) { throw 'O pedido mudou desde a abertura da janela.' }
$reviewExe = Join-Path $request.BuildRoot 'bin\NetLane.TrayRoutingReview\release\NetLane.TrayRoutingReview.exe'
if ($Stage -eq 'verify') { $arguments = '--verify "' + $requestPath + '" ' + $requestHash }
else { $arguments = '--measure "' + $requestPath + '" ' + $requestHash + ' ' + $Stage }
$process = Start-Process -FilePath $reviewExe -ArgumentList $arguments -WindowStyle Hidden -PassThru -Wait
$code = $process.ExitCode
$process.Dispose()
if ($code -ne 0) { throw "Fase $Stage nao aprovada (codigo $code). Consulte os recibos de erro em $run" }
Write-Output "Fase $Stage aprovada. Recibos: $run"
