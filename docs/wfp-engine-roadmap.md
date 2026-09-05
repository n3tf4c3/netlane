# Roteiro de implementacao WFP para NetLane

Objetivo da etapa:
- Interceptar novas conexoes por processo (executavel) e associar interface de saida.

Camadas recomendadas para a primeira iteracao:
- `FWPM_LAYER_ALE_AUTH_CONNECT_V4` para fluxo outbound TCP/UDP IPv4.
- Aplicar condicao:
  - `FWPM_CONDITION_ALE_APP_ID` (executavel)
  - opcional: `FWPM_CONDITION_IP_PROTOCOL`
- Regra:
  - `BLOCK` para fallback Bloqueado.
  - `PERMIT` com metadata de interface alvo para aplicacao no callout local.

Observacoes:
- WFP e uma engine de filtro de pacotes, nao um roteador pronto.
- Para forcar interface por app geralmente e necessario callout em sublayer para reescrever/selecionar `FWP_DIRECTION_OUTBOUND` context e interface index.
- Evitar regressao global:
  - usar `provider` e `sublayer` dedicados.
- Fail open em erro:
  - remover sublayer e filtros no stop/falha do servico.
- Primeira entrega:
  - tratar apenas IPv4 e estados conectados.

Status desta iteracao:
- `WfpRoutingEngine` abre sessao WFP via `FwpmEngineOpen0` e valida permissoes (Windows + admin), com estado interno de regras.
- Falhas de preflight retornam para `DryRunRoutingEngine` para manter PoC executavel.
- Existe `FirewallRoutingEngine` (`--firewall`) como alternativa experimental para validar direcionamento por app/interface via regra de firewall.

Checklist de implementacao no NetLane:
- Criar sessao WFP (`FwpmEngineOpen0`).
- Criar provider + sublayer.
- Registrar callout para bind de encaminhamento.
- Criar filtro por `appId` + protocolo -> action redirecionada por contexto interno.
- Persistir regra para reversao rapida.
- Testar com chrome.exe e curl.exe (HTTP/HTTPS + UDP baseline).

Criticos de risco:
- `GetExtendedTcpTable`/`GetExtendedUdpTable` sao apenas monitoramento; o enforcement real requer WFP.
- Necessita privilegio de admin/servico persistente.
