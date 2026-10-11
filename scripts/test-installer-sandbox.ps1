[CmdletBinding()]
param()

# Run only inside the explicitly authorized, disposable Windows Sandbox.
# No routing, UI/service startup, protection changes, or host installation.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$inputDirectory = 'C:\NetLaneQaInput'
$outputDirectory = 'C:\NetLaneQaOutput'
$machine = Get-CimInstance Win32_ComputerSystem
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $machine.Model -ne 'Virtual Machine' -or -not [Environment]::Is64BitProcess) {
    throw 'Este roteiro só pode executar no Windows Sandbox descartável.'
}
$plan = Get-Content -LiteralPath (Join-Path $inputDirectory 'plan.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($env:COMPUTERNAME -eq $plan.HostComputer) { throw 'Execução no host recusada.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not (New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'O roteiro precisa do token administrativo do Sandbox; não eleva automaticamente.'
}
$programDirectory = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane'
$profileDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'NetLane'
$productKey = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane'
if ((Test-Path -LiteralPath $programDirectory) -or (Test-Path -LiteralPath $profileDirectory) -or (Test-Path -LiteralPath $productKey)) {
    throw 'Use uma sessão de Sandbox nova, sem NetLane ou perfil anterior.'
}
$utf8 = New-Object System.Text.UTF8Encoding($false)
$cases = New-Object 'System.Collections.Generic.List[object]'

function Write-Receipt([string]$Name, $Value) {
    [IO.File]::WriteAllText((Join-Path $outputDirectory $Name), ($Value | ConvertTo-Json -Depth 8), $utf8)
}
function Invoke-LabInstaller($Package, [int]$ExpectedExitCode) {
    if ($Package.Installer -ne [IO.Path]::GetFileName($Package.Installer)) { throw 'Nome de instalador inválido.' }
    $path = Join-Path $inputDirectory $Package.Installer
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $Package.Sha256) { throw 'Hash do instalador diferente do plano.' }
    $process = Start-Process -FilePath $path -ArgumentList '/S' -PassThru -WindowStyle Hidden
    if (-not $process.WaitForExit(300000)) { throw 'Instalador não concluiu; nenhum processo foi encerrado à força.' }
    $process.Refresh()
    if ($process.ExitCode -ne $ExpectedExitCode) { throw "Saída inesperada: $($process.ExitCode); esperado $ExpectedExitCode." }
    if (@(Get-Process -Name 'NetLane.UI','NetLane.Service' -ErrorAction SilentlyContinue).Count) { throw 'Painel/serviço iniciado inesperadamente.' }
    return $process.ExitCode
}
function Assert-LabPayload($Package) {
    $manifest = Get-Content -LiteralPath (Join-Path $inputDirectory $Package.Manifest) -Raw -Encoding UTF8 | ConvertFrom-Json
    if ((Get-ItemProperty -LiteralPath $productKey).DisplayVersion -ne $Package.Version) { throw 'Versão instalada diferente.' }
    $entries = @($manifest.Files) + @([pscustomobject]@{ Path = 'package-manifest.json'; Sha256 = $Package.ManifestSha256 })
    foreach ($entry in $entries) {
        $path = [IO.Path]::GetFullPath((Join-Path $programDirectory $entry.Path))
        if (-not $path.StartsWith($programDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Caminho fora do programa.' }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw "Arquivo instalado diferente: $($entry.Path)" }
    }
    return $entries.Count
}

try {
    Write-Receipt 'started.json' ([ordered]@{ Computer = $env:COMPUTERNAME; User = $env:USERNAME; StartedUtc = [DateTime]::UtcNow.ToString('o'); HostInstallation = $false; NetworkTests = $false })
    $exitCode = Invoke-LabInstaller $plan.Previous 0
    $files = Assert-LabPayload $plan.Previous
    $cases.Add([ordered]@{ Case = 'clean-install-0.3.4'; Passed = $true; ExitCode = $exitCode; FilesVerified = $files })
    New-Item -ItemType Directory -Path $profileDirectory | Out-Null
    $profileFile = Join-Path $profileDirectory 'netlane-rules.json'
    [IO.File]::WriteAllText($profileFile, '[]', $utf8)
    $unknownFile = Join-Path $programDirectory 'qa-preserved.txt'
    [IO.File]::WriteAllText($unknownFile, 'Disposable QA file; preserve exactly.', $utf8)
    $unknownNotice = Join-Path $programDirectory 'notices\qa-preserved.txt'
    [IO.File]::WriteAllText($unknownNotice, 'Disposable QA notice; preserve exactly.', $utf8)
    $protected = @(foreach ($path in @($profileFile, $unknownFile, $unknownNotice)) { [pscustomobject]@{ Path = $path; Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash } })
    $exitCode = Invoke-LabInstaller $plan.Latest 0
    $files = Assert-LabPayload $plan.Latest
    foreach ($entry in $protected) {
        if ((Get-FileHash -LiteralPath $entry.Path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw 'Fixture alterada no upgrade.' }
    }
    foreach ($relative in $plan.RetiredPayloadFiles) {
        if (Test-Path -LiteralPath (Join-Path $programDirectory $relative)) { throw "Arquivo antigo permaneceu: $relative" }
    }
    $cases.Add([ordered]@{ Case = 'upgrade-0.3.4-to-0.3.5'; Passed = $true; ExitCode = $exitCode; FilesVerified = $files; ProtectedFixtures = $protected.Count })
    $exitCode = Invoke-LabInstaller $plan.Previous 6
    $files = Assert-LabPayload $plan.Latest
    $cases.Add([ordered]@{ Case = 'downgrade-refused'; Passed = $true; ExitCode = $exitCode; FilesVerified = $files })
    # Exact disposable user's command; no Windows logon or startup execution.
    $runKeyPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    $runKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
    try { $runKey.SetValue('NetLane', ('"' + (Join-Path $programDirectory 'NetLane.UI.exe') + '" --startup'), [Microsoft.Win32.RegistryValueKind]::String) }
    finally { $runKey.Close() }
    $uninstaller = Join-Path $programDirectory 'Uninstall.exe'
    $process = Start-Process -FilePath $uninstaller -ArgumentList '/S' -PassThru -WindowStyle Hidden
    if (-not $process.WaitForExit(300000)) { throw 'Remoção não concluiu; nenhum processo foi encerrado à força.' }
    $process.Refresh()
    if ($process.ExitCode -ne 0) { throw "Remoção falhou: $($process.ExitCode)." }
    foreach ($entry in $protected) {
        if ((Get-FileHash -LiteralPath $entry.Path -Algorithm SHA256).Hash -ne $entry.Sha256) { throw 'Fixture alterada na remoção.' }
    }
    $runKey = Get-Item -LiteralPath $runKeyPath
    try { if ($null -ne $runKey.GetValue('NetLane')) { throw 'Autorun esperado permaneceu.' } }
    finally { $runKey.Close() }
    if ((Test-Path -LiteralPath $productKey) -or (Test-Path -LiteralPath $uninstaller)) { throw 'Registro ou desinstalador permaneceu.' }
    $latestManifest = Get-Content -LiteralPath (Join-Path $inputDirectory $plan.Latest.Manifest) -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($entry in (@($latestManifest.Files) + @([pscustomobject]@{ Path = 'package-manifest.json' }))) {
        if (Test-Path -LiteralPath (Join-Path $programDirectory $entry.Path)) { throw "Payload permaneceu na remoção: $($entry.Path)" }
    }
    $cases.Add([ordered]@{ Case = 'uninstall-preserves-fixtures-removes-own-autorun'; Passed = $true; ExitCode = $process.ExitCode; ProtectedFixtures = $protected.Count })
    Write-Receipt 'result.json' ([ordered]@{ Passed = $true; Cases = @($cases.ToArray()); FullInstallerQaVerified = $false; RealLogonVerified = $false; NetworkTestsExecuted = $false; HostInstallation = $false; FinishedUtc = [DateTime]::UtcNow.ToString('o') })
} catch {
    Write-Receipt 'result.json' ([ordered]@{ Passed = $false; Error = $_.Exception.Message; Cases = @($cases.ToArray()); FullInstallerQaVerified = $false; NetworkTestsExecuted = $false; HostInstallation = $false; FinishedUtc = [DateTime]::UtcNow.ToString('o') })
    throw
}
