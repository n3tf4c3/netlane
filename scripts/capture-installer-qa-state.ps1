[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^[A-Za-z0-9_-]{1,64}$')][string]$Label,
    [string]$OutputDirectory
)

# Read-only product/network snapshot. Does not launch NetLane or an installer,
# change the registry/network, or declare a QA scenario approved.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not [Environment]::Is64BitProcess) { throw 'Use PowerShell de 64 bits.' }
$repository = Split-Path -Parent $PSScriptRoot
$artifacts = [IO.Path]::GetFullPath((Join-Path $repository 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $artifacts ('installer-qa\' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + $Label + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
}
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (-not $output.StartsWith($artifacts, [StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $output)) {
    throw 'Use uma pasta nova dentro de artifacts; nenhum recibo anterior é substituído.'
}

function Get-FileTreeState([string]$Root) {
    if (-not (Test-Path -LiteralPath $Root)) { return }
    $rootItem = Get-Item -LiteralPath $Root -Force
    if (-not $rootItem.PSIsContainer -or ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "A raiz de coleta não é uma pasta comum: $Root"
    }
    $pending = New-Object 'System.Collections.Generic.Stack[string]'
    $pending.Push($Root)
    while ($pending.Count -gt 0) {
        foreach ($item in Get-ChildItem -LiteralPath $pending.Pop() -Force | Sort-Object Name) {
            $relative = $item.FullName.Substring($Root.Length + 1).Replace('\', '/')
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                [ordered]@{ Path = $relative; Kind = 'ReparsePoint'; Sha256 = $null }
            } elseif ($item.PSIsContainer) {
                [ordered]@{ Path = $relative; Kind = 'Directory'; Sha256 = $null }
                $pending.Push($item.FullName)
            } else {
                [ordered]@{ Path = $relative; Kind = 'File'; Bytes = $item.Length; Sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash }
            }
        }
    }
}

$programDirectory = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane'
$profileDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'NetLane'
$startMenuDirectory = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'NetLane'
$productKey = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane'
$product = $null
if (Test-Path -LiteralPath $productKey) {
    $product = Get-ItemProperty -LiteralPath $productKey | Select-Object DisplayName,DisplayVersion,InstallLocation,UninstallString,DisplayIcon,Publisher,NoModify,NoRepair,EstimatedSize
}
$startup = $null
if (Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run') {
    $runKey = Get-Item -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    try { $startup = $runKey.GetValue('NetLane', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $runKey.Close() }
}
$flags = @{}
foreach ($family in @('ipv4', 'ipv6')) {
    $flags[$family] = @(& (Join-Path $env:SystemRoot 'System32\netsh.exe') interface $family show global)
    if ($LASTEXITCODE -ne 0) { throw "Falha na leitura dos flags $family." }
}
$state = [ordered]@{
    Label = $Label
    CapturedAtUtc = [DateTime]::UtcNow.ToString('o')
    Computer = $env:COMPUTERNAME
    UserSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    OperatingSystem = (Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture)
    ProductRegistration = $product
    StartupCommandCurrentUser = $startup
    Program = [ordered]@{ Root = $programDirectory; Exists = (Test-Path -LiteralPath $programDirectory); Entries = @(Get-FileTreeState $programDirectory) }
    Profile = [ordered]@{ Root = $profileDirectory; Exists = (Test-Path -LiteralPath $profileDirectory); Entries = @(Get-FileTreeState $profileDirectory) }
    StartMenu = [ordered]@{ Root = $startMenuDirectory; Exists = (Test-Path -LiteralPath $startMenuDirectory); Entries = @(Get-FileTreeState $startMenuDirectory) }
    Processes = @(Get-CimInstance Win32_Process -Filter "Name LIKE 'NetLane.%'" | Select-Object Name,ProcessId,SessionId,ExecutablePath,CreationDate)
    Adapters = @(Get-NetAdapter | Sort-Object InterfaceGuid | Select-Object InterfaceGuid,Name,ifIndex,Status,MacAddress)
    Bindings = @(Get-NetAdapterBinding -Name '*' -ComponentID ms_tcpip,ms_tcpip6 | Sort-Object Name,ComponentID | Select-Object Name,ComponentID,Enabled)
    IpInterfaces = @(Get-NetIPInterface | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,InterfaceAlias,AddressFamily,AutomaticMetric,InterfaceMetric,ConnectionState)
    PersistentIpInterfaces = @(Get-NetIPInterface -PolicyStore PersistentStore | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,InterfaceAlias,AddressFamily,AutomaticMetric,InterfaceMetric)
    IpAddresses = @(Get-NetIPAddress | Sort-Object InterfaceIndex,IPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin)
    DefaultRoutes = @(Get-NetRoute | Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0','::/0') } | Sort-Object InterfaceIndex,DestinationPrefix,NextHop | Select-Object InterfaceIndex,DestinationPrefix,NextHop,RouteMetric)
    Dns = @(Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses)
    GlobalNetworkFlags = $flags
    InstallerExecutedByCollector = $false
    NetworkChangedByCollector = $false
    FullInstallerQaVerified = $false
}
New-Item -ItemType Directory -Path $output -Force | Out-Null
$statePath = Join-Path $output 'state.json'
[IO.File]::WriteAllText($statePath, ($state | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
[pscustomobject]@{ State = $statePath; Sha256 = (Get-FileHash -LiteralPath $statePath -Algorithm SHA256).Hash; InstallerExecuted = $false; NetworkChanged = $false; QaScenarioVerified = $false } | ConvertTo-Json
