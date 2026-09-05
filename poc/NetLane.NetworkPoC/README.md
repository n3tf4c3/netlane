# NetLane Network PoC

Diretorio da prova de conceito (Fase 0).

## O que esta implementado agora

- Descoberta de interfaces conectadas (GUID, tipo e IPv4).
- Descoberta de processos candidatos por executavel.
- Montagem de regras de teste em modo dry-run:
  - `chrome.exe` -> Wi-Fi
  - `curl.exe` -> Ethernet
- Resumo textual das regras preparadas.

## Como executar (quando o SDK .NET estiver disponivel)

```powershell
cd C:\Codes\netlane
dotnet run --project poc\NetLane.NetworkPoC\NetLane.NetworkPoC.csproj
```

```powershell
dotnet run --project poc\NetLane.NetworkPoC\NetLane.NetworkPoC.csproj -- --wfp
```

`--wfp` usa o modo de preflight WFP; em falha, cai para dry-run.

```powershell
dotnet run --project poc\NetLane.NetworkPoC\NetLane.NetworkPoC.csproj -- --firewall
```

`--firewall` usa modo de enforcement experimental via regra de firewall por `Program` + `InterfaceAlias` para validar o conceito de isolamento por app/interface.

```powershell
dotnet run --project poc\NetLane.NetworkPoC\NetLane.NetworkPoC.csproj -- --check-public-ip
```

`--check-public-ip` adiciona uma consulta extra com `curl` para coletar IP público atual (quando `curl.exe` estiver disponível).

A PoC inicial mostra somente a fase de descoberta e planejamento de regra.

## Resultado esperado

- Listagem de interfaces com GUID.
- Listagem de processos candidatos.
- Lista de regras preparadas conforme motor selecionado (`dry-run`, `wfp`, `firewall`).
- Consulta de IP público com `curl` (quando solicitada com `--check-public-ip`).

## Proximo passo (tecnico real)

Substituir `WfpRoutingEngine` por implementacao real de WFP (kernel user-mode driver callback) no projeto `NetLane.Network`.

Objetivos do passo final:
- aplicar regra por executavel
- validar IP publico por app
- validar fail-open no caso de falha do motor
