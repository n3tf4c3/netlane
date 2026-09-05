param(
    [switch]$Full
)

$ErrorActionPreference = "Stop"

Write-Host "NetLane - Preparação de ambiente"

function Write-Section {
    param([string]$Text)
    Write-Host ""
    Write-Host "=== $Text ===" -ForegroundColor Cyan
}

Write-Section "Verificando pré-requisitos"

function Test-Cmd {
    param([string]$Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

$HasDotnet = Test-Cmd "dotnet"
if ($HasDotnet) {
    $dotnetVersion = (& dotnet --version)
    Write-Host "dotnet encontrado: $dotnetVersion" -ForegroundColor Green
} else {
    Write-Host "dotnet não encontrado. Instale o .NET SDK (recomendado 8.0) antes de continuar." -ForegroundColor Red
}

$IsAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole] "Administrator"
)
if ($IsAdmin) {
    Write-Host "Executando com privilégios de administrador." -ForegroundColor Green
} else {
    Write-Host "Execute este terminal como Administrador para instalar/rodar serviço e WFP." -ForegroundColor Yellow
}

Write-Section "Comandos de bootstrap"
Write-Host "dotnet new sln -n NetLane"
Write-Host "dotnet new classlib -n NetLane.Core -f net8.0 -o src/NetLane.Core"
Write-Host "dotnet new classlib -n NetLane.Network -f net8.0 -o src/NetLane.Network"
Write-Host "dotnet new classlib -n NetLane.Service -f net8.0 -o src/NetLane.Service"
Write-Host "dotnet new wpf -n NetLane.UI -f net8.0-windows -o src/NetLane.UI"
Write-Host "dotnet new console -n NetLane.NetworkPoC -f net8.0 -o poc/NetLane.NetworkPoC"

Write-Section "Incluir projetos na solução"
Write-Host "dotnet sln NetLane.sln add src/NetLane.Core/NetLane.Core.csproj"
Write-Host "dotnet sln NetLane.sln add src/NetLane.Network/NetLane.Network.csproj"
Write-Host "dotnet sln NetLane.sln add src/NetLane.Service/NetLane.Service.csproj"
Write-Host "dotnet sln NetLane.sln add src/NetLane.UI/NetLane.UI.csproj"
Write-Host "dotnet sln NetLane.sln add poc/NetLane.NetworkPoC/NetLane.NetworkPoC.csproj"

if ($Full) {
    Write-Section "Dependências sugeridas"
    Write-Host "dotnet add src/NetLane.Network/NetLane.Network.csproj package Microsoft.Extensions.Logging.Abstractions"
    Write-Host "dotnet add src/NetLane.Network/NetLane.Network.csproj package Microsoft.Win32.Registry"
    Write-Host "dotnet add src/NetLane.Service/NetLane.Service.csproj package Microsoft.Extensions.Hosting"
    Write-Host "dotnet add src/NetLane.Service/NetLane.Service.csproj package Microsoft.Extensions.Hosting.WindowsServices"
}

Write-Section "Observações"
Write-Host "Fase 0 recomendada: validar WFP em PoC antes de avançar para UI."
Write-Host "Regra de segurança: comportamento padrão deve ser fail-open."
