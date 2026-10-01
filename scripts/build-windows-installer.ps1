[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$NsisCompilerPath,
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version = '0.3.3',
    [string]$OutputDirectory
)

# Build only: never execute an installer, start NetLane, or modify network settings.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = Split-Path -Parent $PSScriptRoot
$packaging = Join-Path $repository 'packaging\windows'
$compiler = (Resolve-Path -LiteralPath $NsisCompilerPath).Path
$compilerVersion = (& $compiler /VERSION | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $compilerVersion -ne 'v3.12') { throw 'Use o compilador portátil NSIS 3.12.' }
if (([version]$Version).Major -gt 65535 -or ([version]$Version).Minor -gt 65535 -or ([version]$Version).Build -gt 65535) {
    throw 'Cada componente da versão deve ser menor que 65536.'
}
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $repository ('artifacts\installer\' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
}
$output = [IO.Path]::GetFullPath($OutputDirectory)
$artifacts = [IO.Path]::GetFullPath((Join-Path $repository 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $output.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)) {
    throw 'A saída deve ser uma pasta nova dentro de artifacts. Builds anteriores nunca são apagados.'
}
$payload = Join-Path $output 'payload'
New-Item -ItemType Directory -Path $payload -Force | Out-Null
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Write-GeneratedText([string]$Path, [string]$Content) {
    [IO.File]::WriteAllText($Path, $Content, $utf8)
}
function Nsis-Literal([string]$Value) {
    if ($Value.Contains('"') -or $Value.Contains("`r") -or $Value.Contains("`n")) { throw 'Caminho incompatível com NSIS.' }
    return $Value.Replace('$', '$$')
}
function Invoke-Checked([string]$Executable, [string[]]$Arguments) {
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Comando falhou ($LASTEXITCODE): $Executable" }
}
function Get-PayloadFiles {
    @(Get-ChildItem -LiteralPath $payload -Recurse -File | Sort-Object FullName)
}

$revision = (& git -C $repository rev-parse HEAD | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Não foi possível identificar a revisão Git.' }
$dirty = -not [string]::IsNullOrWhiteSpace((& git -C $repository status --porcelain | Out-String))
foreach ($project in @('NetLane.UI', 'NetLane.Service')) {
    $destination = if ($project -eq 'NetLane.UI') { $payload } else { Join-Path $payload 'service' }
    Invoke-Checked 'dotnet' @('publish', (Join-Path $repository "src\$project\$project.csproj"),
        '--configuration', 'Release', '--runtime', 'win-x64', '--self-contained', 'true',
        '--artifacts-path', (Join-Path $output 'build'), '--output', $destination,
        '-p:PublishSingleFile=false', '-p:PublishTrimmed=false', '-p:DebugType=None', '-p:DebugSymbols=false',
        "-p:Version=$Version", "-p:InformationalVersion=$Version-local", "-p:SourceRevisionId=$revision")
}
Copy-Item -LiteralPath (Join-Path $packaging 'netlane-installed.layout') -Destination $payload
Copy-Item -LiteralPath (Join-Path $packaging 'LEIA-ME.txt') -Destination $payload

# Retain redistribution notices provided by the compiler and restored .NET packages.
$notices = Join-Path $payload 'notices'
New-Item -ItemType Directory -Path $notices | Out-Null
Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $compiler) 'COPYING') -Destination (Join-Path $notices 'NSIS-COPYING.txt')
$seenPackages = @{}
foreach ($assetsFile in Get-ChildItem -LiteralPath (Join-Path $output 'build\obj') -Recurse -Filter 'project.assets.json' -File) {
    $assets = Get-Content -LiteralPath $assetsFile.FullName -Raw | ConvertFrom-Json
    foreach ($library in $assets.libraries.PSObject.Properties) {
        if ($library.Value.type -ne 'package' -or $seenPackages.ContainsKey($library.Name)) { continue }
        $seenPackages[$library.Name] = $true
        foreach ($packageRoot in $assets.packageFolders.PSObject.Properties.Name) {
            $packagePath = Join-Path $packageRoot $library.Value.path
            if (-not (Test-Path -LiteralPath $packagePath)) { continue }
            foreach ($notice in Get-ChildItem -LiteralPath $packagePath -File | Where-Object { $_.Name -match '^(LICENSE|THIRD.PARTY.NOTICES|ThirdPartyNotices|NOTICE)(\.|$)' }) {
                $noticeName = $library.Name.Replace('/', '-') + '-' + $notice.Name
                Copy-Item -LiteralPath $notice.FullName -Destination (Join-Path $notices $noticeName)
            }
        }
    }
}

foreach ($required in @('NetLane.UI.exe', 'NetLane.UI.runtimeconfig.json', 'coreclr.dll',
        'service\NetLane.Service.exe', 'service\NetLane.Service.runtimeconfig.json', 'service\coreclr.dll',
        'service\appsettings.json', 'service\default-connection.protocol', 'netlane-installed.layout')) {
    if (-not (Test-Path -LiteralPath (Join-Path $payload $required) -PathType Leaf)) { throw "Arquivo obrigatório ausente: $required" }
}
$defaults = Get-Content -LiteralPath (Join-Path $payload 'service\appsettings.json') -Raw | ConvertFrom-Json
if (@($defaults.NetLane.Routing.Policies).Count -ne 0) { throw 'O pacote não pode incluir regras de exemplo ou locais no appsettings.' }
foreach ($file in Get-PayloadFiles) {
    if ($file.Name -match '(?i)(^netlane-rules\.|^interface-selection\.|^default-connection\.json|\.pdb$|\.runtime\.(json|lock)$|\.log$|\.bak$|\.trx$|Probe|Review|RoutingCheck)') {
        throw "Arquivo privado ou de ensaio proibido no pacote: $($file.Name)"
    }
}
$frameworks = @{}
foreach ($relative in @('NetLane.UI.runtimeconfig.json', 'service\NetLane.Service.runtimeconfig.json')) {
    $runtime = Get-Content -LiteralPath (Join-Path $payload $relative) -Raw | ConvertFrom-Json
    $included = @($runtime.runtimeOptions.includedFrameworks)
    if ($included.Count -eq 0) { throw "Publicação não é self-contained: $relative" }
    foreach ($framework in $included) {
        if ($frameworks.ContainsKey($framework.name) -and $frameworks[$framework.name] -ne $framework.version) {
            throw 'UI e serviço contêm versões diferentes do runtime.'
        }
        $frameworks[$framework.name] = $framework.version
    }
}

# Runtime packs are downloadDependencies, not ordinary libraries in project.assets.json.
foreach ($assetsFile in Get-ChildItem -LiteralPath (Join-Path $output 'build\obj') -Recurse -Filter 'project.assets.json' -File) {
    $assets = Get-Content -LiteralPath $assetsFile.FullName -Raw | ConvertFrom-Json
    foreach ($frameworkName in $frameworks.Keys) {
        $packageName = ($frameworkName + '.Runtime.win-x64').ToLowerInvariant()
        foreach ($packageRoot in $assets.packageFolders.PSObject.Properties.Name) {
            $packagePath = Join-Path $packageRoot ($packageName + '\' + $frameworks[$frameworkName])
            if (-not (Test-Path -LiteralPath $packagePath)) { continue }
            foreach ($notice in Get-ChildItem -LiteralPath $packagePath -File | Where-Object { $_.Name -match '^(LICENSE|THIRD.PARTY.NOTICES|ThirdPartyNotices|NOTICE)(\.|$)' }) {
                Copy-Item -LiteralPath $notice.FullName -Destination (Join-Path $notices ($packageName + '-' + $frameworks[$frameworkName] + '-' + $notice.Name))
            }
        }
    }
}
foreach ($frameworkName in $frameworks.Keys) {
    $noticePrefix = ($frameworkName + '.Runtime.win-x64-' + $frameworks[$frameworkName]).ToLowerInvariant()
    if (@(Get-ChildItem -LiteralPath $notices -File | Where-Object { $_.Name.StartsWith($noticePrefix, [StringComparison]::OrdinalIgnoreCase) -and $_.Name -match 'LICENSE' }).Count -eq 0) {
        throw "Licença do runtime não encontrada: $frameworkName"
    }
}

$inventory = @(foreach ($file in Get-PayloadFiles) {
    [ordered]@{ Path = $file.FullName.Substring($payload.Length + 1).Replace('\', '/'); Bytes = $file.Length; Sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash }
})
Write-GeneratedText (Join-Path $payload 'package-manifest.json') ([ordered]@{
    Product = 'NetLane'; Version = $Version; Channel = 'local-preview'; Runtime = 'win-x64'; SelfContained = $true
    GitRevision = $revision; WorktreeDirty = $dirty; Frameworks = $frameworks; Files = $inventory
} | ConvertTo-Json -Depth 8)

# Compile an exact payload list, not a recursive wildcard. The uninstaller never
# reads a deletion list from disk and never recursively removes directories.
$installLines = New-Object 'System.Collections.Generic.List[string]'
$uninstallLines = New-Object 'System.Collections.Generic.List[string]'
$directories = @{}
$totalBytes = 0L
# Install the marker first so an interrupted copy can be retried. Remove it last.
$installLines.Add('ClearErrors')
$installLines.Add('SetOutPath "$INSTDIR"')
$installLines.Add('IfErrors install_failed')
$installLines.Add('File "' + (Nsis-Literal (Join-Path $payload 'netlane-installed.layout')) + '"')
$installLines.Add('IfErrors install_failed')
$fileChecks = New-Object 'System.Collections.Generic.List[string]'
foreach ($file in Get-PayloadFiles) {
    $relative = $file.FullName.Substring($payload.Length + 1)
    $totalBytes += $file.Length
    $fileChecks.Add('!insertmacro CheckPayloadFile "' + (Nsis-Literal $relative) + '"')
    if ($relative -eq 'netlane-installed.layout') { continue }
    $directory = Split-Path -Parent $relative
    $target = if ($directory) { '$INSTDIR\' + (Nsis-Literal $directory) } else { '$INSTDIR' }
    $installLines.Add('ClearErrors')
    $installLines.Add('SetOutPath "' + $target + '"')
    $installLines.Add('IfErrors install_failed')
    $installLines.Add('File "' + (Nsis-Literal $file.FullName) + '"')
    $installLines.Add('IfErrors install_failed')
    $uninstallLines.Add('ClearErrors')
    $uninstallLines.Add('Delete "$INSTDIR\' + (Nsis-Literal $relative) + '"')
    $uninstallLines.Add('IfErrors uninstall_failed')
    while ($directory) { $directories[$directory] = $true; $directory = Split-Path -Parent $directory }
}
foreach ($directory in ($directories.Keys | Sort-Object { $_.Length } -Descending)) {
    $uninstallLines.Add('RMDir "$INSTDIR\' + (Nsis-Literal $directory) + '"')
}
Write-GeneratedText (Join-Path $output 'install-files.nsh') ($installLines -join "`r`n")
Write-GeneratedText (Join-Path $output 'uninstall-files.nsh') ($uninstallLines -join "`r`n")
$directoryChecks = @(foreach ($directory in ($directories.Keys | Sort-Object)) {
    '!insertmacro CheckPayloadDirectory "' + (Nsis-Literal $directory) + '"'
})
Write-GeneratedText (Join-Path $output 'validate-payload.nsh') (($directoryChecks + @($fileChecks)) -join "`r`n")
$setup = Join-Path $output "NetLane-$Version-preview-win-x64-setup.exe"
$defines = @(
    '!define PRODUCT_VERSION "' + $Version + '"'
    '!define SETUP_OUTPUT "' + (Nsis-Literal $setup) + '"'
    '!define INSTALL_FILES "' + (Nsis-Literal (Join-Path $output 'install-files.nsh')) + '"'
    '!define UNINSTALL_FILES "' + (Nsis-Literal (Join-Path $output 'uninstall-files.nsh')) + '"'
    '!define VALIDATE_PAYLOAD "' + (Nsis-Literal (Join-Path $output 'validate-payload.nsh')) + '"'
    '!define PAYLOAD_KIB ' + [math]::Ceiling($totalBytes / 1024)
)
Write-GeneratedText (Join-Path $output 'build-defines.nsh') ($defines -join "`r`n")
Invoke-Checked $compiler @('/V2', '/WX', '/INPUTCHARSET', 'UTF8', ('/DBUILD_DEFINES=' + (Join-Path $output 'build-defines.nsh')), (Join-Path $packaging 'NetLane.nsi'))
$setupHash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash
Write-GeneratedText ($setup + '.sha256') ($setupHash + '  ' + [IO.Path]::GetFileName($setup) + "`r`n")
Write-GeneratedText (Join-Path $output 'build-report.json') ([ordered]@{
    Installer = [IO.Path]::GetFileName($setup); Sha256 = $setupHash; Bytes = (Get-Item -LiteralPath $setup).Length
    Version = $Version; GitRevision = $revision; WorktreeDirty = $dirty; Nsis = $compilerVersion
    CompilerSha256 = (Get-FileHash -LiteralPath $compiler -Algorithm SHA256).Hash
    PayloadFiles = @(Get-PayloadFiles).Count; Frameworks = $frameworks; InstallerExecuted = $false
    RuntimeGuardVerified = $false; InstallationVerified = $false
    Signed = ((Get-AuthenticodeSignature -LiteralPath $setup).Status -eq 'Valid')
} | ConvertTo-Json -Depth 5)
Write-Host "Instalador gerado, NÃO executado: $setup"
Write-Host "SHA-256: $setupHash"
