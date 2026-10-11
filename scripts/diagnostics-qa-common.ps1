# Portable integrity helpers. Importing this file performs no workload.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-QaLocalPath([string]$Path) {
    if (-not [IO.Path]::IsPathRooted($Path) -or $Path.StartsWith('\\') -or $Path.Contains('"')) {
        throw 'Use caminho absoluto local, sem UNC ou aspas.'
    }
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Link/junção recusado: $current" }
        }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}

function Get-QaFile([string]$Root, [string]$Relative) {
    if ($Relative -notmatch '^[A-Za-z0-9_ .()+/-]+$' -or $Relative -match '(^/|//|(^|/)\.{1,2}(/|$)|[ .](/|$))') {
        throw "Caminho de pacote inválido: $Relative"
    }
    $full = [IO.Path]::GetFullPath((Join-Path $Root $Relative))
    if (-not $full.StartsWith($Root.TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Caminho fora do pacote.'
    }
    Assert-QaLocalPath $full
    return $full
}

function Write-QaJson([string]$Path, $Value) {
    $file = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $writer = [IO.StreamWriter]::new($file, [Text.UTF8Encoding]::new($false))
    try { $writer.WriteLine(($Value | ConvertTo-Json -Depth 20)); $writer.Flush(); $file.Flush($true) }
    finally { $writer.Dispose() }
}

function Read-QaPackage([string]$Root) {
    Assert-QaLocalPath $Root
    $file = Get-QaFile $Root 'qa-package.json'
    if ((Get-Item -LiteralPath $file).Length -gt 4MB) { throw 'Manifesto do pacote grande demais.' }
    $data = Get-Content -LiteralPath $file -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($data.Schema -ne 1 -or -not $data.PrivateQaOnly -or -not $data.SyntheticFixtures -or
        $data.Files.Count -lt 5 -or $data.Files.Count -gt 10000) { throw 'Manifesto do kit incompatível.' }
    $seen = @{}
    foreach ($entry in $data.Files) {
        if ($seen.ContainsKey($entry.Path) -or $entry.Sha256 -notmatch '^[A-Fa-f0-9]{64}$') { throw 'Entrada duplicada ou hash inválido.' }
        $seen[$entry.Path] = $true
        $path = Get-QaFile $Root $entry.Path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or
            (Get-Item -LiteralPath $path).Length -ne $entry.Bytes -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw "Integridade divergente: $($entry.Path)" }
    }
    foreach ($required in @('application/netlane.exe', 'application/netlane-agent.exe', 'application/netlane-qa.exe',
        'fixtures/expected.json', 'manual-cases.json', 'scripts/test-diagnostics-qa.ps1', 'scripts/open-diagnostics-qa.ps1', 'scripts/diagnostics-qa-common.ps1')) {
        if (-not $seen.ContainsKey($required)) { throw "Arquivo obrigatório ausente: $required" }
    }
    # Results are mutable evidence. Everything else must belong to the inventory,
    # including adjacent DLLs that could change the behavior of a packaged executable.
    foreach ($item in Get-ChildItem -LiteralPath $Root -Force) {
        if ($item.Name -eq 'results') { Assert-QaLocalPath $item.FullName; continue }
        Assert-QaLocalPath $item.FullName
        $files = if ($item.PSIsContainer) { @(Get-ChildItem -LiteralPath $item.FullName -Recurse -File -Force) } else { @($item) }
        foreach ($candidate in $files) {
            Assert-QaLocalPath $candidate.FullName
            $relative = $candidate.FullName.Substring($Root.TrimEnd('\','/').Length+1).Replace('\','/')
            if ($relative -ne 'qa-package.json' -and -not $seen.ContainsKey($relative)) { throw "Arquivo fora do manifesto: $relative" }
        }
    }
    return $data
}
