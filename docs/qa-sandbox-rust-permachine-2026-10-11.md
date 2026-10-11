# QA Rust no Windows Sandbox — instalador por máquina — 2026-10-11

Rodada do instalador Rust convertido para instalação por máquina
(`%ProgramFiles%\NetLane Rust`, achado 1 da auditoria de 2026-10-11).
**Resultado: bloqueada pelo Smart App Control antes da primeira execução do
instalador. Nenhum caso do pacote por máquina foi concluído.** O Sandbox foi
encerrado e descartado; nenhuma proteção foi alterada e o bloqueio não foi
contornado nem repetido.

## Pacote e ambiente

- Instalador `NetLane-Rust-0.4.0-preview-win-x64-setup.exe`, sem assinatura,
  SHA-256 `01228A38D1F6917D2E1821197212B46BB76CF342139399D01A81E28D54A48B69`,
  commit `a5d6a98`; manifesto `926EB648…C22C`. Verificação estática aprovada
  (710 arquivos, instalador/desinstalador `requireAdministrator`, UI `asInvoker`).
- Rodada `b3ef067e-1298-4169-bdc0-ae0693299c45`, Sandbox
  `ad066c40-deb4-4ff9-9c80-1e42b861e910`; convidado Windows 11 Enterprise
  build 26100, `WDAGUtilityAccount` com token administrativo.
- Rede, entrada de áudio/vídeo, clipboard e impressoras desativados; entrada
  só leitura com pacote, manifesto, roteiros, planos, fixture de regras e o
  redistribuível VC++ x64 já conferido (SHA-256 `84306899…295C`, assinatura
  Microsoft válida); saída gravável só para recibos.
- `wsb start` sem conexão não cria sessão de usuário: o `LogonCommand` só rodou
  depois de `wsb connect`. As etapas seguintes usaram `wsb exec -r ExistingLogin`.

## Resultados

Evidências: `artifacts/rust-sandbox-permachine-20261011-063527`.

| Etapa | Resultado | Recibo |
| --- | --- | --- |
| Preflight | coletado; programa, perfil, registro e processos ausentes; nenhuma rede | `output/preflight.json` |
| Runtime VC++ (só no Sandbox) | instalado, retorno 0, versão 14.51.36247.0 | `output/runtime.json`, logs |
| Preflight após runtime | coletado | `output/preflight-after-runtime.json` |
| Recusa da prévia per-user (código 4) | **bloqueada**: `Start-Process` do instalador recusado pelo Controle de Aplicativo | `output/legacyrefusal.json` |
| Diagnóstico | eventos de Integridade de Código 3077, 3033 e 3118 (“Smart App Control Block”) às 10:56:04 UTC; `VerifiedAndReputablePolicyState=1`; programa, registro, processos e fixture legada ausentes | `output/block-diagnostics.json` |

Recusa sem administrador (código 9), instalação, ACL da pasta e do agente,
smoke do agente instalado, reinstalação e remoção **não foram executados**.

Nas rodadas de 2026-10-10 o mesmo Sandbox registrava
`VerifiedAndReputablePolicyState=2` e o instalador per-user sem assinatura
rodou. Nesta rodada o valor era `1` e o binário novo, sem reputação, foi
bloqueado — o mesmo mecanismo que bloqueou o instalador .NET em 2026-10-08.

O recibo `legacyrefusal.json` registra `InstallerExecuted=true` por um defeito
do roteiro, que marcava o campo antes de o processo iniciar; o processo nunca
iniciou. O roteiro foi corrigido para marcar somente depois de `Start-Process`.

## Próximo passo

O QA runtime do pacote por máquina depende de um destes caminhos, sem alterar
proteções: instalador e binários **assinados** (procedimento SimplySign), ou
a VM de laboratório autorizada sem bloqueio a executáveis de desenvolvimento.
Os roteiros já cobrem: recusa da prévia per-user (`LegacyRefusal`), recusa sem
administrador com usuário comum temporário e `RunAsInvoker` (`AdminRefusal`),
instalação com conferência de ACL da pasta e do agente, smoke
`netlane-agent --validate` (`Smoke`), reinstalação e remoção. A demonstração
da UI fica para conferência manual (`DemoSkipped` no plano).
