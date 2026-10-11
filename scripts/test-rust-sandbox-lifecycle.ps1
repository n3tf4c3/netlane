[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Demo','AfterDemo','Reinstall','Remove')][string]$Mode,
    [string]$PlanPath = 'C:\NetLaneRustQaInput\plan-lifecycle.json',
    [ValidatePattern('^(-v[0-9]+)?$')][string]$ReceiptSuffix = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$inputRoot = 'C:\NetLaneRustQaInput'
$outputRoot = 'C:\NetLaneRustQaOutput'
$plan = Get-Content -Raw -LiteralPath $PlanPath -Encoding UTF8 | ConvertFrom-Json
if ($env:COMPUTERNAME -eq $plan.HostComputer) { throw 'Execucao no host recusada.' }
if ([IO.Path]::GetFullPath($PlanPath) -notmatch '^C:\\NetLaneRustQaInput\\plan-lifecycle(?:-v[0-9]+)?\.json$') { throw 'Plano fora do mapeamento previsto.' }
if ($plan.Schema -ne 1 -or $plan.Product -ne 'NetLaneRustPreview' -or $env:USERNAME -ne 'WDAGUtilityAccount' -or -not [Environment]::Is64BitProcess) { throw 'Sandbox/plano invalido.' }
$baseline = Get-Content -Raw -LiteralPath (Join-Path $outputRoot 'preflight.json') -Encoding UTF8 | ConvertFrom-Json
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if ($baseline.RoundId -ne $plan.RoundId -or $baseline.Identity.Computer -ne $env:COMPUTERNAME -or $baseline.Identity.Sid -ne $identity.User.Value) { throw 'Outra sessao.' }
if ((Get-FileHash -LiteralPath $PSCommandPath).Hash -ne $plan.LifecycleSha256) { throw 'Runner diferente do plano.' }
$installed = Get-Content -Raw -LiteralPath (Join-Path $outputRoot 'install.json') -Encoding UTF8 | ConvertFrom-Json
if ($installed.Verdict -ne 'InstalledPendingVerification' -or $installed.ExitCode -ne 0) { throw 'Primeira instalacao nao concluida; nao repetir pacote bloqueado.' }
# Per-machine package: Program Files, all-users Start menu, HKLM registration; profile per user.
$root = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NetLane Rust'
$profile = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'NetLane-Rust'
$shortcut = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'NetLane Rust.lnk'
$registry = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLaneRustPreview'
$manifestPath = Join-Path $inputRoot 'package-manifest.json'
if ((Get-FileHash -LiteralPath $manifestPath).Hash -ne $plan.Package.ManifestSha256) { throw 'Manifesto diferente do plano.' }
$manifest = Get-Content -Raw -LiteralPath $manifestPath -Encoding UTF8 | ConvertFrom-Json
function Write-NewJson([string]$Path,$Value) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 16))
    $stream = [IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $stream.Write($bytes,0,$bytes.Length) } finally { $stream.Dispose() }
}
function Assert-RegularPath([string]$Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if ((Test-Path -LiteralPath $current) -and ((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Link/juncao recusado.' }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}
function Get-PayloadPath([string]$Relative) {
    if ($Relative -notmatch '^[A-Za-z0-9_ .()+/-]+$' -or $Relative -match '(^/|//|(^|/)\.{1,2}(/|$)|[ .](/|$))') { throw 'Caminho de payload invalido.' }
    $full = [IO.Path]::GetFullPath((Join-Path $root $Relative))
    if (-not $full.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Caminho fora do produto.' }
    Assert-RegularPath $full
    return $full
}
function Assert-Payload {
    foreach ($file in $manifest.Files) {
        $path = Get-PayloadPath $file.Path
        $item = Get-Item -LiteralPath $path
        if ($item.PSIsContainer -or $item.Length -ne $file.Bytes -or (Get-FileHash -LiteralPath $path).Hash -ne $file.Sha256) { throw "Payload divergente: $($file.Path)" }
    }
    if ((Get-FileHash -LiteralPath (Get-PayloadPath 'package-manifest.json')).Hash -ne $plan.Package.ManifestSha256 -or
        (Get-Content -Raw -LiteralPath (Get-PayloadPath 'netlane-rust-installed.layout')) -cne '1' -or
        (Get-Content -Raw -LiteralPath (Get-PayloadPath 'rust-package-version.txt')) -cne $plan.Package.Version) { throw 'Identidade de instalacao divergente.' }
    $registration = Get-ItemProperty -LiteralPath $registry
    if ($registration.InstallLocation -ne $root -or $registration.DisplayVersion -ne $plan.Package.Version -or -not (Test-Path -LiteralPath $shortcut)) { throw 'Registro/atalho divergente.' }
    if ($plan.Package.PSObject.Properties['ExpectedDisplayName'] -and $registration.DisplayName -cne $plan.Package.ExpectedDisplayName) { throw 'Nome instalado tem codificacao incorreta.' }
}
function Get-ProductProcesses { return ,@(Get-Process | Where-Object { $_.ProcessName -match '^NetLane([.-]|$)' } | Select-Object Id,ProcessName,Path) }
function Get-Snapshot {
    $run = @()
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
    if ($key) {
        try { foreach ($name in $key.GetValueNames() | Sort-Object) { $run += [ordered]@{Name=$name;Kind=$key.GetValueKind($name).ToString();Value=$key.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)} } } finally { $key.Close() }
    }
    return [ordered]@{Processes=(Get-ProductProcesses);ProfileExists=(Test-Path -LiteralPath $profile);RegistryExists=(Test-Path -LiteralPath $registry);ShortcutExists=(Test-Path -LiteralPath $shortcut);Run=$run;Network=@(Get-NetAdapter | Select-Object Name,Status,MacAddress)}
}
function Get-FixtureHashes {
    return @((Join-Path $root 'qa-unknown-preserve.txt'),(Join-Path $profile 'qa-preserve.txt'),(Join-Path $profile 'netlane-rules.json') | ForEach-Object { Assert-RegularPath $_; [ordered]@{Path=$_;Sha256=(Get-FileHash -LiteralPath $_).Hash} })
}
function Assert-Fixtures($Expected) {
    $actual = Get-FixtureHashes
    if (($actual | ConvertTo-Json -Depth 5 -Compress) -cne ($Expected | ConvertTo-Json -Depth 5 -Compress)) { throw 'Fixtures alteradas/removidas.' }
}
function Start-OneAttempt([string]$File,[string]$Journal) {
    Write-NewJson (Join-Path $outputRoot $Journal) ([ordered]@{RoundId=$plan.RoundId;AtUtc=[DateTime]::UtcNow.ToString('o');Path=$File;Sha256=(Get-FileHash -LiteralPath $File).Hash})
    $process = Start-Process -FilePath $File -ArgumentList '/S' -PassThru
    if (-not $process.WaitForExit(120000)) { throw 'Processo ainda em execucao; nenhuma finalizacao forcada.' }
    if ($process.ExitCode -ne 0) { throw "Instalador/remocao retornou $($process.ExitCode)." }
    return $process.ExitCode
}
$result = [ordered]@{Schema=1;RoundId=$plan.RoundId;Mode=$Mode;RunnerSha256=$plan.LifecycleSha256;StartedAtUtc=[DateTime]::UtcNow.ToString('o');Verdict='Pending';GuestTokenElevated=$baseline.Identity.Elevated;NetworkTests=$false;AgentStarted=$false;ForcedTermination=$false}
try {
    $result.Before = Get-Snapshot
    if ($Mode -ne 'AfterDemo' -and $result.Before.Processes.Count) { throw 'Produto ainda em uso.' }
    if ($Mode -in @('Demo','Reinstall','Remove')) { Assert-Payload }
    switch ($Mode) {
        'Demo' {
            Write-NewJson (Join-Path $outputRoot 'demo-attempt.json') ([ordered]@{RoundId=$plan.RoundId;AtUtc=$result.StartedAtUtc;BinarySha256=(Get-FileHash -LiteralPath (Join-Path $root 'netlane.exe')).Hash;Arguments='--demo --profile C:\NetLaneRustQaDemo'})
            if (Test-Path -LiteralPath 'C:\NetLaneRustQaDemo') { throw 'Perfil demo anterior.' }
            $process = Start-Process -FilePath (Join-Path $root 'netlane.exe') -ArgumentList '--demo --profile C:\NetLaneRustQaDemo' -RedirectStandardError (Join-Path $outputRoot 'demo-stderr.txt') -RedirectStandardOutput (Join-Path $outputRoot 'demo-stdout.txt') -PassThru
            $result.ProcessId = $process.Id
            $deadline = [DateTime]::UtcNow.AddSeconds(30)
            do { Start-Sleep -Milliseconds 300; $process.Refresh() } while (-not $process.HasExited -and $process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
            if ($process.HasExited -or $process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'UI nao abriu; consultar stderr e eventos.' }
            $result.MainWindowHandle = $process.MainWindowHandle.ToInt64()
            $result.Verdict = 'WindowOpenedPendingVisualQa'
        }
        'AfterDemo' {
            $demo = Get-Content -Raw -LiteralPath (Join-Path $outputRoot ('demo'+$ReceiptSuffix+'.json')) -Encoding UTF8 | ConvertFrom-Json
            if ($demo.RoundId -ne $plan.RoundId -or $demo.Verdict -ne 'WindowOpenedPendingVisualQa' -or (Get-ProductProcesses).Count) { throw 'Encerramento normal nao confirmado.' }
            if (Test-Path -LiteralPath $profile) { throw 'Demo criou/acessou perfil padrao.' }
            Assert-Payload
            $result.Verdict = 'Passed'
            $result.ProductProcessesAbsent = $true
            $result.DefaultProfileCreated = $false
        }
        'Reinstall' {
            $afterDemo = Get-Content -Raw -LiteralPath (Join-Path $outputRoot ('afterdemo'+$ReceiptSuffix+'.json')) | ConvertFrom-Json
            if ($afterDemo.Verdict -ne 'Passed' -or (Test-Path -LiteralPath $profile)) { throw 'Demo nao encerrou ou perfil inesperado.' }
            Assert-RegularPath $profile
            foreach ($path in @((Join-Path $root 'qa-unknown-preserve.txt'),(Join-Path $profile 'qa-preserve.txt'),(Join-Path $profile 'netlane-rules.json'))) { if (Test-Path -LiteralPath $path) { throw 'Fixture anterior.' } }
            $null = New-Item -ItemType Directory -Path $profile
            [IO.File]::WriteAllText((Join-Path $root 'qa-unknown-preserve.txt'),'Sandbox QA unknown file',[Text.UTF8Encoding]::new($false))
            [IO.File]::WriteAllText((Join-Path $profile 'qa-preserve.txt'),'Sandbox QA fictitious data',[Text.UTF8Encoding]::new($false))
            [IO.File]::WriteAllText((Join-Path $profile 'netlane-rules.json'),'[]',[Text.UTF8Encoding]::new($false))
            $fixtures = Get-FixtureHashes
            Write-NewJson (Join-Path $outputRoot 'fixtures.json') $fixtures
            $setup = Join-Path $inputRoot $plan.Package.Installer
            if ((Get-FileHash -LiteralPath $setup).Hash -ne $plan.Package.Sha256) { throw 'Setup diferente do plano.' }
            $result.ExitCode = Start-OneAttempt $setup 'reinstall-attempt.json'
            Assert-Payload
            Assert-Fixtures $fixtures
            $result.PayloadFilesVerified = $manifest.Files.Count+1
            $result.FixturesPreserved = $true
            $result.UpgradeBetweenVersionsVerified = $false
            $result.Verdict = 'Passed'
        }
        'Remove' {
            $reinstall = Get-Content -Raw -LiteralPath (Join-Path $outputRoot ('reinstall'+$ReceiptSuffix+'.json')) | ConvertFrom-Json
            if ($reinstall.RoundId -ne $plan.RoundId -or $reinstall.Verdict -ne 'Passed') { throw 'Reinstalacao nao concluida.' }
            $fixtures = Get-Content -Raw -LiteralPath (Join-Path $outputRoot 'fixtures.json') | ConvertFrom-Json
            Assert-Fixtures $fixtures
            $uninstall = Join-Path $root 'Uninstall.exe'
            if ((Get-FileHash -LiteralPath $uninstall).Hash -ne @($installed.After.Program.Entries | Where-Object Path -eq 'Uninstall.exe')[0].Sha256) { throw 'Desinstalador diferente do original.' }
            $result.LauncherExitCode = Start-OneAttempt $uninstall 'remove-attempt.json'
            $known = @($manifest.Files.Path)+@('package-manifest.json','Uninstall.exe')
            $deadline = [DateTime]::UtcNow.AddSeconds(45)
            do {
                $remaining = @($known | Where-Object { Test-Path -LiteralPath (Get-PayloadPath $_) })
                if ($remaining.Count) { Start-Sleep -Milliseconds 300 }
            } while ($remaining.Count -and [DateTime]::UtcNow -lt $deadline)
            if ($remaining.Count -or (Test-Path -LiteralPath $registry) -or (Test-Path -LiteralPath $shortcut)) { throw 'Remocao incompleta; preservar evidencia.' }
            Assert-Fixtures $fixtures
            $remainingFiles = @(Get-ChildItem -LiteralPath $root -File -Recurse)
            if ($remainingFiles.Count -ne 1 -or $remainingFiles[0].FullName -ne (Join-Path $root 'qa-unknown-preserve.txt')) { throw 'Conteudo residual inesperado.' }
            $result.KnownFilesRemoved = $known.Count
            $result.FixturesPreserved = $true
            $result.RegistryRemoved = $true
            $result.ShortcutRemoved = $true
            $result.Verdict = 'Passed'
        }
    }
    $result.After = Get-Snapshot
    if (($result.Before.Run | ConvertTo-Json -Depth 5 -Compress) -cne ($result.After.Run | ConvertTo-Json -Depth 5 -Compress)) { throw 'Run mudou inesperadamente.' }
    if ($result.After.Network.Count -or @($result.After.Processes | Where-Object ProcessName -eq 'netlane-agent').Count) { throw 'Agente/rede inesperados.' }
} catch {
    $result.Verdict = 'Failed'
    $result.Error = $_.Exception.ToString()
    $result.After = Get-Snapshot
    $result.CodeIntegrityEvents = @(Get-WinEvent -FilterHashtable @{LogName='Microsoft-Windows-CodeIntegrity/Operational';StartTime=([DateTime]::Parse($result.StartedAtUtc).ToLocalTime()).AddSeconds(-2)} -ErrorAction SilentlyContinue | Select-Object TimeCreated,Id,Message)
} finally {
    $result.CompletedAtUtc = [DateTime]::UtcNow.ToString('o')
    Write-NewJson (Join-Path $outputRoot ($Mode.ToLowerInvariant()+$ReceiptSuffix+'.json')) $result
}
if ($result.Verdict -notin @('Passed','WindowOpenedPendingVisualQa')) { exit 1 }
