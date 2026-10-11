# QA Rust na VM de laboratório — instalador por máquina — 2026-10-11

Rodada autorizada do instalador Rust por máquina (`%ProgramFiles%\NetLane Rust`,
achado 1 da auditoria de 2026-10-11) na VM `Win11 - Work`, depois que o
[Sandbox bloqueou o pacote sem assinatura](qa-sandbox-rust-permachine-2026-10-11.md).
**Resultado: todos os casos planejados aprovados.** Nenhuma proteção foi alterada,
nenhum agente/sessão WFP foi iniciado e não houve teste de rede.

## Pacote, alvo e acesso

- Instalador sem assinatura `NetLane-Rust-0.4.0-preview-win-x64-setup.exe`,
  SHA-256 `01228A38D1F6917D2E1821197212B46BB76CF342139399D01A81E28D54A48B69`
  (commit `a5d6a98`), o mesmo bloqueado no Sandbox.
- VM `DESKTOP-DC6M247`, VMware, MAC `00:0C:29:2F:4A:D0`, UUID
  `3F1D4D56-D0E2-DD67-F0F1-12D6082F4AD0`, Windows 11 Pro 26300, fora de domínio;
  Smart App Control desligado (`VerifiedAndReputablePolicyState=0`).
- Snapshot de retorno `PC-OK` (criado 2026-10-11 11:51 UTC, anterior ao SSH),
  conferido no `.vmsd` e confirmado pelo usuário.
- Acesso por OpenSSH com chave dedicada `netlane-lab-qa`, habilitado pelo
  usuário na VM; regra de firewall restrita ao host `192.168.184.1`. Sessão SSH
  com token administrativo. Insumos copiados e conferidos por hash em
  `C:\NetLaneRustQaInput`; recibos em `C:\NetLaneRustQaOutput`.
- Rodada `64ba3354-2b2e-44dd-afda-885d55752aa1`; plano final v5.

## Resultados

Evidências: `artifacts/rust-lab-permachine-20261011-102945` (`output/`, planos v1–v5,
inventário de insumos e auxiliares de diagnóstico).

| Caso | Resultado | Recibo |
| --- | --- | --- |
| Preflight | coletado; nenhum NetLane Rust/.NET, perfil, registro ou processo | `preflight.json` |
| Runtime antigo (14.40) | **recusa com código 8**, nada copiado | `runtimerefusal.json` |
| Runtime VC++ | atualizado para 14.51.36247 (assinatura Microsoft conferida), retorno 0 | `runtime.json`, logs |
| Prévia per-user existente | **recusa com código 4**, nada instalado; fixture removida | `legacyrefusal.json` |
| Sem administrador | token restrito (Administradores *deny-only*, integridade média) + `RunAsInvoker`: **recusa com código 9** pela guarda do próprio instalador; nada instalado | `adminrefusal.json` |
| Instalação limpa | retorno 0; registro HKLM e atalho comum; nome `NetLane Rust (prévia local)` correto; **nenhuma regra permite a Usuários/Autenticados/Todos/Interativo gravar na pasta ou no `netlane-agent.exe`**; nenhum evento de Integridade de Código | `install.json` |
| Smoke | `netlane-agent.exe --validate` instalado leu a fixture (retorno 0) sem criar perfil | `smoke.json` |
| Reinstalação | retorno 0; 710 arquivos + manifesto íntegros; arquivo desconhecido e perfil fictício preservados | `reinstall.json`, `fixtures.json` |
| Remoção | 711 arquivos conhecidos removidos; registro e atalho removidos; fixtures preservadas | `remove.json` |

## Desvios registrados

- **Recusa sem administrador:** a 1ª tentativa (`Start-Process -Credential` de um
  usuário comum temporário) foi barrada pelo próprio manifesto (`requer
  elevação`), sem chegar à guarda interna. A 2ª perdeu o código de saída
  (`Start-Process`) e a 3ª falhou ao criar processo de outro usuário a partir da
  sessão SSH (`0xC0000142`); em todas nada foi instalado e o usuário temporário foi
  removido. O caso final usa uma cópia restrita do token da sessão
  (`scripts/rust-qa-restricted-launcher.cs`), que confirmou não pertencer a
  Administradores. Recibos `adminrefusal-attempt1..3.json` preservados.
- **Instalação, tentativa 1:** instalada corretamente (código 0, ACL aprovada, nome
  correto pelos *codepoints*), mas o recibo saiu `InstalledMetadataInvalid` porque
  o *harness* corrompeu o nome esperado ao regravar o plano sem UTF-8 (v2–v4). O
  plano v5 grava o nome explicitamente; a instalação da tentativa 1 foi removida
  pelo próprio desinstalador (`remove-attempt1-install.json`, aprovado) e a
  instalação limpa foi repetida a partir do mesmo baseline. Um recibo de recusa
  por diário existente (`install-attempt2-blocked-journal.json`) não executou o
  instalador.
- A demonstração da UI não foi repetida (`DemoSkipped`): exige clique em Sair; a UI
  não mudou com o layout por máquina.

## Estado deixado na VM

Instalação NetLane removida. Permanecem, por desenho dos casos: o arquivo
desconhecido `C:\Program Files\NetLane Rust\qa-unknown-preserve.txt`, o perfil
fictício `%LOCALAPPDATA%\NetLane-Rust` do usuário de QA, o runtime VC++ 14.51, as
pastas `C:\NetLaneRustQa*`, o OpenSSH com a chave do laboratório e a regra de
firewall `NetLane-Lab-SSH`. Voltar ao snapshot `PC-OK` desfaz tudo isso, inclusive
o acesso SSH.

Assinatura e distribuição pública continuam gates separados; esta rodada não
aprova SmartScreen, logon/autorun, upgrade entre versões nem falha parcial.
