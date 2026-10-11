[CmdletBinding()]
param(
    [string]$PlanPath = 'C:\NetLaneRustQaInput\plan.json',
    [ValidateSet('Preflight','Runtime','Install','LegacyRefusal','AdminRefusal')][string]$Mode = 'Preflight',
    [ValidatePattern('^[a-z0-9-]{1,40}$')][string]$Label = 'preflight',
    [ValidatePattern('^[a-z0-9-]{1,40}$')][string]$BaselineLabel = 'preflight'
)
# Dedicated Rust Sandbox round. No host execution or protection changes.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$plan = Get-Content -Raw -LiteralPath $PlanPath -Encoding UTF8 | ConvertFrom-Json
if ($env:COMPUTERNAME -eq $plan.HostComputer) { throw 'Execucao no host recusada.' }
if ($plan.Schema -ne 1 -or $plan.Product -ne 'NetLaneRustPreview' -or -not [Environment]::Is64BitProcess) { throw 'Plano/plataforma invalido.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$machine = Get-CimInstance Win32_ComputerSystem
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $machine.Model -ne 'Virtual Machine' -or $machine.Manufacturer -ne 'Microsoft Corporation') {
    throw 'Este ensaio exige uma nova sessao Windows Sandbox com WDAGUtilityAccount.'
}
if ([IO.Path]::GetFullPath($PlanPath) -notmatch '^C:\\NetLaneRustQaInput\\plan(?:-runtime)?\.json$') { throw 'Plano fora da pasta mapeada prevista.' }
$inputRoot = 'C:\NetLaneRustQaInput'
$outputRoot = 'C:\NetLaneRustQaOutput'
foreach ($path in @($inputRoot,$outputRoot)) {
    $item = Get-Item -LiteralPath $path -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Mapeamento invalido.' }
}
function Write-NewJson([string]$Path,$Value) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 20))
    $stream = [IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $stream.Write($bytes,0,$bytes.Length) } finally { $stream.Dispose() }
}
function Get-TreeState([string]$Root) {
    $entries = @()
    if (Test-Path -LiteralPath $Root) {
        $queue = [Collections.Generic.Queue[string]]::new()
        $queue.Enqueue($Root)
        while ($queue.Count) {
            foreach ($item in Get-ChildItem -LiteralPath ($queue.Dequeue()) -Force | Sort-Object Name) {
                $relative = $item.FullName.Substring($Root.Length+1).Replace('\','/')
                if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                    $entries += [ordered]@{Path=$relative;Kind='Link';Followed=$false}
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
function Get-RunHashes {
    $values = @()
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
    if ($null -eq $key) { return ,$values }
    try {
        foreach ($name in $key.GetValueNames() | Sort-Object) {
            $value = @($key.GetValueKind($name).ToString(),$key.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames))
            $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($value | ConvertTo-Json -Depth 5 -Compress))
            $sha = [Security.Cryptography.SHA256]::Create()
            try { $hash = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-','') } finally { $sha.Dispose() }
            $values += [ordered]@{Name=$name;Sha256=$hash}
        }
    } finally { $key.Close() }
    return ,$values
}
function Get-ProductState {
    $local = [Environment]::GetFolderPath('LocalApplicationData')
    # Per-machine package: Program Files, all-users Start menu, HKLM registration.
    $shortcut = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'NetLane Rust.lnk'
    $registration = $null
    $regPath = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLaneRustPreview'
    if (Test-Path -LiteralPath $regPath) { $registration = Get-ItemProperty -LiteralPath $regPath | Select-Object DisplayName,DisplayVersion,InstallLocation,UninstallString }
    $shortcutHash = $null
    if (Test-Path -LiteralPath $shortcut) { $shortcutHash = (Get-FileHash -LiteralPath $shortcut).Hash }
    return [ordered]@{
        Program=(Get-TreeState (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane Rust'))
        LegacyUserProgram=(Get-TreeState (Join-Path $local 'Programs\NetLane Rust'))
        Profile=(Get-TreeState (Join-Path $local 'NetLane-Rust'))
        LegacyProgram=(Get-TreeState (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane'))
        LegacyProfile=(Get-TreeState (Join-Path $local 'NetLane'))
        Registration=$registration; ShortcutSha256=$shortcutHash; Run=(Get-RunHashes)
        Processes=@(Get-Process | Where-Object { $_.ProcessName -match '^NetLane([.-]|$)' } | Select-Object Id,ProcessName,Path)
    }
}
# Allow rules that let non-administrators change the elevated agent or its folder.
function Get-UserWritableRules([string]$Path) {
    $sids = @('S-1-1-0','S-1-5-11','S-1-5-32-545','S-1-5-4')
    $write = [Security.AccessControl.FileSystemRights]'WriteData, AppendData, WriteExtendedAttributes, WriteAttributes, Delete, DeleteSubdirectoriesAndFiles, ChangePermissions, TakeOwnership'
    return ,@((Get-Acl -LiteralPath $Path).Access | Where-Object {
        $_.AccessControlType -eq 'Allow' -and ($_.FileSystemRights -band $write) -and
        $sids -contains $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
    } | ForEach-Object { "$($_.IdentityReference) $($_.FileSystemRights)" })
}
function Wait-Setup($Process) {
    if (-not $Process.WaitForExit(120000)) { throw 'Instalador ainda em execucao; nenhuma finalizacao forcada.' }
    return $Process.ExitCode
}
function Assert-NothingInstalled($State) {
    return -not $State.Program.Exists -and -not $State.Registration -and -not $State.ShortcutSha256 -and -not $State.Processes.Count
}
$setup = Join-Path $inputRoot $plan.Package.Installer
if ($plan.Package.Installer -notmatch '^NetLane-Rust-[A-Za-z0-9.-]+-setup\.exe$' -or
    (Get-FileHash -LiteralPath $setup).Hash -ne $plan.Package.Sha256 -or
    (Get-FileHash -LiteralPath (Join-Path $inputRoot 'package-manifest.json')).Hash -ne $plan.Package.ManifestSha256 -or
    (Get-FileHash -LiteralPath $PSCommandPath).Hash -ne $plan.RunnerSha256) { throw 'Insumo diferente do plano.' }
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
$guest = [ordered]@{Computer=$env:COMPUTERNAME;User=$identity.Name;Sid=$identity.User.Value;Model=$machine.Model;Manufacturer=$machine.Manufacturer;Elevated=$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)}
$result = [ordered]@{Schema=1;Product=$plan.Product;RoundId=$plan.RoundId;Mode=$Mode;Label=$Label;RunnerSha256=$plan.RunnerSha256;StartedAtUtc=[DateTime]::UtcNow.ToString('o');Identity=$guest;InstallerSha256=$plan.Package.Sha256;InstallerExecuted=$false;Verdict='Pending'}
try {
    if ($Mode -eq 'Preflight') {
        $runtimePath = Join-Path $env:WINDIR 'System32\vcruntime140.dll'
        $runtime = $null
        if (Test-Path -LiteralPath $runtimePath) { $runtime = (Get-Item -LiteralPath $runtimePath).VersionInfo.FileVersion }
        $ciState = $null
        $ciKey = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' -ErrorAction SilentlyContinue
        if ($ciKey -and $ciKey.PSObject.Properties['VerifiedAndReputablePolicyState']) { $ciState = $ciKey.VerifiedAndReputablePolicyState }
        $ciOutput = @(); $ciExit = $null
        if (Get-Command CiTool.exe -ErrorAction SilentlyContinue) { $ciOutput = @(& CiTool.exe -lp -json 2>&1 | ForEach-Object { $_.ToString() }); $ciExit=$LASTEXITCODE }
        $result.OperatingSystem = Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture
        $result.Video = @(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion)
        $result.Network = @(Get-NetAdapter | Select-Object Name,Status,MacAddress)
        $result.Prerequisites = [ordered]@{VisualCppX64=$runtime;Minimum=$plan.Package.VisualCppX64Minimum;IcuPresent=(Test-Path -LiteralPath (Join-Path $env:WINDIR 'System32\icuuc.dll'))}
        $result.CodeIntegrity = [ordered]@{VerifiedAndReputablePolicyState=$ciState;CiToolExitCode=$ciExit;CiToolOutput=$ciOutput}
        $result.State = Get-ProductState
        $result.Verdict = 'Collected'
    } else {
        $baseline = Get-Content -Raw -LiteralPath (Join-Path $outputRoot ($BaselineLabel+'.json')) -Encoding UTF8 | ConvertFrom-Json
        if ($baseline.RoundId -ne $plan.RoundId -or $baseline.Identity.Computer -ne $guest.Computer -or $baseline.Identity.Sid -ne $guest.Sid -or $baseline.Verdict -ne 'Collected') { throw 'Baseline de outra sessao ou incompleto.' }
        if ($baseline.State.Program.Exists -or $baseline.State.Profile.Exists -or $baseline.State.Registration -or $baseline.State.Processes.Count) { throw 'Sandbox nao esta limpo.' }
        if ($Mode -eq 'Runtime') {
            $redist = Join-Path $inputRoot 'VC_redist.x64.exe'
            if ((Get-FileHash -LiteralPath $redist).Hash -ne $plan.Runtime.Sha256) { throw 'Redistribuivel diferente do plano.' }
            $signature = Get-AuthenticodeSignature -LiteralPath $redist
            if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'Assinatura Microsoft do redistribuivel invalida.' }
            Write-NewJson (Join-Path $outputRoot 'runtime-attempt.json') ([ordered]@{RoundId=$plan.RoundId;AtUtc=$result.StartedAtUtc;Sha256=$plan.Runtime.Sha256;Signer=$signature.SignerCertificate.Subject})
            $process = Start-Process -FilePath $redist -ArgumentList '/install /quiet /norestart /log C:\NetLaneRustQaOutput\runtime-install.log' -PassThru
            $result.ProcessId = $process.Id
            $result.RuntimeExecuted = $true
            if ($process.WaitForExit(300000)) {
                $result.ExitCode = $process.ExitCode
                $result.Verdict = if ($process.ExitCode -eq 0) { 'RuntimeInstalled' } elseif ($process.ExitCode -eq 3010) { 'RestartRequired' } else { 'RuntimeFailed' }
            } else {
                $result.Verdict = 'RuntimeStillRunning'
            }
            $result.State = Get-ProductState
        } elseif ($Mode -eq 'LegacyRefusal') {
            # A fictitious marker of the earlier per-user preview must block the per-machine setup.
            $legacy = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\NetLane Rust'
            if (Test-Path -LiteralPath $legacy) { throw 'Fixture legada anterior.' }
            $null = New-Item -ItemType Directory -Path $legacy
            [IO.File]::WriteAllText((Join-Path $legacy 'netlane-rust-installed.layout'),'1',[Text.Encoding]::ASCII)
            try {
                $result.Before = Get-ProductState
                $process = Start-Process -FilePath $setup -ArgumentList '/S' -PassThru
                $result.InstallerExecuted = $true
                $result.ExitCode = Wait-Setup $process
                $result.After = Get-ProductState
            } finally { Remove-Item -LiteralPath $legacy -Recurse -Force }
            $result.Verdict = if ($result.ExitCode -eq 4 -and (Assert-NothingInstalled $result.After)) { 'Passed' } else { 'Failed' }
        } elseif ($Mode -eq 'AdminRefusal') {
            # A standard user bypassing the manifest (RunAsInvoker) must still be refused by the
            # setup itself (code 9). Temporary account and copy, removed afterwards.
            $work = 'C:\NetLaneQaAdminRefusal'
            if (Test-Path -LiteralPath $work) { throw 'Pasta anterior.' }
            $null = New-Item -ItemType Directory -Path $work
            $copy = Join-Path $work $plan.Package.Installer
            Copy-Item -LiteralPath $setup -Destination $copy
            if ((Get-FileHash -LiteralPath $copy).Hash -ne $plan.Package.Sha256) { throw 'Copia divergente.' }
            $name = 'nlqa' + [guid]::NewGuid().ToString('N').Substring(0,8)
            $password = ConvertTo-SecureString ([guid]::NewGuid().ToString('N') + 'Aa1!') -AsPlainText -Force
            $null = New-LocalUser -Name $name -Password $password -AccountNeverExpires -PasswordNeverExpires
            try {
                Add-LocalGroupMember -SID 'S-1-5-32-545' -Member $name
                $credential = [pscredential]::new("$env:COMPUTERNAME\$name", $password)
                $result.Before = Get-ProductState
                $env:__COMPAT_LAYER = 'RunAsInvoker'
                try { $process = Start-Process -FilePath $copy -ArgumentList '/S' -Credential $credential -WorkingDirectory $work -LoadUserProfile -PassThru }
                finally { Remove-Item Env:__COMPAT_LAYER }
                $result.InstallerExecuted = $true
                $result.StandardUser = $name
                $result.ExitCode = Wait-Setup $process
                $result.After = Get-ProductState
            } finally {
                Remove-LocalUser -Name $name
                Remove-Item -LiteralPath $work -Recurse -Force
            }
            $result.Verdict = if ($result.ExitCode -eq 9 -and (Assert-NothingInstalled $result.After)) { 'Passed' } else { 'Failed' }
        } else {
        if (-not $baseline.Prerequisites.IcuPresent -or -not $baseline.Prerequisites.VisualCppX64 -or [version]$baseline.Prerequisites.VisualCppX64 -lt [version]$plan.Package.VisualCppX64Minimum) { throw 'Dependencias ausentes; nao tentar o instalador.' }
        Write-NewJson (Join-Path $outputRoot 'install-attempt.json') ([ordered]@{RoundId=$plan.RoundId;AtUtc=$result.StartedAtUtc;InstallerSha256=$plan.Package.Sha256})
        $result.Before = Get-ProductState
        $result.InstallerLaunchAttempted = $true
        try {
            $process = Start-Process -FilePath $setup -ArgumentList '/S' -PassThru
            $result.InstallerExecuted = $true
            $result.ProcessId = $process.Id
            if (-not $process.WaitForExit(120000)) { throw 'Instalador ainda em execucao; nenhuma finalizacao forcada.' }
            $result.ExitCode = $process.ExitCode
            $result.Verdict = if ($process.ExitCode -eq 0) { 'InstalledPendingVerification' } else { 'InstallerFailed' }
        } catch {
            $result.Verdict = 'LaunchFailed'
            $result.Error = $_.Exception.ToString()
        }
        $result.After = Get-ProductState
        if ($result.Verdict -eq 'InstalledPendingVerification') {
            $programRoot = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane Rust'
            $result.UserWritable = [ordered]@{
                Folder=(Get-UserWritableRules $programRoot)
                Agent=(Get-UserWritableRules (Join-Path $programRoot 'netlane-agent.exe'))
            }
            if ($result.UserWritable.Folder.Count -or $result.UserWritable.Agent.Count) { $result.Verdict = 'InstalledAclInvalid' }
        }
        if ($result.Verdict -eq 'InstalledPendingVerification' -and $plan.Package.PSObject.Properties['ExpectedDisplayName']) {
            $result.RegistrationNameValid = $result.After.Registration.DisplayName -ceq $plan.Package.ExpectedDisplayName
            $result.RegistrationNameCodepoints = @($result.After.Registration.DisplayName.ToCharArray() | ForEach-Object { [int]$_ })
            if (-not $result.RegistrationNameValid) { $result.Verdict = 'InstalledMetadataInvalid' }
        }
        $events = @(Get-WinEvent -FilterHashtable @{LogName='Microsoft-Windows-CodeIntegrity/Operational';StartTime=([DateTime]::Parse($result.StartedAtUtc).ToLocalTime()).AddSeconds(-2)} -ErrorAction SilentlyContinue)
        $result.CodeIntegrityEvents = @($events | Select-Object TimeCreated,Id,RecordId,Message)
        if ($events.Count) { [IO.File]::WriteAllText((Join-Path $outputRoot 'install-code-integrity.xml'),('<Events>' + (($events | ForEach-Object { $_.ToXml() }) -join "`n") + '</Events>'),[Text.UTF8Encoding]::new($false)) }
        }
    }
} catch {
    $result.Verdict = 'Failed'
    $result.Error = $_.Exception.ToString()
} finally {
    $result.CompletedAtUtc = [DateTime]::UtcNow.ToString('o')
    $receiptName = if ($Mode -eq 'Preflight') { $Label } else { $Mode.ToLowerInvariant() }
    Write-NewJson (Join-Path $outputRoot ($receiptName+'.json')) $result
}
if ($result.Verdict -notin @('Collected','RuntimeInstalled','InstalledPendingVerification','Passed')) { exit 1 }
