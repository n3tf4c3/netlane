# Shared static packaging helpers. No installation, signing, or product execution.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-RegularPath([string]$Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Link/junção recusado: $current" }
        }
        $parent = [IO.Path]::GetDirectoryName($current)
        if ($parent -eq $current) { break }
        $current = $parent
    }
}

function Get-ArtifactPath([string]$Root, [string]$Path) {
    $candidate = if ([IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $Root $Path }
    $full = [IO.Path]::GetFullPath($candidate)
    $prefix = [IO.Path]::GetFullPath((Join-Path $Root 'artifacts')) + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use uma pasta sob artifacts.' }
    Assert-RegularPath $full
    return $full
}

function Get-PackageFile([string]$Root, [string]$Relative) {
    # Strict portable allowlist syntax, including rejection of ADS and dot segments.
    if ($Relative -notmatch '^[A-Za-z0-9_ .()+/-]+$' -or $Relative -match '(^/|//|(^|/)\.{1,2}(/|$)|[ .](/|$))') {
        throw "Caminho de pacote inválido: $Relative"
    }
    $full = [IO.Path]::GetFullPath((Join-Path $Root $Relative))
    if (-not $full.StartsWith($Root.TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Caminho fora do pacote.'
    }
    Assert-RegularPath $full
    return $full
}

function Write-PackageJson([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 15), [Text.UTF8Encoding]::new($false))
}

function Get-GitObjectHash([string]$Type,[byte[]]$Content) {
    $header = [Text.Encoding]::ASCII.GetBytes("$Type $($Content.Length)`0")
    $objectBytes = New-Object byte[] ($header.Length + $Content.Length)
    [Array]::Copy($header,0,$objectBytes,0,$header.Length)
    [Array]::Copy($Content,0,$objectBytes,$header.Length,$Content.Length)
    $sha = [Security.Cryptography.SHA1]::Create()
    try { return [BitConverter]::ToString($sha.ComputeHash($objectBytes)).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
}

function Read-GitTree([byte[]]$Content) {
    $entries = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
    $offset = 0
    while ($offset -lt $Content.Length) {
        $end = $offset
        while ($end -lt $Content.Length -and $Content[$end] -ne 0) { $end++ }
        if ($end -eq $Content.Length -or $end+21 -gt $Content.Length) { throw 'Objeto tree Git inválido.' }
        $label = [Text.Encoding]::UTF8.GetString($Content,$offset,$end-$offset)
        if ($label -notmatch '^(100644|100755|40000|120000|160000) ([A-Za-z0-9_.-]+)$') { throw 'Entrada tree Git inválida.' }
        $mode = $Matches[1]; $name = $Matches[2]
        $hash = [BitConverter]::ToString($Content,$end+1,20).Replace('-','').ToLowerInvariant()
        if ($entries.ContainsKey($name)) { throw 'Entrada tree Git duplicada.' }
        $entries.Add($name,[pscustomobject]@{Mode=$mode;Sha=$hash})
        $offset = $end+21
    }
    return ,$entries
}

function Assert-GitLicenseOnlyCommit($Commit,[string]$ParentSha) {
    # Bind the proof to publisher commit IDs and compare complete root trees offline.
    $child = [Convert]::FromBase64String($Commit.RawObjects.ChildCommitBase64)
    $parent = [Convert]::FromBase64String($Commit.RawObjects.ParentCommitBase64)
    $childTree = [Convert]::FromBase64String($Commit.RawObjects.ChildTreeBase64)
    $parentTree = [Convert]::FromBase64String($Commit.RawObjects.ParentTreeBase64)
    if ((Get-GitObjectHash 'commit' $child) -cne $Commit.sha -or (Get-GitObjectHash 'commit' $parent) -cne $ParentSha) {
        throw 'Objeto commit Git difere da revisão publicada.'
    }
    $childText = [Text.Encoding]::UTF8.GetString($child)
    $parentText = [Text.Encoding]::UTF8.GetString($parent)
    $parents = [regex]::Matches($childText,'(?m)^parent ([a-f0-9]{40})$')
    if ($parents.Count -ne 1 -or $parents[0].Groups[1].Value -cne $ParentSha) { throw 'Pai do objeto commit Git incorreto.' }
    $childTreeId = [regex]::Match($childText,'\Atree ([a-f0-9]{40})\n').Groups[1].Value
    $parentTreeId = [regex]::Match($parentText,'\Atree ([a-f0-9]{40})\n').Groups[1].Value
    if ((Get-GitObjectHash 'tree' $childTree) -cne $childTreeId -or (Get-GitObjectHash 'tree' $parentTree) -cne $parentTreeId) {
        throw 'Objeto tree Git difere do commit.'
    }
    $before = Read-GitTree $parentTree; $after = Read-GitTree $childTree
    if ($before.ContainsKey('LICENSE') -or -not $after.ContainsKey('LICENSE') -or $after.Count -ne $before.Count+1 -or
        $after['LICENSE'].Mode -cne '100644' -or $after['LICENSE'].Sha -cne $Commit.files[0].sha) { throw 'Adição de LICENSE não confere na árvore Git.' }
    foreach ($name in $before.Keys) {
        if (-not $after.ContainsKey($name) -or $before[$name].Mode -cne $after[$name].Mode -or $before[$name].Sha -cne $after[$name].Sha) {
            throw 'Fontes da crate alterados na árvore Git da licença posterior.'
        }
    }
}

function Get-PeSummary([string]$Path) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or [BitConverter]::ToUInt16($bytes,0) -ne 0x5A4D) { throw 'Cabeçalho DOS inválido.' }
    $offset = [BitConverter]::ToInt32($bytes,0x3C)
    if ($offset -lt 64 -or $offset -gt $bytes.Length-26 -or [BitConverter]::ToUInt32($bytes,$offset) -ne 0x4550) { throw 'Cabeçalho PE inválido.' }
    $optional = $offset+24
    $magic = [BitConverter]::ToUInt16($bytes,$optional)
    $directoryOffset = switch ($magic) { 0x10b {96}; 0x20b {112}; default {throw 'Optional header inválido.'} }
    $sectionOffset = $optional + [BitConverter]::ToUInt16($bytes,$offset+20)
    $sectionCount = [BitConverter]::ToUInt16($bytes,$offset+6)
    function Convert-Rva([uint32]$Rva) {
        for ($index=0; $index -lt $sectionCount; $index++) {
            $section = $sectionOffset + 40*$index
            $address = [BitConverter]::ToUInt32($bytes,$section+12)
            $size = [BitConverter]::ToUInt32($bytes,$section+16)
            if ($Rva -ge $address -and $Rva-$address -lt $size) {
                $raw = [BitConverter]::ToUInt32($bytes,$section+20) + $Rva-$address
                if ($raw -ge $bytes.Length) { throw 'RVA fora do PE.' }
                return [int]$raw
            }
        }
        throw 'RVA sem seção.'
    }
    $imports = @()
    $importRva = [BitConverter]::ToUInt32($bytes,$optional+$directoryOffset+8)
    if ($importRva) {
        $descriptor = Convert-Rva $importRva
        for ($index=0; $index -lt 256; $index++) {
            if ($descriptor+20 -gt $bytes.Length) { throw 'Import descriptor fora do PE.' }
            $nameRva = [BitConverter]::ToUInt32($bytes,$descriptor+12)
            if (-not $nameRva) { break }
            $nameOffset = Convert-Rva $nameRva
            $end = $nameOffset
            while ($end -lt $bytes.Length -and $bytes[$end] -ne 0 -and $end-$nameOffset -lt 260) { $end++ }
            if ($end -eq $bytes.Length -or $end-$nameOffset -ge 260) { throw 'Nome DLL inválido.' }
            $imports += [Text.Encoding]::ASCII.GetString($bytes,$nameOffset,$end-$nameOffset)
            $descriptor += 20
        }
    }
    return [pscustomobject]@{ Machine=[BitConverter]::ToUInt16($bytes,$offset+4); LinkerMajor=[int]$bytes[$optional+2]; LinkerMinor=[int]$bytes[$optional+3]; Imports=$imports }
}

function Get-FileInventory([string]$Directory) {
    @(Get-ChildItem -LiteralPath $Directory -Recurse -File | Sort-Object FullName | ForEach-Object {
        Assert-RegularPath $_.FullName
        [pscustomobject][ordered]@{ Path=$_.FullName.Substring($Directory.Length+1).Replace('\','/'); Bytes=$_.Length; Sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    })
}

function Export-RustLicenses([string]$Root, [string]$Build, [string]$Destination) {
    New-Item -ItemType Directory -Path $Destination | Out-Null
    Push-Location $Root
    try {
        $raw = & cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata falhou; restaure as dependências antes de empacotar.' }
    } finally { Pop-Location }
    [IO.File]::WriteAllText((Join-Path $Build 'cargo-metadata.json'), ($raw -join "`n"), [Text.UTF8Encoding]::new($false))
    $metadata = ($raw -join "`n") | ConvertFrom-Json
    $supplements = @{}
    $supplementFile = Join-Path $Root 'assets/licenses/rust-supplements.json'
    $supplementData = Get-Content -Raw -Encoding UTF8 $supplementFile | ConvertFrom-Json
    if ($supplementData.Schema -ne 1) { throw 'Schema de licenças suplementares inválido.' }
    foreach ($entry in $supplementData.Packages) {
        $key = "$($entry.Name)@$($entry.Version)"
        if ($supplements.ContainsKey($key)) { throw 'Licença suplementar duplicada.' }
        $supplements[$key] = $entry
    }
    $packages = @{}; $nodes = @{}; $seen = @{}
    foreach ($package in $metadata.packages) { $packages[$package.id] = $package }
    foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
    $queue = [Collections.Generic.Queue[string]]::new()
    foreach ($id in $metadata.workspace_members) { $queue.Enqueue($id) }
    while ($queue.Count) {
        $id = $queue.Dequeue()
        if ($seen.ContainsKey($id)) { continue }
        $seen[$id] = $true
        foreach ($dependency in $nodes[$id].deps) {
            if (@($dependency.dep_kinds | Where-Object { $_.kind -ne 'dev' }).Count) { $queue.Enqueue($dependency.pkg) }
        }
    }
    $entries = @(); $issues = @()
    foreach ($package in @($seen.Keys | ForEach-Object { $packages[$_] } | Where-Object source | Sort-Object name,version)) {
        $source = Split-Path -Parent $package.manifest_path
        Assert-RegularPath $source
        # Include nested native-library notices, not only the crate's top-level license.
        $texts = @(Get-ChildItem -LiteralPath $source -Recurse -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|COPYRIGHT)([._-]|$)' })
        if ($package.license_file) {
            $declared = [IO.Path]::GetFullPath((Join-Path $source $package.license_file))
            if ($declared.StartsWith($source + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $declared -PathType Leaf)) {
                $texts += Get-Item -LiteralPath $declared
            } else { $issues += "$($package.name) $($package.version): license-file ausente ou fora da crate" }
        }
        $texts = @($texts | Sort-Object FullName -Unique)
        $noticeEntries = @()
        $index = 0
        foreach ($file in $texts) {
            Assert-RegularPath $file.FullName
            if ($file.Length -eq 0) { continue }
            $index++
            $relative = ('crates/{0}-{1}/{2:D3}.txt' -f $package.name,$package.version,$index)
            $target = Get-PackageFile $Destination $relative
            New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
            Copy-Item -LiteralPath $file.FullName -Destination $target
            $noticeEntries += [ordered]@{ Path=$relative; OriginalPath=$file.FullName.Substring($source.Length+1).Replace('\','/'); Sha256=(Get-FileHash -LiteralPath $target).Hash }
        }
        $supplementKey = "$($package.name)@$($package.version)"
        if ($supplements.ContainsKey($supplementKey)) {
            $supplement = $supplements[$supplementKey]
            $vcs = Get-Content -Raw -Encoding UTF8 (Join-Path $source '.cargo_vcs_info.json') | ConvertFrom-Json
            if ($vcs.git.sha1 -ne $supplement.GitCommit) { throw "Origem da licença suplementar difere da crate: $supplementKey" }
            $noticeCommit = $supplement.GitCommit
            $followup = $supplement.PSObject.Properties['LicenseOnlyFollowup']
            if ($followup) {
                # A direct child may add only the missing LICENSE; source and manifest stay identical.
                $proof = $followup.Value
                $repository = $package.repository -replace '\.git$',''
                $provider = if ($proof.PSObject.Properties['Provider']) { $proof.Provider } else { 'github-api' }
                if ($provider -eq 'git-object-proof' -and $repository -match '^https://gitlab\.redox-os\.org/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
                    $evidenceUrl = "$repository/-/commit/$($proof.GitCommit)"
                    $noticeUrl = "$repository/-/raw/$($proof.GitCommit)/LICENSE"
                } elseif ($provider -eq 'github-api' -and $repository -match '^https://github\.com/([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)$') {
                    $slug = $Matches[1]
                    $evidenceUrl = "https://api.github.com/repos/$slug/commits/$($proof.GitCommit)"
                    $noticeUrl = "https://raw.githubusercontent.com/$slug/$($proof.GitCommit)/LICENSE"
                } else { throw 'Repositório/provedor da licença posterior inválido.' }
                if ($proof.GitCommit -notmatch '^[a-f0-9]{40}$' -or
                    $proof.EvidencePath -notmatch '^rust-supplements/[A-Za-z0-9_.-]+/license-commit\.json$' -or
                    $proof.EvidenceUrl -cne $evidenceUrl) {
                    throw 'Proveniência da licença posterior inválida.'
                }
                $proofPath = Get-PackageFile (Join-Path $Root 'assets/licenses') $proof.EvidencePath
                if ((Get-FileHash -LiteralPath $proofPath).Hash -ne $proof.EvidenceSha256) { throw 'Evidência da licença posterior alterada.' }
                $commit = Get-Content -Raw -Encoding UTF8 $proofPath | ConvertFrom-Json
                if ($commit.sha -cne $proof.GitCommit -or @($commit.parents).Count -ne 1 -or
                    $commit.parents[0].sha -cne $supplement.GitCommit -or @($commit.files).Count -ne 1 -or
                    $commit.files[0].filename -cne 'LICENSE' -or $commit.files[0].status -cne 'added' -or @($supplement.Notices).Count -ne 1) {
                    throw 'A licença posterior precisa ser a única adição no filho direto da revisão da crate.'
                }
                if ($provider -eq 'git-object-proof') {
                    if ($commit.SourceUrl -cne $proof.EvidenceUrl) { throw 'Origem do recibo Git inválida.' }
                    Assert-GitLicenseOnlyCommit $commit $supplement.GitCommit
                }
                $noticeCommit = $proof.GitCommit
            }
            foreach ($notice in $supplement.Notices) {
                $noticeSource = Get-PackageFile (Join-Path $Root 'assets/licenses') $notice.Path
                if ($notice.Path -notmatch '^rust-supplements/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\.txt$' -or
                    ($notice.Url -notmatch '^https://raw\.githubusercontent\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/[a-f0-9]{40}/[A-Za-z0-9_.-]+$' -and
                     -not ($followup -and $provider -eq 'git-object-proof' -and $notice.Url -ceq $noticeUrl)) -or
                    -not $notice.Url.Contains('/' + $noticeCommit + '/') -or (Get-FileHash -LiteralPath $noticeSource).Hash -ne $notice.Sha256) {
                    throw "Texto suplementar alterado: $supplementKey"
                }
                if ($followup) {
                    if ($notice.Url -cne $noticeUrl) { throw 'URL da licença posterior inválida.' }
                    # Verify the exact Git blob referenced by the primary commit evidence.
                    $content = [IO.File]::ReadAllBytes($noticeSource)
                    $gitHash = Get-GitObjectHash 'blob' $content
                    if ($gitHash -cne $commit.files[0].sha) { throw 'Texto difere do blob da licença posterior.' }
                }
                $index++
                $relative = ('crates/{0}-{1}/{2:D3}.txt' -f $package.name,$package.version,$index)
                $target = Get-PackageFile $Destination $relative
                New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
                Copy-Item -LiteralPath $noticeSource -Destination $target
                $noticeEntries += [ordered]@{Path=$relative;OriginalPath=$notice.Url;Sha256=$notice.Sha256}
            }
        }
        if (-not $package.license) { $issues += "$($package.name) $($package.version): sem expressão de licença" }
        if (-not $noticeEntries.Count) { $issues += "$($package.name) $($package.version): sem texto de licença no pacote restaurado" }
        $entry = [ordered]@{ Name=$package.name; Version=$package.version; Source=$package.source; Repository=$package.repository; License=$package.license; Notices=$noticeEntries }
        if ($supplements.ContainsKey($supplementKey) -and $supplements[$supplementKey].PSObject.Properties['LicenseOnlyFollowup']) {
            $entry.SupplementProvenance = $supplements[$supplementKey].LicenseOnlyFollowup
        }
        $entries += $entry
    }
    $inventory = [ordered]@{
        Schema=1; Target='x86_64-pc-windows-msvc'; CargoLockSha256=(Get-FileHash -LiteralPath (Join-Path $Root 'Cargo.lock')).Hash
        Scope='Inventário conservador Cargo: arestas normais e build, sem dev, features padrão; não comprova ligação binária nem cobre sozinho bibliotecas nativas e runtime MSVC.'
        Packages=$entries; Issues=$issues; PublicReleaseReady=$false
    }
    Write-PackageJson (Join-Path $Destination 'cargo-licenses.json') $inventory
    # Keep the original resource notices and per-theme copyright/license files.
    $resources = @{
        'themes-summary.txt'='assets/themes/LICENSES'; 'one-MIT.txt'='assets/themes/one/LICENSE'
        'one-dark-pro-MIT.txt'='assets/themes/one-dark-pro/LICENSE'; 'ayu-MIT.txt'='assets/themes/ayu/LICENSE'
        'gruvbox-MIT.txt'='assets/themes/gruvbox/LICENSE'; 'material-symbols.txt'='assets/icons/LICENSE'
        'jetbrains-mono.txt'='assets/fonts/jetbrains-mono/OFL.txt'
    }
    foreach ($name in $resources.Keys) { Copy-Item -LiteralPath (Join-Path $Root $resources[$name]) -Destination (Join-Path $Destination $name) }
    return [ordered]@{ Packages=$entries.Count; Notices=@($entries | ForEach-Object { $_.Notices }).Count; Issues=$issues; PublicReleaseReady=$false }
}
