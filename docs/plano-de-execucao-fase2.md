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
- Fase 2 concluída com `NetworkRoutingWorker` aplicando política por arquivo JSON e rollback em `StopAsync`.
- Critérios validados:
  - Build da solução sem erros.
  - Aplicação de política em ciclo de monitoramento com `RouteMode` por aplicação.
  - Persistência de política em `src/NetLane.Service/netlane-rules.json` com criação por bootstrap quando necessário.
  - `Dry-run` com fallback automático quando WFP exige privilégios de admin.
  - Remoção de estado ativo no encerramento do worker.

Próximo ciclo:
- Fase 3 — Interface final: criar a interface de edição/visualização de `App → Interface`, com início de persistência e operação do serviço via UI.
