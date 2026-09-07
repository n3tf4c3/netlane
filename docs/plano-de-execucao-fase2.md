# Fase 2 — Routing Engine

Atualização em 2026-09-06: o motor foi substituído por [políticas nativas de conexão por AppId/LUID](roteamento-nativo.md), com resultado explícito e heartbeat para a UI. O serviço não mais apresenta dry-run como aplicação real. A prova de tráfego elevado continua pendente. Os registros abaixo de filtros PERMIT/BLOCK documentam a implementação anterior, não o funcionamento do novo motor.

Objetivo:
- Implementar aplicação de política de rota por aplicativo dentro do `NetLane.Service`, com persistência simples em arquivo JSON e aplicação por varredura.
- Garantir fail-open quando o WFP não estiver disponível, com resultado não aplicado (`UnavailableRoutingEngine`).
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
- Fase 2 concluída com `NetworkRoutingWorker` aplicando política por arquivo JSON e rollback em `StopAsync`.
- Critérios validados:
  - Build da solução sem erros.
  - Aplicação de política em ciclo de monitoramento com `RouteMode` por aplicação.
  - Persistência de política em `src/NetLane.Service/netlane-rules.json` com criação por bootstrap quando necessário.
  - `Dry-run` com fallback automático quando WFP exige privilégios de admin.
  - Remoção de estado ativo no encerramento do worker.

Próximo ciclo:
- Fase 3 — Interface final: criar a interface de edição/visualização de `App → Interface`, com início de persistência e operação do serviço via UI.
