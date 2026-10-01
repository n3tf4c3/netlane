[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BuildDirectory,
    [Parameter(Mandatory = $true)][string]$SevenZipPath
)

# Static/archive validation only. Never execute the installer or bundled apps.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = Split-Path -Parent $PSScriptRoot
$build = (Resolve-Path -LiteralPath $BuildDirectory).Path
$artifacts = [IO.Path]::GetFullPath((Join-Path $repository 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $build.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use um build dentro de artifacts.' }
$archiveTool = (Resolve-Path -LiteralPath $SevenZipPath).Path
$report = Get-Content -LiteralPath (Join-Path $build 'build-report.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($report.Installer -ne [IO.Path]::GetFileName($report.Installer)) { throw 'Nome de instalador inválido.' }
$setup = Join-Path $build $report.Installer
if ((Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash -ne $report.Sha256) { throw 'Hash do instalador não confere.' }
$extracted = Join-Path $build ('archive-check-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
& $archiveTool x $setup ('-o' + $extracted) -y -bso0 -bsp0
if ($LASTEXITCODE -ne 0) { throw 'Falha na extração estática; o instalador não foi executado.' }
$manifestPath = Join-Path $extracted 'package-manifest.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($manifest.Runtime -ne 'win-x64' -or -not $manifest.SelfContained -or $manifest.Version -ne $report.Version) {
    throw 'Metadados do pacote não conferem.'
}
$expected = @{ 'package-manifest.json' = $true }
foreach ($entry in $manifest.Files) {
    $path = [IO.Path]::GetFullPath((Join-Path $extracted $entry.Path))
    if (-not $path.StartsWith($extracted + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Manifesto contém caminho fora do pacote.'
    }
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Arquivo ausente no instalador: $($entry.Path)" }
    if ((Get-Item -LiteralPath $path).Length -ne $entry.Bytes -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.Sha256) {
        throw "Conteúdo diferente no instalador: $($entry.Path)"
    }
    $expected[$entry.Path.Replace('\', '/')] = $true
}
foreach ($file in Get-ChildItem -LiteralPath $extracted -Recurse -File) {
    $relative = $file.FullName.Substring($extracted.Length + 1).Replace('\', '/')
    if ($relative -in @('$PLUGINSDIR/modern-wizard.bmp', '$PLUGINSDIR/nsDialogs.dll', '$PLUGINSDIR/System.dll', 'Uninstall.exe')) {
        continue # NSIS-generated uninstaller and exact UI/System plug-in list, not application payload.
    }
    if (-not $expected.ContainsKey($relative)) { throw "Arquivo fora do manifesto: $relative" }
    if ($file.Name -match '(?i)(^netlane-rules\.|^interface-selection\.|^quality-settings\.|\.pdb$|\.runtime\.(json|lock)$|\.log$|\.bak$|\.trx$|Probe|Review|RoutingCheck)') {
        throw "Dados locais ou ferramentas de ensaio no instalador: $relative"
    }
}
foreach ($relative in @('NetLane.UI.exe', 'service/NetLane.Service.exe')) {
    $bytes = [IO.File]::ReadAllBytes((Join-Path $extracted $relative))
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
    if ([BitConverter]::ToUInt16($bytes, $peOffset + 4) -ne 0x8664) { throw "Executável não é x64: $relative" }
    if ($relative -eq 'NetLane.UI.exe' -and [Text.Encoding]::UTF8.GetString($bytes) -notmatch 'requestedExecutionLevel\s+level="asInvoker"') {
        throw 'O painel não possui manifesto asInvoker.'
    }
}
$uiIcon = $null
if ([version]$report.Version -ge [version]'0.3.2') {
    $uiIcon = & (Join-Path $PSScriptRoot 'test-app-icon.ps1') -ExecutablePath (Join-Path $extracted 'NetLane.UI.exe') | ConvertFrom-Json
    if (-not $uiIcon.LogoMatches -or $uiIcon.ExecutableStarted) { throw 'Ícone do aplicativo ausente ou diferente do logo.' }
}
$installerBytes = [IO.File]::ReadAllBytes($setup)
if ([Text.Encoding]::UTF8.GetString($installerBytes) -notmatch 'requestedExecutionLevel\s+level="requireAdministrator"') {
    throw 'Instalador não possui manifesto de elevação.'
}
$uninstallerBytes = [IO.File]::ReadAllBytes((Join-Path $extracted 'Uninstall.exe'))
if ([Text.Encoding]::UTF8.GetString($uninstallerBytes) -notmatch 'requestedExecutionLevel\s+level="requireAdministrator"') {
    throw 'Desinstalador não possui manifesto de elevação.'
}
$source = Get-Content -LiteralPath (Join-Path $repository 'packaging\windows\NetLane.nsi') -Raw -Encoding UTF8
$deletions = Get-Content -LiteralPath (Join-Path $build 'uninstall-files.nsh') -Raw -Encoding UTF8
if ($source -match '(?im)^\s*(Exec|ExecWait|ExecShell|RMDir\s+/r)\b' -or $deletions -match '(?i)RMDir\s+/r|\*|\$LOCALAPPDATA|\$APPDATA') {
    throw 'A definição contém execução externa ou remoção ampla inesperada.'
}
if (-not $source.Contains('RequestExecutionLevel admin') -or -not $source.Contains('RequireNetLaneClosed "un."')) {
    throw 'Pré-condições obrigatórias do instalador ausentes.'
}
$defaults = Get-Content -LiteralPath (Join-Path $extracted 'service\appsettings.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if (@($defaults.NetLane.Routing.Policies).Count) { throw 'O instalador inclui regras no appsettings.' }
if ([version]$report.Version -ge [version]'0.3.1' -and (Get-Content -LiteralPath (Join-Path $extracted 'service\default-connection.protocol') -Raw -Encoding UTF8).Trim() -ne 'NetLane.DefaultConnection.v1') {
    throw 'Componente da conexão padrão não acompanha o pacote.'
}
if (@(Get-ChildItem -LiteralPath $extracted -Recurse -File | Where-Object { $_.Name -like 'default-connection.json*' }).Count -ne 0) {
    throw 'Configuração pessoal da conexão padrão encontrada no pacote.'
}
$result = [ordered]@{
    Passed = $true; InstallerSha256 = $report.Sha256; VerifiedPayloadFiles = $expected.Count
    X64Executables = 2; UiAsInvoker = $true; InstallerRequiresAdministrator = $true
    NoLocalRules = $true; EmptyDefaultPolicies = $true
    UiApplicationIcon = $uiIcon
    InstallerExecuted = $false; RuntimeGuardVerified = $false; InstallationVerified = $false
    SignatureStatus = (Get-AuthenticodeSignature -LiteralPath $setup).Status.ToString()
}
$json = $result | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $extracted 'verification.json'), $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Output $json
