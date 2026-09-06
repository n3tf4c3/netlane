# Fase 1 — Monitor (NetLane)

Objetivo desta fase:
- Criar o esqueleto do runtime de monitoramento (Windows Service) com coleta periódica de:
  - adaptadores conectados;
  - aplicações candidatas com conexões;
  - conexões de rede ativas (TCP/UDP) e associação por PID.
- Sem qualquer alteração de rota de tráfego nesta etapa.

Entregas implementadas:
- Worker do serviço (`NetworkMonitorWorker`) com loop de polling.
- Registro de dependências de rede em DI no `NetLane.Service`:
  - `WindowsNetworkInterfaceDetector`;
  - `ProcessApplicationCatalog`;
  - `WindowsNetworkFlowMonitor`.
- Configuração do host de serviço com:
  - `NetLane:Monitor:PollIntervalSeconds`;
  - `TopApplicationsToLog`;
  - `IncludeInactiveInterfacesInSnapshot`.
- `appsettings.json` com valores padrão de monitoramento.

Critérios de sucesso desta etapa:
- `dotnet build NetLane.sln` sem erros.
- Serviço consegue iniciar sem falhar por infraestrutura.
- Logs de snapshot emitindo contadores e ranking de aplicações com conexões.

Próximo ciclo (Fase 2 — Routing Engine):
- Persistência de regra por aplicação;
- Aplicação das regras ao detectar abertura de processos;
- Fail-open + rollback seguro.
