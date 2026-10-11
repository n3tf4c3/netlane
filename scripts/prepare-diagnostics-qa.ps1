param(
    [string]$OutputDirectory = ('artifacts/diagnostics-qa-' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
    [ValidateRange(2000,500000)][int]$Records = 70000,
    [ValidateRange(64,65535)][int]$PacketBytes = 2112
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'rust-packaging-common.ps1')
. (Join-Path $PSScriptRoot 'diagnostics-qa-common.ps1')
$output = Get-ArtifactPath $root $OutputDirectory
Assert-QaLocalPath $output
if (Test-Path -LiteralPath $output) { throw 'Escolha uma pasta nova; candidatos anteriores são preservados.' }
if ($Records % 2 -or 24L + [long]$Records * (16L + $PacketBytes) -gt 1GB) { throw 'Fixtures exigem quantidade par e arquivo de até 1 GiB.' }

# Bind the build to actual source hashes, including local QA work. HEAD alone is insufficient.
Push-Location $root
try {
    $baseCommit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Git HEAD indisponível.' }
    $gitState = @(& git status --porcelain --untracked-files=all)
    if ($LASTEXITCODE -ne 0) { throw 'Git status falhou.' }
    $sources = @(& git ls-files -- Cargo.toml Cargo.lock .cargo/config.toml crates assets THIRD_PARTY.md)
    if ($LASTEXITCODE -ne 0) { throw 'Lista de fontes falhou.' }
    $localSources = @(& git ls-files --others --exclude-standard -- crates)
    if ($LASTEXITCODE -ne 0) { throw 'Lista de fontes locais falhou.' }
    $sources += @($localSources | Where-Object { $_ -match '\.(rs|toml)$' })
} finally { Pop-Location }
$sources += @('crates/netlane-capture/src/qa.rs', 'crates/netlane-capture/src/bin/netlane-qa.rs',
    'scripts/prepare-diagnostics-qa.ps1', 'scripts/test-diagnostics-qa.ps1', 'scripts/open-diagnostics-qa.ps1',
    'scripts/diagnostics-qa-common.ps1', 'scripts/build-rust-preview.ps1', 'scripts/rust-packaging-common.ps1', 'docs/qa-diagnostico-rede.md',
    'scripts/test-native-diagnostics-qa.ps1', 'scripts/diagnostics-interop.py', 'scripts/qa-network-tools.Dockerfile', 'scripts/measure-capture-throughput.ps1')
$sourceHashes = @($sources | Sort-Object -Unique | ForEach-Object {
    $path = Get-QaFile $root $_
    [pscustomobject]@{Path=$_;Sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}
})

New-Item -ItemType Directory -Path $output | Out-Null
$previewRelative = $output.Substring($root.TrimEnd('\','/').Length + 1).Replace('\','/') + '/application'
& (Join-Path $PSScriptRoot 'build-rust-preview.ps1') -OutputDirectory $previewRelative
# Replace the preview's shared demo shortcut with launchers that create isolated QA sessions.
Remove-Item -LiteralPath (Get-QaFile $output 'application/Abrir demonstracao.cmd')
Copy-Item -LiteralPath (Join-Path $root 'artifacts/rust-target/release/netlane-qa.exe') -Destination (Join-Path $output 'application/netlane-qa.exe')
New-Item -ItemType Directory -Path (Join-Path $output 'scripts') | Out-Null
foreach ($name in @('diagnostics-qa-common.ps1','test-diagnostics-qa.ps1','open-diagnostics-qa.ps1')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $output ('scripts/' + $name))
}
Copy-Item -LiteralPath (Join-Path $root 'docs/qa-diagnostico-rede.md') -Destination (Join-Path $output 'LEIA-ME.md')
foreach ($launcher in @(@('Abrir QA demonstracao.cmd','open-diagnostics-qa.ps1 -Mode Demo'),@('Executar QA offline.cmd','test-diagnostics-qa.ps1'))) {
    $parts = $launcher[1].Split(' ',2)
    $arguments = if ($parts.Count -eq 2) { ' ' + $parts[1] } else { '' }
    [IO.File]::WriteAllText((Get-QaFile $output $launcher[0]), '@echo off' + "`r`n" +
        'powershell.exe -NoProfile -File "%~dp0scripts\' + $parts[0] + '"' + $arguments + "`r`n" + 'pause' + "`r`n", [Text.Encoding]::ASCII)
}
& (Join-Path $output 'application/netlane-qa.exe') --generate-fixtures (Join-Path $output 'fixtures') --records $Records --packet-bytes $PacketBytes
if ($LASTEXITCODE -ne 0) { throw 'Geração das fixtures falhou; candidato incompleto.' }
$cases = @(1..12 | ForEach-Object { [ordered]@{Id=('QA-{0:D2}' -f $_);Status='pending';StartedUtc=$null;EndedUtc=$null;Target=$null;Evidence=@();Notes=$null} })
Write-QaJson (Join-Path $output 'manual-cases.json') ([ordered]@{Schema=1;Cases=$cases;Instructions='Copie para results/ da sessão antes de preencher. Consulte LEIA-ME.md; não marque casos reais pela execução offline.'})
foreach ($entry in $sourceHashes) {
    if ((Get-FileHash -LiteralPath (Get-QaFile $root $entry.Path)).Hash -ne $entry.Sha256) { throw "Fonte mudou durante o build: $($entry.Path)" }
}
$version = [regex]::Match((Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw), '(?m)^version = "([0-9.]+)"\r?$').Groups[1].Value
if (-not $version) { throw 'Versão do workspace indisponível.' }
Write-QaJson (Join-Path $output 'qa-package.json') ([ordered]@{
    Schema=1;Version=$version;CreatedUtc=[DateTime]::UtcNow.ToString('o');BaseCommit=$baseCommit
    SourceIsClean=($gitState.Count -eq 0);GitState=$gitState;SourceFiles=$sourceHashes
    PrivateQaOnly=$true;SyntheticFixtures=$true;Installer=$false;Signed=$false;ChecksSkipped=$false
    Files=@(Get-FileInventory $output)
})
$null = Read-QaPackage $output
Write-Output "Kit privado de QA criado: $output"
Write-Output 'Execute scripts/test-diagnostics-qa.ps1 para o ensaio offline. Casos reais continuam pendentes.'
