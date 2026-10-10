param([string]$OutputDirectory = 'artifacts/rust-compatibility')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$output = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
if (-not $output.StartsWith((Join-Path $root 'artifacts') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use uma pasta sob artifacts para as fixtures.' }
New-Item -ItemType Directory -Force -Path $output | Out-Null
$probe = Join-Path $root 'tests/NetLane.RustCompatibilityProbe/bin/Release/net8.0/NetLane.RustCompatibilityProbe.exe'
$agent = Join-Path $root 'artifacts/rust-target/release/netlane-agent.exe'
& dotnet build (Join-Path $root 'tests/NetLane.RustCompatibilityProbe/NetLane.RustCompatibilityProbe.csproj') --configuration Release --nologo
if ($LASTEXITCODE -ne 0) { throw 'Build da referência falhou.' }
if (-not (Test-Path -LiteralPath $agent)) { throw 'Compile o workspace Rust em Release antes da comparação.' }
$fixtures = @(
    @{Name='empty'; Json='[]'; Valid=$true; Bom=$false},
    @{Name='numeric-pascal'; Json='[{"ApplicationId":"app.exe","ExecutablePath":"C:\\Apps\\app.exe","RouteMode":2,"InterfaceId":"{AA6F3B88-072A-45B6-B0CB-8AFE9567E62F}","FallbackInterfaceId":"D3AE43D2-8203-41D6-9D7F-0B6FA59518FD","futureSetting":{"value":42}}]'; Valid=$true; Bom=$false},
    @{Name='string-bom'; Json='[{"applicationId":"other.exe","routeMode":"Blocked","enabled":false,"includeRelatedExecutables":false}]'; Valid=$true; Bom=$true},
    @{Name='mixed-case'; Json='[{"APPLICATIONID":"mixed.exe","ROUTEMODE":"wifi","ENABLED":true}]'; Valid=$true; Bom=$false},
    @{Name='null'; Json='null'; Valid=$false; Bom=$false},
    @{Name='invalid-mode'; Json='[{"applicationId":"app.exe","routeMode":99}]'; Valid=$false; Bom=$false},
    @{Name='invalid-guid'; Json='[{"applicationId":"app.exe","interfaceId":"bad"}]'; Valid=$false; Bom=$false},
    @{Name='duplicate'; Json='[{"applicationId":"X"},{"applicationId":"x"}]'; Valid=$false; Bom=$false}
)
$receipts = @()
function Invoke-Validation([string]$Executable, [string]$Path) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.Arguments = '--validate "' + $Path + '"'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw "Falha ao iniciar $Executable" }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        return @{Code=$process.ExitCode;Output=$stdout.Result;Error=$stderr.Result}
    } finally { $process.Dispose() }
}
foreach ($fixture in $fixtures) {
    $path = Join-Path $output ($fixture.Name + '.json')
    [IO.File]::WriteAllText($path, $fixture.Json, [Text.UTF8Encoding]::new($fixture.Bom))
    $before = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    $dotnet = Invoke-Validation $probe $path
    $rust = Invoke-Validation $agent $path
    $dotnetCode = $dotnet.Code
    $rustCode = $rust.Code
    if (($dotnetCode -eq 0) -ne $fixture.Valid -or ($rustCode -eq 0) -ne $fixture.Valid) { throw "Veredito divergente: $($fixture.Name), .NET=$dotnetCode, Rust=$rustCode" }
    if ($fixture.Valid) {
        $left = Join-Path $output ($fixture.Name + '.dotnet.json')
        $right = Join-Path $output ($fixture.Name + '.rust.json')
        [IO.File]::WriteAllText($left, $dotnet.Output, [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($right, $rust.Output, [Text.UTF8Encoding]::new($false))
        & $probe --compare $left $right
        if ($LASTEXITCODE -ne 0) { throw "Conteúdo divergente: $($fixture.Name)" }
    }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $before) { throw 'A validação alterou a fixture.' }
    $receipts += @{Name=$fixture.Name;Valid=$fixture.Valid;DotnetExit=$dotnetCode;RustExit=$rustCode;Preserved=$true}
}
$receipts | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'verification.json') -Encoding UTF8
Write-Output "Compatibilidade: $($receipts.Count)/$($fixtures.Count) fixtures aprovadas; arquivos preservados."
