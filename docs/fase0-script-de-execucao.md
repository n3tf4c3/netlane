# Fase 0 - ExecuÃ§Ã£o prÃ¡tica (quando dotnet estiver disponÃ­vel)

## Objetivo

Rodar a PoC mÃ­nima com dois apps em interfaces diferentes e validar IP pÃºblico por app.

## Ordem de execuÃ§Ã£o

1. Preparar ambiente
   - Abra PowerShell como Administrador.
   - Execute: `pwsh ./scripts/setup-netlane.ps1 -Full`

2. Criar soluÃ§Ã£o e projetos

   ```powershell
   dotnet new sln -n NetLane
   dotnet new classlib -n NetLane.Core -f net8.0 -o src/NetLane.Core
   dotnet new classlib -n NetLane.Network -f net8.0 -o src/NetLane.Network
   dotnet new classlib -n NetLane.Service -f net8.0-windows -o src/NetLane.Service
   dotnet new wpf -n NetLane.UI -f net8.0-windows -o src/NetLane.UI
   dotnet new console -n NetLane.NetworkPoC -f net8.0 -o poc/NetLane.NetworkPoC

   dotnet sln NetLane.sln add src/NetLane.Core/NetLane.Core.csproj
   dotnet sln NetLane.sln add src/NetLane.Network/NetLane.Network.csproj
   dotnet sln NetLane.sln add src/NetLane.Service/NetLane.Service.csproj
   dotnet sln NetLane.sln add src/NetLane.UI/NetLane.UI.csproj
   dotnet sln NetLane.sln add poc/NetLane.NetworkPoC/NetLane.NetworkPoC.csproj
   ```

3. Implementar PoC real em `poc/NetLane.NetworkPoC`
   - Coletar interfaces conectadas (GUID + nome amigÃ¡vel)
   - Identificar processo por conexÃ£o
   - Aplicar regra por executable (`chrome.exe`, `curl.exe`)
   - ForÃ§ar saÃ­da pela interface desejada

4. Validar
   - `curl.exe` reporta IP pÃºblico de rede A
   - `chrome.exe` reporta IP pÃºblico de rede B
   - Testes com HTTP/HTTPS e UDP conforme planejamento

5. Documentar
   - Resultado no `docs/plano-de-execucao-fase0.md` (seÃ§Ã£o de evidÃªncia)

## CritÃ©rio de avanÃ§o

Quando a PoC estiver repetÃ­vel e sem impacto global no roteamento:
- seguir para Fase 1 (Monitor)
- manter fail-open como regra de seguranÃ§a
