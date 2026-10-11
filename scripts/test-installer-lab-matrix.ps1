[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$PlanPath)

# Headless matrix cases for the .NET installer in an identified VMware lab guest:
# process guard, file in use, invalid destinations (/D, links, type swaps, moved
# uninstaller), removal failures and copy failure + repair. Faults are induced only
# on the product's own paths and always reverted. No UI/service startup, real logon,
# network changes, protection changes or signing. Cancel, UI autorun, real logon and
# installed UI need an interactive desktop and stay out of this script.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$plan = Get-Content -LiteralPath $PlanPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($env:COMPUTERNAME -eq $plan.HostComputer) { throw 'Execucao no host recusada.' }
if (-not [Environment]::Is64BitProcess) { throw 'Use PowerShell de 64 bits.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$machine = Get-CimInstance Win32_ComputerSystem
$machineProduct = Get-CimInstance Win32_ComputerSystemProduct
if ($env:COMPUTERNAME -ne $plan.Target.Computer -or $identity.User.Value -ne $plan.Target.UserSid -or
    $machine.Manufacturer -ne 'VMware, Inc.' -or $machineProduct.UUID -ne $plan.Target.Uuid -or
    -not @(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -eq $plan.Target.IPv4 }).Count) {
    throw 'Identidade do alvo diferente do plano autorizado.'
}
if (-not (New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Token administrativo ausente; nenhuma elevacao automatica.'
}
# Service logons (e.g. OpenSSH) enable backup/restore/take-ownership; they bypass the
# deny ACEs used to induce faults. A UAC-elevated desktop token has them disabled, and
# child installers inherit this process token, so disable them here.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class QaTokenPrivileges {
    [StructLayout(LayoutKind.Sequential, Pack = 4)] struct Luid { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential, Pack = 4)] struct TokenPrivilege { public int Count; public Luid Luid; public int Attributes; }
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool OpenProcessToken(IntPtr process, int access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool LookupPrivilegeValue(string system, string name, out Luid luid);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool AdjustTokenPrivileges(IntPtr token, bool disableAll, ref TokenPrivilege state, int length, IntPtr previous, IntPtr returned);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    public static void Disable(string name) {
        IntPtr token;
        if (!OpenProcessToken(GetCurrentProcess(), 0x28, out token)) throw new System.ComponentModel.Win32Exception();
        try {
            var state = new TokenPrivilege { Count = 1, Attributes = 0 };
            if (!LookupPrivilegeValue(null, name, out state.Luid)) throw new System.ComponentModel.Win32Exception();
            if (!AdjustTokenPrivileges(token, false, ref state, 0, IntPtr.Zero, IntPtr.Zero) || Marshal.GetLastWin32Error() != 0) throw new System.ComponentModel.Win32Exception();
        } finally { CloseHandle(token); }
    }
}
'@
foreach ($privilege in @('SeBackupPrivilege', 'SeRestorePrivilege', 'SeTakeOwnershipPrivilege')) { [QaTokenPrivileges]::Disable($privilege) }
if (@(whoami /priv /fo csv | ConvertFrom-Csv | Where-Object { $_.PSObject.Properties.Value[0] -in @('SeBackupPrivilege','SeRestorePrivilege','SeTakeOwnershipPrivilege') -and $_.PSObject.Properties.Value[2] -notmatch 'Desativad|Disabled' }).Count) {
    throw 'Privilegios de backup/restauracao continuam ativos; falhas induzidas nao seriam validas.'
}
$labRoot = Split-Path -Parent ([IO.Path]::GetFullPath($PlanPath))
if ($labRoot -ne $plan.Target.LabRoot -or $labRoot.Contains(' ')) { throw 'Pasta de QA diferente do plano ou com espacos.' }
$inputDirectory = Join-Path $labRoot 'input'
$outputDirectory = Join-Path $labRoot 'artifacts\results'
$workDirectory = Join-Path $labRoot 'work'
$collector = Join-Path $labRoot 'scripts\capture-installer-qa-state.ps1'
$programDirectory = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane'
$profileDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'NetLane'
$startMenuDirectory = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'NetLane'
$shortcut = Join-Path $startMenuDirectory 'NetLane.lnk'
$productKey = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane'
$runKeyPath = 'Software\Microsoft\Windows\CurrentVersion\Run'
$everyone = New-Object Security.Principal.SecurityIdentifier('S-1-1-0')
if ((Test-Path -LiteralPath $programDirectory) -or (Test-Path -LiteralPath $profileDirectory) -or
    (Test-Path -LiteralPath $productKey) -or (Test-Path -LiteralPath $startMenuDirectory)) {
    throw 'O laboratorio deve estar sem instalacao, perfil ou atalhos anteriores do NetLane.'
}
if ((Test-Path -LiteralPath $outputDirectory) -or (Test-Path -LiteralPath $workDirectory)) { throw 'Preserve os recibos anteriores; use uma rodada nova.' }
$utf8 = New-Object System.Text.UTF8Encoding($false)
$cases = New-Object 'System.Collections.Generic.List[object]'
$cleanup = New-Object 'System.Collections.Generic.List[scriptblock]'
$stage = 'preflight'

function Assert-NoProductProcesses {
    if (@(Get-Process -Name 'NetLane.*' -ErrorAction SilentlyContinue).Count) { throw 'Processo NetLane presente; nenhum encerramento forcado.' }
}
function Get-StartupValues {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($runKeyPath)
    if ($null -eq $key) { return '' }
    try {
        return (@(foreach ($name in $key.GetValueNames() | Sort-Object) {
            [pscustomobject]@{ Name = $name; Kind = $key.GetValueKind($name).ToString()
                Value = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
        }) | ConvertTo-Json -Depth 4 -Compress)
    } finally { $key.Close() }
}
function Write-Receipt([string]$Name, $Value) {
    $path = Join-Path $outputDirectory $Name
    if (Test-Path -LiteralPath $path) { throw 'Recibo anterior nao pode ser substituido.' }
    [IO.File]::WriteAllText($path, ($Value | ConvertTo-Json -Depth 12), $utf8)
}
function Capture-State([string]$Label) {
    $receipt = & $collector -Label $Label | ConvertFrom-Json
    if (-not $receipt.State -or (Get-FileHash -LiteralPath $receipt.State -Algorithm SHA256).Hash -ne $receipt.Sha256) { throw 'Estado de QA incompleto.' }
    return $receipt
}
function Assert-Package($Package) {
    if ((Get-FileHash -LiteralPath (Join-Path $inputDirectory $Package.Installer) -Algorithm SHA256).Hash -ne $Package.Sha256 -or
        (Get-FileHash -LiteralPath (Join-Path $inputDirectory $Package.Manifest) -Algorithm SHA256).Hash -ne $Package.ManifestSha256) {
        throw 'Hash de insumo diferente do plano.'
    }
}
function Get-Manifest($Package) {
    return Get-Content -LiteralPath (Join-Path $inputDirectory $Package.Manifest) -Raw -Encoding UTF8 | ConvertFrom-Json
}
function Invoke-Checked([string]$FilePath, [string]$Arguments, [int]$Expected) {
    $process = Start-Process -FilePath $FilePath -ArgumentList $Arguments -PassThru -WindowStyle Hidden
    $null = $process.Handle  # fast refusals can exit before WaitForExit
    if (-not $process.WaitForExit(300000)) { throw 'Processo nao concluiu; nenhum encerramento forcado.' }
    $process.Refresh()
    if ($process.ExitCode -ne $Expected) { throw "Saida inesperada: $($process.ExitCode); esperado $Expected." }
    return $process.ExitCode
}
function Invoke-Installer($Package, [int]$Expected, [string]$Extra = '') {
    Assert-Package $Package
    $arguments = ('/S ' + $Extra).Trim()
    return Invoke-Checked (Join-Path $inputDirectory $Package.Installer) $arguments $Expected
}
# Same mechanism as the NSIS launcher (copy outside $INSTDIR + _?=), but keeps the real exit code.
function Invoke-Uninstaller([int]$Expected, [string]$InstallDirectory = $programDirectory, [string]$Source = (Join-Path $programDirectory 'Uninstall.exe')) {
    $copy = Join-Path $workDirectory ('uninstall-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $copy | Out-Null
    Copy-Item -LiteralPath $Source -Destination $copy
    return Invoke-Checked (Join-Path $copy 'Uninstall.exe') ('/S _?=' + $InstallDirectory) $Expected
}
function Get-ProductState {
    $files = @()
    if (Test-Path -LiteralPath $programDirectory) {
        $files = @(Get-ChildItem -LiteralPath $programDirectory -Recurse -Force | Sort-Object FullName | ForEach-Object {
            $hash = if ($_.PSIsContainer -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)) { '' } else { (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
            '{0}|{1}|{2}' -f $_.FullName.Substring($programDirectory.Length), [int]$_.Attributes, $hash
        })
    }
    $registry = if (Test-Path -LiteralPath $productKey) { (Get-ItemProperty -LiteralPath $productKey | Select-Object DisplayVersion, UninstallString, InstallLocation | ConvertTo-Json -Compress) } else { '' }
    $menu = if (Test-Path -LiteralPath $startMenuDirectory) { @(Get-ChildItem -LiteralPath $startMenuDirectory -Force | ForEach-Object { '{0}|{1}' -f $_.Name, [int]$_.Attributes }) } else { @() }
    return ([ordered]@{ Files = $files; Registry = $registry; Menu = $menu; Startup = (Get-StartupValues) } | ConvertTo-Json -Depth 4 -Compress)
}
function Assert-SameState([string]$Before, [string]$What) {
    if ((Get-ProductState) -cne $Before) { throw "Estado do produto alterado: $What" }
}
function Assert-Payload($Package) {
    $manifest = Get-Manifest $Package
    if ((Get-ItemProperty -LiteralPath $productKey).DisplayVersion -ne $Package.Version) { throw 'Versao instalada diferente.' }
    $entries = @($manifest.Files) + @([pscustomobject]@{ Path = 'package-manifest.json'; Sha256 = $Package.ManifestSha256 })
    foreach ($entry in $entries) {
        $path = [IO.Path]::GetFullPath((Join-Path $programDirectory $entry.Path))
        if (-not $path.StartsWith($programDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Caminho fora do programa.' }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw "Arquivo instalado diferente: $($entry.Path)" }
    }
    if (-not (Test-Path -LiteralPath $shortcut)) { throw 'Atalho instalado ausente.' }
    return $entries.Count
}
function Assert-Removed {
    if ((Test-Path -LiteralPath $productKey) -or (Test-Path -LiteralPath $programDirectory) -or (Test-Path -LiteralPath $startMenuDirectory)) {
        throw 'Remocao incompleta.'
    }
}
function Assert-Sentinels($Sentinels) {
    foreach ($entry in $Sentinels) {
        if (-not (Test-Path -LiteralPath $entry.Path) -or (Get-FileHash -LiteralPath $entry.Path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw "Arquivo externo/fixture alterado: $($entry.Path)" }
    }
}
function New-Sentinel([string]$Path, [string]$Text) {
    New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force | Out-Null
    [IO.File]::WriteAllText($Path, $Text, $utf8)
    return [pscustomobject]@{ Path = $Path; Sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }
}
function Add-Case([string]$Name, [hashtable]$Data) {
    $entry = [ordered]@{ Case = $Name; Passed = $true }
    foreach ($key in $Data.Keys) { $entry[$key] = $Data[$key] }
    $cases.Add($entry)
}
function Set-FileDeny([string]$Path, [Security.AccessControl.FileSystemRights]$Rights) {
    $acl = Get-Acl -LiteralPath $Path
    $rule = New-Object Security.AccessControl.FileSystemAccessRule($everyone, $Rights, 'None', 'None', 'Deny')
    $acl.AddAccessRule($rule)
    Set-Acl -LiteralPath $Path -AclObject $acl
    $cleanup.Add({ $a = Get-Acl -LiteralPath $Path; [void]$a.RemoveAccessRule($rule); Set-Acl -LiteralPath $Path -AclObject $a }.GetNewClosure())
}
function Clear-Faults {
    for ($i = $cleanup.Count - 1; $i -ge 0; $i--) { & $cleanup[$i] }
    $cleanup.Clear()
}

Assert-NoProductProcesses
$startupBefore = Get-StartupValues
if ($startupBefore -match '"Name":"NetLane"') { throw 'Autorun anterior do NetLane presente; nao sobrescrever.' }
Assert-Package $plan.Previous
Assert-Package $plan.Latest
New-Item -ItemType Directory -Path $outputDirectory, $workDirectory | Out-Null
$external = Join-Path $workDirectory 'external'
try {
    Write-Receipt 'started.json' ([ordered]@{
        Computer = $env:COMPUTERNAME; UserSid = $identity.User.Value; Uuid = $machineProduct.UUID
        StartedUtc = [DateTime]::UtcNow.ToString('o'); HostInstallation = $false; NetworkTests = $false; RealLogon = $false
    })
    $before = Capture-State 'matrix-before'
    $sentinels = @(
        (New-Sentinel (Join-Path $external 'sentinel.txt') 'External QA file; must never change.'),
        (New-Sentinel (Join-Path $external 'NetLane.Core.dll') 'External file disguised as payload; must never change.'),
        (New-Sentinel (Join-Path $external 'notices\qa-external.txt') 'External notice; must never change.'),
        (New-Sentinel (Join-Path $profileDirectory 'netlane-rules.json') '[]')
    )
    $profileFixture = $sentinels[3]

    # Prove the fault mechanism before relying on it: a deny ACE must block deletion here.
    $probeFile = (New-Sentinel (Join-Path $workDirectory 'deny-check\probe.txt') 'probe').Path
    Set-FileDeny $probeFile 'Delete'
    Set-FileDeny (Split-Path -Parent $probeFile) 'DeleteSubdirectoriesAndFiles'
    $denied = $false
    try { [IO.File]::Delete($probeFile) } catch { $denied = $true }
    Clear-Faults
    if (-not $denied -or -not (Test-Path -LiteralPath $probeFile)) { throw 'Negacao de ACL sem efeito neste token; falhas induzidas invalidas.' }

    $stage = 'clean-destination-is-junction'
    $cleanup.Add({ if (Test-Path -LiteralPath $programDirectory) { [IO.Directory]::Delete($programDirectory) } })
    New-Item -ItemType Junction -Path $programDirectory -Target $external | Out-Null
    $exit = Invoke-Installer $plan.Latest 4
    Assert-Sentinels $sentinels
    if (Test-Path -LiteralPath $productKey) { throw 'Registro criado apesar da recusa.' }
    Clear-Faults
    Add-Case $stage @{ ExitCode = $exit; ExternalFilesIntact = $true }

    $stage = 'clean-destination-unmarked-folder'
    New-Item -ItemType Directory -Path $programDirectory | Out-Null
    $unmarked = New-Sentinel (Join-Path $programDirectory 'qa-unmarked.txt') 'Unmarked folder content; must never change.'
    $exit = Invoke-Installer $plan.Latest 4
    Assert-Sentinels @($unmarked)
    if (@(Get-ChildItem -LiteralPath $programDirectory -Force).Count -ne 1 -or (Test-Path -LiteralPath $productKey)) { throw 'Pasta sem marcador alterada.' }
    Remove-Item -LiteralPath $unmarked.Path; Remove-Item -LiteralPath $programDirectory
    Add-Case $stage @{ ExitCode = $exit; FolderPreserved = $true }

    $stage = 'install-ignores-D'
    $other = Join-Path $workDirectory 'other-destination'
    $exit = Invoke-Installer $plan.Latest 0 ('/D=' + $other)
    $files = Assert-Payload $plan.Latest
    if (Test-Path -LiteralPath $other) { throw '/D foi aceito.' }
    Assert-NoProductProcesses
    $installed = Capture-State 'matrix-installed-035'
    Add-Case $stage @{ ExitCode = $exit; FilesVerified = $files; AlternateDestinationCreated = $false; State = $installed }
    $baseline = Get-ProductState

    $stage = 'process-guard'
    $probeDirectory = Join-Path $workDirectory 'probe'
    New-Item -ItemType Directory -Path $probeDirectory | Out-Null
    $probePath = Join-Path $probeDirectory 'NetLane.QaProbe.exe'
    Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\PING.EXE') -Destination $probePath
    $probe = Start-Process -FilePath $probePath -ArgumentList '-n 600 127.0.0.1' -PassThru -WindowStyle Hidden
    try {
        Start-Sleep -Seconds 1
        $installExit = Invoke-Installer $plan.Latest 2
        $uninstallExit = Invoke-Uninstaller 2
        $probe.Refresh()
        if ($probe.HasExited) { throw 'Processo de prova encerrado pelo instalador.' }
        Assert-SameState $baseline 'guarda de processo'
    } finally {
        if (-not $probe.HasExited) { $probe.Kill(); $probe.WaitForExit() }
    }
    Assert-NoProductProcesses
    Add-Case $stage @{ InstallExitCode = $installExit; UninstallExitCode = $uninstallExit; ProbeSurvived = $true; StatePreserved = $true; Probe = 'copia de PING.EXE com prefixo NetLane.; nao e a UI real' }

    $stage = 'file-in-use'
    $locked = Join-Path $programDirectory 'NetLane.Core.dll'
    $stream = [IO.File]::Open($locked, 'Open', 'Read', 'None')
    try {
        $installExit = Invoke-Installer $plan.Latest 7
        $uninstallExit = Invoke-Uninstaller 7
    } finally { $stream.Dispose() }
    Assert-SameState $baseline 'arquivo em uso'
    Add-Case $stage @{ InstallExitCode = $installExit; UninstallExitCode = $uninstallExit; LockedFile = 'NetLane.Core.dll'; StatePreserved = $true }

    $swaps = @(
        @{ Name = 'subfolder-is-junction'; Path = (Join-Path $programDirectory 'notices'); Kind = 'Junction'; Target = (Join-Path $external 'notices') },
        @{ Name = 'file-is-folder'; Path = $locked; Kind = 'Directory' },
        @{ Name = 'file-is-symlink'; Path = $locked; Kind = 'SymbolicLink'; Target = (Join-Path $external 'NetLane.Core.dll') },
        @{ Name = 'shortcut-is-folder'; Path = $shortcut; Kind = 'Directory' },
        @{ Name = 'startmenu-is-junction'; Path = $startMenuDirectory; Kind = 'Junction'; Target = $external }
    )
    foreach ($swap in $swaps) {
        $stage = 'invalid-' + $swap.Name
        $aside = $swap.Path + '.qa-aside'
        Rename-Item -LiteralPath $swap.Path -NewName (Split-Path -Leaf $aside)
        $path = $swap.Path
        $cleanup.Add({
            $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
            if ($item) { if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { $item.Delete() } else { Remove-Item -LiteralPath $path -Force } }
            Rename-Item -LiteralPath $aside -NewName (Split-Path -Leaf $path)
        }.GetNewClosure())
        if ($swap.Kind -eq 'Directory') { New-Item -ItemType Directory -Path $swap.Path | Out-Null }
        else { New-Item -ItemType $swap.Kind -Path $swap.Path -Target $swap.Target | Out-Null }
        $installExit = Invoke-Installer $plan.Latest 4
        $uninstallExit = Invoke-Uninstaller 4
        Assert-Sentinels $sentinels
        if (-not (Test-Path -LiteralPath $aside)) { throw 'Item original movido foi alterado.' }
        Clear-Faults
        Assert-SameState $baseline $swap.Name
        Add-Case $stage @{ InstallExitCode = $installExit; UninstallExitCode = $uninstallExit; ExternalFilesIntact = $true; StatePreserved = $true }
    }

    $stage = 'moved-uninstaller'
    $moved = Join-Path $workDirectory 'moved'
    New-Item -ItemType Directory -Path $moved | Out-Null
    Copy-Item -LiteralPath (Join-Path $programDirectory 'Uninstall.exe') -Destination $moved
    $movedSentinel = New-Sentinel (Join-Path $moved 'qa-moved.txt') 'Next to a moved uninstaller; must never change.'
    $exit = Invoke-Uninstaller 4 $moved (Join-Path $moved 'Uninstall.exe')
    Assert-Sentinels @($movedSentinel)
    Assert-SameState $baseline 'desinstalador movido'
    Add-Case $stage @{ ExitCode = $exit; StatePreserved = $true }

    $stage = 'uninstall-fails-on-shortcut'
    Set-FileDeny $shortcut 'Delete'
    Set-FileDeny $startMenuDirectory 'DeleteSubdirectoriesAndFiles'
    $exit = Invoke-Uninstaller 7
    Clear-Faults
    if (-not (Test-Path -LiteralPath $productKey) -or -not (Test-Path -LiteralPath (Join-Path $programDirectory 'Uninstall.exe')) -or
        -not (Test-Path -LiteralPath (Join-Path $programDirectory 'netlane-installed.layout')) -or -not (Test-Path -LiteralPath $shortcut)) {
        throw 'Marcador, desinstalador, registro ou atalho removidos antes da falha.'
    }
    if (Test-Path -LiteralPath (Join-Path $programDirectory 'NetLane.UI.exe')) { throw 'Falha induzida antes da remocao do payload.' }
    Assert-Sentinels @($profileFixture)
    $partial = Capture-State 'matrix-uninstall-failed-shortcut'
    $retryExit = Invoke-Uninstaller 0
    Assert-Removed
    Assert-Sentinels @($profileFixture)
    Add-Case $stage @{ ExitCode = $exit; MarkerAndRegistryKept = $true; RetryExitCode = $retryExit; RetryRemoved = $true; State = $partial }

    $stage = 'uninstall-fails-on-registry'
    $exit = Invoke-Installer $plan.Latest 0
    $null = Assert-Payload $plan.Latest
    $registryKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane', $true)
    $registryRule = New-Object Security.AccessControl.RegistryAccessRule($everyone, 'Delete', 'None', 'None', 'Deny')
    try { $acl = $registryKey.GetAccessControl(); $acl.AddAccessRule($registryRule); $registryKey.SetAccessControl($acl) } finally { $registryKey.Close() }
    $cleanup.Add({
        $k = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane', $true)
        try { $a = $k.GetAccessControl(); [void]$a.RemoveAccessRule($registryRule); $k.SetAccessControl($a) } finally { $k.Close() }
    }.GetNewClosure())
    $exit = Invoke-Uninstaller 7
    Clear-Faults
    if (-not (Test-Path -LiteralPath $productKey) -or -not (Test-Path -LiteralPath (Join-Path $programDirectory 'Uninstall.exe')) -or
        -not (Test-Path -LiteralPath (Join-Path $programDirectory 'netlane-installed.layout'))) { throw 'Marcador ou desinstalador removidos antes da falha.' }
    if (Test-Path -LiteralPath $shortcut) { throw 'Falha induzida antes da remocao do atalho.' }
    Assert-Sentinels @($profileFixture)
    $partial = Capture-State 'matrix-uninstall-failed-registry'
    $retryExit = Invoke-Uninstaller 0
    Assert-Removed
    Assert-Sentinels @($profileFixture)
    Add-Case $stage @{ ExitCode = $exit; MarkerKept = $true; RetryExitCode = $retryExit; RetryRemoved = $true; State = $partial }

    $stage = 'copy-failure-and-repair'
    $exit = Invoke-Installer $plan.Previous 0
    $null = Assert-Payload $plan.Previous
    Set-FileDeny (Join-Path $programDirectory 'notices') 'WriteData'
    $failedExit = Invoke-Installer $plan.Latest 7
    Clear-Faults
    $previousHashes = @{}; foreach ($entry in (Get-Manifest $plan.Previous).Files) { $previousHashes[$entry.Path] = $entry.Sha256 }
    $summary = @{ Latest = 0; Previous = 0; Missing = 0; Other = 0 }
    foreach ($entry in (Get-Manifest $plan.Latest).Files) {
        $path = Join-Path $programDirectory $entry.Path
        if (-not (Test-Path -LiteralPath $path)) { $summary.Missing++; continue }
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
        if ($hash -eq $entry.Sha256) { $summary.Latest++ } elseif ($hash -eq $previousHashes[$entry.Path]) { $summary.Previous++ } else { $summary.Other++ }
    }
    if ((Get-ItemProperty -LiteralPath $productKey).DisplayVersion -ne $plan.Previous.Version) { throw 'Versao registrada mudou apesar da falha.' }
    if ($summary.Missing -eq 0 -or $summary.Other -ne 0) { throw 'Falha de copia nao ocorreu no ponto induzido.' }
    Assert-Sentinels @($profileFixture)
    $partial = Capture-State 'matrix-copy-failed'
    $repairExit = Invoke-Installer $plan.Latest 0
    $files = Assert-Payload $plan.Latest
    foreach ($relative in $plan.RetiredPayloadFiles) { if (Test-Path -LiteralPath (Join-Path $programDirectory $relative)) { throw "Arquivo antigo permaneceu: $relative" } }
    Assert-Sentinels @($profileFixture)
    $repaired = Capture-State 'matrix-repaired'
    $removeExit = Invoke-Uninstaller 0
    Assert-Removed
    Add-Case $stage @{ FailedExitCode = $failedExit; PartialPayload = $summary; RegisteredVersionKept = $plan.Previous.Version
        RepairExitCode = $repairExit; FilesVerified = $files; FinalRemovalExitCode = $removeExit; PartialState = $partial; RepairedState = $repaired
        AutomaticRollback = $false }

    Assert-NoProductProcesses
    Assert-Sentinels $sentinels
    if ((Get-StartupValues) -cne $startupBefore) { throw 'Entradas de inicializacao alteradas.' }
    Remove-Item -LiteralPath $profileFixture.Path; Remove-Item -LiteralPath $profileDirectory
    $after = Capture-State 'matrix-after'
    $stateBefore = Get-Content -LiteralPath $before.State -Raw -Encoding UTF8 | ConvertFrom-Json
    $stateAfter = Get-Content -LiteralPath $after.State -Raw -Encoding UTF8 | ConvertFrom-Json
    $networkComparisons = @(foreach ($field in @('Adapters','Bindings','IpInterfaces','PersistentIpInterfaces','IpAddresses','DefaultRoutes','Dns','GlobalNetworkFlags')) {
        [pscustomobject]@{ Field = $field; Same = (($stateBefore.$field | ConvertTo-Json -Depth 12 -Compress) -ceq ($stateAfter.$field | ConvertTo-Json -Depth 12 -Compress)) }
    })
    $networkPreserved = @($networkComparisons | Where-Object { -not $_.Same }).Count -eq 0
    Write-Receipt 'result.json' ([ordered]@{
        Passed = $networkPreserved; Cases = @($cases.ToArray()); NetworkComparisons = $networkComparisons
        NetworkStatePreserved = $networkPreserved; StartupValuesPreserved = $true; LabCleanAtEnd = $true
        NotCovered = @('cancelamento na pagina inicial','UI real/bandeja e outra sessao','segunda checagem isolada da Section','autorun pela UI','logon real','UI instalada')
        FullInstallerQaVerified = $false; RealLogonVerified = $false; NetworkTestsExecuted = $false
        HostInstallation = $false; Signed = $false; FinishedUtc = [DateTime]::UtcNow.ToString('o')
    })
    if (-not $networkPreserved) { throw 'Estado de rede variou; conferir o recibo, sem reconfigurar.' }
} catch {
    $failure = $_
    try { Clear-Faults } catch { }
    if (-not (Test-Path -LiteralPath (Join-Path $outputDirectory 'result.json'))) {
        Write-Receipt 'result.json' ([ordered]@{
            Passed = $false; Stage = $stage; Error = $failure.Exception.Message; Cases = @($cases.ToArray())
            FullInstallerQaVerified = $false; RealLogonVerified = $false; NetworkTestsExecuted = $false
            HostInstallation = $false; Signed = $false; FinishedUtc = [DateTime]::UtcNow.ToString('o')
        })
    }
    throw $failure
}
