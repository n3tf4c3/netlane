param([string]$SourceDirectory = 'assets/brand', [string]$OutputDirectory = 'assets/brand')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$dependencies = Join-Path $projectRoot 'artifacts/rust-target/release/deps'
$library = Get-ChildItem -LiteralPath $dependencies -Filter 'libresvg-*.rlib' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
if (!$library) { throw 'Compile primeiro a versão Rust Release para disponibilizar o resvg usado pelo GPUI.' }
$workDirectory = Join-Path $projectRoot 'artifacts/brand-tools'
New-Item -ItemType Directory -Path $workDirectory -Force | Out-Null
$tool = Join-Path $workDirectory 'render-brand.exe'
& rustc --edition 2024 (Join-Path $projectRoot 'tools/render-brand.rs') --extern "resvg=$($library.FullName)" -L "dependency=$dependencies" -o $tool
if ($LASTEXITCODE -ne 0) { throw 'Falha ao compilar o conversor de identidade visual.' }
& $tool (Join-Path $projectRoot $SourceDirectory) (Join-Path $projectRoot $OutputDirectory)
if ($LASTEXITCODE -ne 0) { throw 'Falha ao renderizar os SVGs fornecidos.' }
