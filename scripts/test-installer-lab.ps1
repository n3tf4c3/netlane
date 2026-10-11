[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$PlanPath)

# Four explicitly authorized installer cases in an identified VMware lab guest.
# No UI/service startup, real logon, network changes, protection changes or signing.
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
    -not @(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -eq $plan.Target.IPv4 }).Count -or
    -not $plan.Target.SnapshotVerified) {
    throw 'Identidade ou snapshot do alvo diferente do plano autorizado.'
}
if (-not (New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Token administrativo ausente; nenhuma elevacao automatica.'
}
$labRoot = Split-Path -Parent ([IO.Path]::GetFullPath($PlanPath))
if ($labRoot -ne $plan.Target.LabRoot) { throw 'Pasta de QA diferente do plano.' }
$inputDirectory = Join-Path $labRoot 'input'
$outputDirectory = Join-Path $labRoot 'artifacts\results'
$collector = Join-Path $labRoot 'scripts\capture-installer-qa-state.ps1'
$programDirectory = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane'
$profileDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'NetLane'
$startMenuDirectory = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'NetLane'
$productKey = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane'
$runKeyPath = 'Software\Microsoft\Windows\CurrentVersion\Run'
foreach ($root in @($labRoot, $inputDirectory, (Split-Path -Parent $collector))) {
    $item = Get-Item -LiteralPath $root -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Pasta de QA invalida.' }
}
if ((Test-Path -LiteralPath $programDirectory) -or (Test-Path -LiteralPath $profileDirectory) -or
    (Test-Path -LiteralPath $productKey) -or (Test-Path -LiteralPath $startMenuDirectory)) {
    throw 'O laboratorio deve estar sem instalacao, perfil ou atalhos anteriores do NetLane.'
}
if (Test-Path -LiteralPath $outputDirectory) { throw 'Preserve os recibos anteriores; use uma rodada nova.' }
$utf8 = New-Object System.Text.UTF8Encoding($false)
$cases = New-Object 'System.Collections.Generic.List[object]'
$stage = 'preflight'

function Assert-NoProductProcesses {
    if (@(Get-Process -Name 'NetLane.UI','NetLane.Service' -ErrorAction SilentlyContinue).Count) {
        throw 'Painel ou servico presentes; nenhum encerramento forcado.'
    }
}
function Get-StartupValues {
    $values = @()
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($runKeyPath)
    if ($null -eq $key) { return ,$values }
    try {
        foreach ($name in $key.GetValueNames() | Sort-Object) {
            $kind = $key.GetValueKind($name).ToString()
            $value = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $encoded = @($kind, $value) | ConvertTo-Json -Depth 5 -Compress
            $sha = [Security.Cryptography.SHA256]::Create()
            try { $hash = [BitConverter]::ToString($sha.ComputeHash($utf8.GetBytes($encoded))).Replace('-', '') }
            finally { $sha.Dispose() }
            $values += [pscustomobject]@{ Name = $name; Kind = $kind; Sha256 = $hash }
        }
    } finally { $key.Close() }
    return ,$values
}
function Write-Receipt([string]$Name, $Value) {
    $path = Join-Path $outputDirectory $Name
    if (Test-Path -LiteralPath $path) { throw 'Recibo anterior nao pode ser substituido.' }
    [IO.File]::WriteAllText($path, ($Value | ConvertTo-Json -Depth 12), $utf8)
}
function Capture-State([string]$Label) {
    $receipt = & $collector -Label $Label | ConvertFrom-Json
    if (-not $receipt.State -or (Get-FileHash -LiteralPath $receipt.State -Algorithm SHA256).Hash -ne $receipt.Sha256) {
        throw 'Estado de QA incompleto.'
    }
    return $receipt
}
function Assert-Package($Package) {
    foreach ($name in @($Package.Installer, $Package.Manifest)) {
        if ($name -ne [IO.Path]::GetFileName($name)) { throw 'Nome de insumo invalido.' }
    }
    if ((Get-FileHash -LiteralPath (Join-Path $inputDirectory $Package.Installer) -Algorithm SHA256).Hash -ne $Package.Sha256 -or
        (Get-FileHash -LiteralPath (Join-Path $inputDirectory $Package.Manifest) -Algorithm SHA256).Hash -ne $Package.ManifestSha256) {
        throw 'Hash de insumo diferente do plano.'
    }
}
function Invoke-LabInstaller($Package, [int]$ExpectedExitCode) {
    Assert-Package $Package
    Assert-NoProductProcesses
    $process = Start-Process -FilePath (Join-Path $inputDirectory $Package.Installer) -ArgumentList '/S' -PassThru -WindowStyle Hidden
    # Cache the handle now: a fast refusal (e.g. downgrade, code 6) can exit before
    # WaitForExit, and PowerShell then reports an empty ExitCode.
    $null = $process.Handle
    if (-not $process.WaitForExit(300000)) { throw 'Instalador nao concluiu; nenhum processo encerrado a forca.' }
    $process.Refresh()
    if ($process.ExitCode -ne $ExpectedExitCode) { throw "Saida inesperada: $($process.ExitCode); esperado $ExpectedExitCode." }
    Assert-NoProductProcesses
    return $process.ExitCode
}
function Assert-Payload($Package) {
    $manifest = Get-Content -LiteralPath (Join-Path $inputDirectory $Package.Manifest) -Raw -Encoding UTF8 | ConvertFrom-Json
    if ((Get-ItemProperty -LiteralPath $productKey).DisplayVersion -ne $Package.Version) { throw 'Versao instalada diferente.' }
    $entries = @($manifest.Files) + @([pscustomobject]@{ Path = 'package-manifest.json'; Sha256 = $Package.ManifestSha256 })
    foreach ($entry in $entries) {
        $path = [IO.Path]::GetFullPath((Join-Path $programDirectory $entry.Path))
        if (-not $path.StartsWith($programDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Caminho fora do programa.' }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw "Arquivo instalado diferente: $($entry.Path)" }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $startMenuDirectory 'NetLane.lnk'))) { throw 'Atalho instalado ausente.' }
    return $entries.Count
}
function Assert-Fixtures($Protected) {
    foreach ($entry in $Protected) {
        if ((Get-FileHash -LiteralPath $entry.Path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw 'Fixture alterada.' }
    }
}

Assert-NoProductProcesses
$startupBefore = Get-StartupValues
if (@($startupBefore | Where-Object { $_.Name -eq 'NetLane' }).Count) { throw 'Autorun anterior do NetLane presente; nao sobrescrever.' }
Assert-Package $plan.Previous
Assert-Package $plan.Latest
New-Item -ItemType Directory -Path $outputDirectory | Out-Null
try {
    Write-Receipt 'started.json' ([ordered]@{
        Computer = $env:COMPUTERNAME; UserSid = $identity.User.Value; Uuid = $machineProduct.UUID
        Snapshot = $plan.Target.SnapshotName; StartedUtc = [DateTime]::UtcNow.ToString('o')
        HostInstallation = $false; NetworkTests = $false; RealLogon = $false
    })
    $before = Capture-State 'lab-before'
    $stage = 'clean-install-0.3.4'
    $exitCode = Invoke-LabInstaller $plan.Previous 0
    $files = Assert-Payload $plan.Previous
    $state = Capture-State 'installed-034'
    $cases.Add([ordered]@{ Case = $stage; Passed = $true; ExitCode = $exitCode; FilesVerified = $files; State = $state })
    New-Item -ItemType Directory -Path $profileDirectory | Out-Null
    $profileFile = Join-Path $profileDirectory 'netlane-rules.json'
    [IO.File]::WriteAllText($profileFile, '[]', $utf8)
    $unknownFile = Join-Path $programDirectory 'qa-preserved.txt'
    [IO.File]::WriteAllText($unknownFile, 'Disposable QA file; preserve exactly.', $utf8)
    $unknownNotice = Join-Path $programDirectory 'notices\qa-preserved.txt'
    [IO.File]::WriteAllText($unknownNotice, 'Disposable QA notice; preserve exactly.', $utf8)
    $protected = @(foreach ($path in @($profileFile, $unknownFile, $unknownNotice)) {
        [pscustomobject]@{ Path = $path; Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash }
    })
    Write-Receipt 'fixtures.json' $protected
    $stage = 'upgrade-0.3.4-to-0.3.5'
    $exitCode = Invoke-LabInstaller $plan.Latest 0
    $files = Assert-Payload $plan.Latest
    Assert-Fixtures $protected
    foreach ($relative in $plan.RetiredPayloadFiles) {
        $retiredPath = [IO.Path]::GetFullPath((Join-Path $programDirectory $relative))
        if (-not $retiredPath.StartsWith($programDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Limite do inventario antigo invalido.' }
        if (Test-Path -LiteralPath $retiredPath) { throw "Arquivo antigo permaneceu: $relative" }
    }
    $beforeDowngrade = Capture-State 'upgrade-035'
    $cases.Add([ordered]@{ Case = $stage; Passed = $true; ExitCode = $exitCode; FilesVerified = $files; ProtectedFixtures = $protected.Count; State = $beforeDowngrade })
    $stage = 'downgrade-refused'
    $exitCode = Invoke-LabInstaller $plan.Previous 6
    $files = Assert-Payload $plan.Latest
    Assert-Fixtures $protected
    $afterDowngrade = Capture-State 'downgrade-refused'
    $stateBefore = Get-Content -LiteralPath $beforeDowngrade.State -Raw -Encoding UTF8 | ConvertFrom-Json
    $stateAfter = Get-Content -LiteralPath $afterDowngrade.State -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($field in @('ProductRegistration','StartupCommandCurrentUser','Program','Profile','StartMenu')) {
        if (($stateBefore.$field | ConvertTo-Json -Depth 12 -Compress) -cne ($stateAfter.$field | ConvertTo-Json -Depth 12 -Compress)) { throw "Estado alterado no downgrade: $field" }
    }
    $cases.Add([ordered]@{ Case = $stage; Passed = $true; ExitCode = $exitCode; FilesVerified = $files; StatePreserved = $true; State = $afterDowngrade })
    $stage = 'uninstall-preserves-fixtures-removes-own-autorun'
    if (($startupBefore | ConvertTo-Json -Depth 6 -Compress) -cne ((Get-StartupValues) | ConvertTo-Json -Depth 6 -Compress)) { throw 'Autorun alterado antes da fixture.' }
    $runKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($runKeyPath)
    try { $runKey.SetValue('NetLane', ('"' + (Join-Path $programDirectory 'NetLane.UI.exe') + '" --startup'), [Microsoft.Win32.RegistryValueKind]::String) }
    finally { $runKey.Close() }
    $uninstaller = Join-Path $programDirectory 'Uninstall.exe'
    Assert-NoProductProcesses
    $process = Start-Process -FilePath $uninstaller -ArgumentList '/S' -PassThru -WindowStyle Hidden
    $null = $process.Handle
    if (-not $process.WaitForExit(300000)) { throw 'Remocao nao concluiu; nenhum processo encerrado a forca.' }
    $process.Refresh()
    if ($process.ExitCode -ne 0) { throw "Remocao falhou: $($process.ExitCode)." }
    $deadline = [DateTime]::UtcNow.AddSeconds(90)
    while ((Test-Path -LiteralPath $productKey) -or (Test-Path -LiteralPath $uninstaller) -or
           (Test-Path -LiteralPath (Join-Path $startMenuDirectory 'NetLane.lnk'))) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Remocao permaneceu incompleta apos encerrar o lancador.' }
        Start-Sleep -Milliseconds 500
    }
    Assert-Fixtures $protected
    if (($startupBefore | ConvertTo-Json -Depth 6 -Compress) -cne ((Get-StartupValues) | ConvertTo-Json -Depth 6 -Compress)) { throw 'Autorun esperado permaneceu ou entrada alheia foi alterada.' }
    $manifest = Get-Content -LiteralPath (Join-Path $inputDirectory $plan.Latest.Manifest) -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($entry in (@($manifest.Files) + @([pscustomobject]@{ Path = 'package-manifest.json' }))) {
        if (Test-Path -LiteralPath (Join-Path $programDirectory $entry.Path)) { throw "Payload permaneceu: $($entry.Path)" }
    }
    Assert-NoProductProcesses
    $after = Capture-State 'lab-after'
    $cases.Add([ordered]@{ Case = $stage; Passed = $true; ExitCode = $process.ExitCode; ProtectedFixtures = $protected.Count; OtherStartupValuesPreserved = $true; State = $after })
    $stateBefore = Get-Content -LiteralPath $before.State -Raw -Encoding UTF8 | ConvertFrom-Json
    $stateAfter = Get-Content -LiteralPath $after.State -Raw -Encoding UTF8 | ConvertFrom-Json
    $networkComparisons = @(foreach ($field in @('Adapters','Bindings','IpInterfaces','PersistentIpInterfaces','IpAddresses','DefaultRoutes','Dns','GlobalNetworkFlags')) {
        [pscustomobject]@{ Field = $field; Same = (($stateBefore.$field | ConvertTo-Json -Depth 12 -Compress) -ceq ($stateAfter.$field | ConvertTo-Json -Depth 12 -Compress)) }
    })
    $networkPreserved = @($networkComparisons | Where-Object { -not $_.Same }).Count -eq 0
    Write-Receipt 'result.json' ([ordered]@{
        Passed = $networkPreserved; Cases = @($cases.ToArray()); NetworkComparisons = $networkComparisons
        NetworkStatePreserved = $networkPreserved; FixtureFiles = $protected; StartupValuesPreserved = $true
        FullInstallerQaVerified = $false; RealLogonVerified = $false; NetworkTestsExecuted = $false
        HostInstallation = $false; Signed = $false; FinishedUtc = [DateTime]::UtcNow.ToString('o')
    })
    if (-not $networkPreserved) { throw 'Estado de rede variou; conferir o recibo, sem reconfigurar.' }
} catch {
    if (-not (Test-Path -LiteralPath (Join-Path $outputDirectory 'result.json'))) {
        Write-Receipt 'result.json' ([ordered]@{
            Passed = $false; Stage = $stage; Error = $_.Exception.Message; Cases = @($cases.ToArray())
            FullInstallerQaVerified = $false; RealLogonVerified = $false; NetworkTestsExecuted = $false
            HostInstallation = $false; Signed = $false; FinishedUtc = [DateTime]::UtcNow.ToString('o')
        })
    }
    throw
}
