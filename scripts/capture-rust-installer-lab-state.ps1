[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$PlanPath,
    [switch]$IdentifyOnly,
    [ValidatePattern('^[a-z0-9-]{1,40}$')][string]$Label='baseline'
)
# Read-only guest collector. Never launches setup, UI, agent, services or network tests.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$plan = Get-Content -Raw -LiteralPath $PlanPath -Encoding UTF8 | ConvertFrom-Json
if ($env:COMPUTERNAME -eq $plan.HostComputer) { throw 'Execucao no host recusada.' }
if ($plan.Schema -ne 1 -or $plan.Product -ne 'NetLaneRustPreview') { throw 'Plano Rust invalido.' }
if (-not [Environment]::Is64BitProcess) { throw 'Use PowerShell de 64 bits.' }
function Assert-RegularPath([string]$Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            if ((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Link/juncao recusado.' }
        }
        $parent = [IO.Path]::GetDirectoryName($current)
        if ($parent -eq $current) { break }
        $current = $parent
    }
}
$labRoot = Split-Path -Parent ([IO.Path]::GetFullPath($PlanPath))
Assert-RegularPath $labRoot
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$machine = Get-CimInstance Win32_ComputerSystem
$product = Get-CimInstance Win32_ComputerSystemProduct
$adapters = @(Get-NetAdapter | Sort-Object InterfaceGuid | Select-Object InterfaceGuid,Name,ifIndex,Status,MacAddress)
$expectedMac = $plan.Target.ExpectedMac -replace '[:-]',''
if ($expectedMac -notmatch '^[A-Fa-f0-9]{12}$' -or $machine.Manufacturer -ne 'VMware, Inc.' -or
    -not @($adapters | Where-Object { ($_.MacAddress -replace '[:-]','') -eq $expectedMac }).Count) {
    throw 'VMware ou MAC do convidado diferente do plano.'
}
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
$ipAddresses = @(Get-NetIPAddress | Sort-Object InterfaceIndex,IPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin)
$guest = [ordered]@{
    CapturedAtUtc=[DateTime]::UtcNow.ToString('o'); Computer=$env:COMPUTERNAME
    UserSid=$identity.User.Value; Uuid=$product.UUID; Manufacturer=$machine.Manufacturer
    OperatingSystem=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture)
    Elevated=$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    LabRoot=$labRoot; Adapters=$adapters; IpAddresses=$ipAddresses
}
function Write-NewJson([string]$Path,$Value) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 15))
    $stream = [IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $stream.Write($bytes,0,$bytes.Length) } finally { $stream.Dispose() }
}
function Assert-LabInput([string]$Relative,[string]$Hash) {
    if ($Relative -notmatch '^input/[A-Za-z0-9_.-]+$' -or $Hash -notmatch '^[a-fA-F0-9]{64}$') { throw 'Insumo de QA invalido.' }
    $path = Join-Path $labRoot $Relative
    Assert-RegularPath $path
    if ((Get-FileHash -LiteralPath $path).Hash -ne $Hash) { throw 'Hash de insumo diferente do plano.' }
    return $path
}
$setup = Assert-LabInput $plan.Package.Installer $plan.Package.Sha256
$manifestPath = Assert-LabInput $plan.Package.Manifest $plan.Package.ManifestSha256
$buildPath = Assert-LabInput $plan.Package.BuildReport $plan.Package.BuildReportSha256
$build = Get-Content -Raw -LiteralPath $buildPath -Encoding UTF8 | ConvertFrom-Json
$manifest = Get-Content -Raw -LiteralPath $manifestPath -Encoding UTF8 | ConvertFrom-Json
if ($build.Product -ne $plan.Product -or $manifest.Product -ne $plan.Product -or
    $build.Version -ne $plan.Package.Version -or $manifest.Version -ne $plan.Package.Version -or
    $build.Sha256 -ne $plan.Package.Sha256 -or $build.ManifestSha256 -ne $plan.Package.ManifestSha256 -or
    $build.Installer -cne [IO.Path]::GetFileName($setup)) { throw 'Pacote diferente do plano.' }
if (-not $IdentifyOnly) {
    if ($guest.Elevated) { throw 'Baseline Rust exige token normal do usuario de QA; nenhuma elevacao automatica.' }
    if (-not $plan.Target.Computer -or -not $plan.Target.UserSid -or -not $plan.Target.Uuid -or -not $plan.Target.LabRoot -or
        $env:COMPUTERNAME -ne $plan.Target.Computer -or $identity.User.Value -ne $plan.Target.UserSid -or
        $product.UUID -ne $plan.Target.Uuid -or $labRoot -ne $plan.Target.LabRoot) { throw 'Identidade do convidado nao confirmada.' }
    if (-not $plan.Target.Snapshot.Verified -or -not $plan.Target.Snapshot.Name) { throw 'Snapshot recuperavel nao confirmado.' }
    $null = Assert-LabInput $plan.Target.Snapshot.RecordPath $plan.Target.Snapshot.RecordSha256
}
$result = [ordered]@{ Schema=1; Product=$plan.Product; Label=$Label; Identity=$guest; PackageSha256=$plan.Package.Sha256 }
if (-not $IdentifyOnly) {
    function Get-TreeState([string]$Root) {
        $entries = @()
        Assert-RegularPath $Root
        if (Test-Path -LiteralPath $Root) {
            $rootItem = Get-Item -LiteralPath $Root -Force
            if (-not $rootItem.PSIsContainer) { throw 'Raiz de produto/perfil nao e uma pasta.' }
            $queue = [Collections.Generic.Queue[string]]::new()
            $queue.Enqueue($Root)
            while ($queue.Count) {
                foreach ($item in Get-ChildItem -LiteralPath ($queue.Dequeue()) -Force | Sort-Object Name) {
                    $relative = $item.FullName.Substring($Root.Length+1).Replace('\','/')
                    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                        $entries += [ordered]@{Path=$relative;Kind='ReparsePoint';Followed=$false}
                    } elseif ($item.PSIsContainer) {
                        $entries += [ordered]@{Path=$relative;Kind='Directory'}
                        $queue.Enqueue($item.FullName)
                    } else {
                        $entries += [ordered]@{Path=$relative;Kind='File';Bytes=$item.Length;Sha256=(Get-FileHash -LiteralPath $item.FullName).Hash}
                    }
                }
            }
        }
        return [ordered]@{Root=$Root;Exists=(Test-Path -LiteralPath $Root);Entries=$entries}
    }
    function Get-Registration([string]$Path) {
        if (-not (Test-Path -LiteralPath $Path)) { return $null }
        Get-ItemProperty -LiteralPath $Path | Select-Object DisplayName,DisplayVersion,InstallLocation,UninstallString,Publisher
    }
    function Get-RunHashes {
        $values = @()
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
        if ($null -eq $key) { return ,$values }
        try {
            foreach ($name in $key.GetValueNames() | Sort-Object) {
                $kind = $key.GetValueKind($name).ToString()
                $value = $key.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                $bytes = [Text.UTF8Encoding]::new($false).GetBytes((@($kind,$value) | ConvertTo-Json -Depth 5 -Compress))
                $sha = [Security.Cryptography.SHA256]::Create()
                try { $hash = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-','') } finally { $sha.Dispose() }
                $values += [ordered]@{Name=$name;Kind=$kind;Sha256=$hash}
            }
        } finally { $key.Close() }
        return ,$values
    }
    $local = [Environment]::GetFolderPath('LocalApplicationData')
    $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'NetLane Rust.lnk'
    Assert-RegularPath $shortcut
    $result.Snapshot = $plan.Target.Snapshot
    $result.RustProgram = Get-TreeState (Join-Path $local 'Programs\NetLane Rust')
    $result.RustProfile = Get-TreeState (Join-Path $local 'NetLane-Rust')
    $result.LegacyProgram = Get-TreeState (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane')
    $result.LegacyProfile = Get-TreeState (Join-Path $local 'NetLane')
    $result.Shortcut = [ordered]@{Path=$shortcut;Exists=(Test-Path -LiteralPath $shortcut);Sha256=$null}
    if ($result.Shortcut.Exists) {
        if ((Get-Item -LiteralPath $shortcut -Force).PSIsContainer) { throw 'Atalho e uma pasta.' }
        $result.Shortcut.Sha256 = (Get-FileHash -LiteralPath $shortcut).Hash
    }
    $result.RustRegistration = Get-Registration 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLaneRustPreview'
    $result.LegacyRegistration = Get-Registration 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane'
    $result.CurrentUserRun = Get-RunHashes
    $result.Processes = @(Get-CimInstance Win32_Process -Filter "Name='netlane.exe' OR Name='netlane-agent.exe' OR Name LIKE 'NetLane.%'" | Select-Object Name,ProcessId,SessionId,ExecutablePath,CreationDate)
    $result.Bindings = @(Get-NetAdapterBinding -Name '*' -ComponentID ms_tcpip,ms_tcpip6 | Sort-Object Name,ComponentID | Select-Object Name,ComponentID,Enabled)
    $result.IpInterfaces = @(Get-NetIPInterface | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,AutomaticMetric,InterfaceMetric,ConnectionState)
    $result.PersistentIpInterfaces = @(Get-NetIPInterface -PolicyStore PersistentStore | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,AutomaticMetric,InterfaceMetric)
    $result.DefaultRoutes = @(Get-NetRoute | Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0','::/0') } | Sort-Object InterfaceIndex,DestinationPrefix,NextHop | Select-Object InterfaceIndex,DestinationPrefix,NextHop,RouteMetric)
    $result.Dns = @(Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses)
    $runtime = Join-Path $env:SystemRoot 'System32\vcruntime140.dll'
    $result.VisualCppX64 = [ordered]@{Exists=(Test-Path -LiteralPath $runtime);FileVersion=$null;Minimum=$manifest.Prerequisites.VisualCppX64Minimum}
    if ($result.VisualCppX64.Exists) { $result.VisualCppX64.FileVersion = (Get-Item -LiteralPath $runtime).VersionInfo.FileVersion }
    $result.IcuX64Present = Test-Path -LiteralPath (Join-Path $env:SystemRoot 'System32\icuuc.dll')
}
$result.IdentifyOnly = [bool]$IdentifyOnly
$result.InstallerExecuted = $false
$result.ProductLaunched = $false
$result.NetworkChanged = $false
$result.RuntimeQaVerified = $false
$result.PublicReleaseReady = $false
$output = Join-Path $labRoot ('results-' + $Label + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $output | Out-Null
$path = Join-Path $output 'state.json'
Write-NewJson $path $result
[ordered]@{State=$path;Sha256=(Get-FileHash -LiteralPath $path).Hash;IdentifyOnly=[bool]$IdentifyOnly;InstallerExecuted=$false;RuntimeQaVerified=$false} | ConvertTo-Json
