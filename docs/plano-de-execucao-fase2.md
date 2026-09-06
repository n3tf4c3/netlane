# Fase 2 — Routing Engine

Objetivo:
- Implementar aplicação de política de rota por aplicativo dentro do `NetLane.Service`, com persistência simples em arquivo JSON e aplicação por varredura.
- Garantir fail-open quando o kernel WFP não estiver disponível (via fallback para `DryRunRoutingEngine`).
- Validar rollback de segurança: remoção de regras ativas ao encerrar o serviço.

Entregas esperadas:
- `NetLaneRoutingOptions` com:
  - `Polling` de política,
  - `PolicyFilePath`,
  - lista inicial de políticas (bootstrap/configuração).
- Worker dedicado (`NetworkRoutingWorker`) para:
  - carregar políticas da configuração e arquivo;
  - resolver interface alvo por `InterfaceId`/modo (`Ethernet`/`WiFi`);
  - aplicar regras por aplicação ativa com engine real ou dry-run;
  - remover regras obsoletas.
- Persistência mínima:
  - quando configurado e o arquivo não existir, o serviço cria o arquivo inicial a partir de `appsettings`.

Critérios de validação:
- build da solução sem erros;
- serviço inicia em admin, logs de aplicação e remoção de regra;
- mudança de `RouteMode` no arquivo de política reflete em aplicação/remoção de regra no ciclo;
- parada do serviço limpa estado kernel/drivers (`RemoveAllRules` no worker).

Pontos de risco:
- aplicação ainda depende da detecção de aplicação ativa no catálogo (`GetNetworkActiveApplications`);
- fallback inicial usa nomes de processo conhecidos (catálogo simplificado).

Concluimos:
- A Fase 2 foi iniciada com worker de enforçamento de política e persistência bootstrap.
