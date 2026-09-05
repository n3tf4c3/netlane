# NetLane

Produto Windows para direcionamento de trafego por aplicacao (app -> interface de rede).

## Estado atual do projeto

Planejamento completo em `docs/Planejamento do Projeto NetLane.md`.

Objetivo desta execucao: iniciar pela Fase 0 (PoC tecnica) antes da UI.

## Estrutura inicial adotada

- `src/NetLane.Core/` - contratos e regras de dominio.
- `src/NetLane.Network/` - motor de rede (deteccao/execucao de politica).
- `src/NetLane.Service/` - servico Windows em background.
- `src/NetLane.UI/` - interface WPF (fase posterior).
- `poc/NetLane.NetworkPoC/` - prova de conceito de roteamento por aplicacao.
- `docs/` - documentacao e planejamento.

## Como comecar (hoje)

1. Validar ambiente de desenvolvimento Windows com .NET SDK instalado.
2. PoC minima:
   - `chrome.exe` -> Wi-Fi
   - `curl.exe` -> Ethernet
   - Executa em `dry-run` por padrão.
3. Coletar evidencia de IP publico distinto por app (incluindo `--check-public-ip`).

Estado atual:
- a etapa de regra esta em modo `dry-run` por padrao.
- o modo `--wfp` tenta inicializar preflight WFP e cai para `dry-run` se indisponivel.
- o modo `--firewall` habilita prova experimental por interface usando firewall do Windows.
- a base para engine WFP esta preparada para evolucao.
- correlacao de apps com conexoes ativas (PID/TCP/UDP IPv4) foi adicionada.

## Fase imediata

- implementacao real do `WfpRoutingEngine` e validacao end-to-end.
- manter `fail-open` e rollback seguro.

Documentacao da proxima etapa:
- `docs/wfp-engine-roadmap.md`

## Comandos uteis (quando ambiente tiver dotnet)

- `dotnet --version` (validar ferramenta)
- `dotnet new sln -n NetLane`
- `dotnet new classlib -n NetLane.Core`
- `dotnet new classlib -n NetLane.Network`
- `dotnet new classlib -n NetLane.Service`
- `dotnet new wpf -n NetLane.UI`
- `dotnet new console -n NetLane.NetworkPoC -o poc/NetLane.NetworkPoC`
