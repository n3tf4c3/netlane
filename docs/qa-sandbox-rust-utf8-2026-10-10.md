# Correção de UTF-8 no instalador Rust — 2026-10-10

Esta rodada verifica a correção da acentuação encontrada no nome instalado na
[primeira rodada do Sandbox](qa-sandbox-rust-2026-10-10.md). A compilação e a
verificação estática passaram. Instalação limpa, UI instalada em demonstração,
reinstalação da mesma versão e remoção passaram com o novo pacote; a correção
do nome no registro foi confirmada. O Sandbox foi encerrado e host/.NET
preservados. Esta é a aprovação dos quatro fluxos básicos neste ambiente,
com token administrativo, sem aprovação completa do instalador ou entrega pública.

## Correção e pacote

Os fontes e includes NSIS usam UTF-8. O compilador estava interpretando arquivos
sem BOM pela página de código padrão do Windows, produzindo `prÃ©via` no registro.
O builder agora informa `/INPUTCHARSET UTF8` e grava `CompilerInputCharset=UTF8`
no recibo, conforme a [documentação NSIS](https://nsis.sourceforge.io/Docs/Chapter3.html).
O verificador aceita recibos históricos sem esse campo e recusa outro charset
quando ele existe. Os executores runtime conferem o nome exato do registro.

- Pacote: `artifacts/rust-installer-utf8-20261010-095640`.
- Instalador: `NetLane-Rust-0.4.0-preview-win-x64-setup.exe`, 6.698.476 bytes.
- SHA-256: `325DEFDF57FFC276D4762D0C650603DFF2547D10BAE30CD0EC29E8900DA6DC8B`.
- Manifesto: `931E4BBF8ED4DF73AC6B6316BD67C5EFD39003DD06802BD01C0BBC52B5FFBC3A`.
- Executáveis Rust iguais por hash aos da primeira rodada; build Release comum,
  sem `gui-test`. Foi usado `-SkipChecks`: os testes Rust não foram repetidos
  nesta alteração de empacotamento; o build Release foi executado.
- Extração estática: `archive-check-2be068eb/verification.json`, `Passed=true`,
  710 arquivos; 390 crates, 694 avisos/textos e nenhuma lacuna de coleta.
- Oito recusas estáticas aprovadas:
  `artifacts/rust-package-negative-20261010-095914-54529942/result.json`.

## Sessão e evidências

Base: `artifacts/rust-sandbox-utf8-20261010-095640`.
Sessão: `927d6faf-6c06-41f7-963d-5915c5a30adb`.
Rede, clipboard, impressoras e áudio/vídeo de entrada desativados; vGPU habilitada
e memória em 8.192 MB. Insumos Rust compartilhados somente para leitura e uma
pasta de saída para recibos. Os sete modos dos executores recusaram o host.

O preflight confirmou ambiente limpo, sem produto/perfil Rust ou processos,
ICU presente e runtime Visual C++ ausente. O redistribuível oficial Microsoft
14.51.36247.0 foi reutilizado após conferir hash e assinatura válida, sem executá-lo
no host. Seu SHA-256 é
`843068991DAAA1F73AD9F6239BCE4D0F6A07A51F18C37EA2A867E9BECA71295C`.

O token `WDAGUtilityAccount` é administrativo. A validação nesta sessão não
comprova instalação com usuário comum, upgrade entre versões, falha parcial,
cancelamento, bandeja/DPI completos ou convivência instalada com .NET.
Agente, WFP e testes reais de rede permanecem fora desta rodada.
Assinatura e revisão final das licenças continuam pendentes.
Os recibos, instaladores e o achado da primeira rodada são preservados.

## Resultados da rodada

O runtime concluiu com retorno 0, sem reboot, entre 10:04:48 e 10:08:53
(America/Cuiabá). O novo preflight confirmou Visual C++ x64 14.51.36247.0 e
produto ainda ausente antes da instalação Rust.

`output/install.json` confirma retorno 0 e
`DisplayName=NetLane Rust (prévia local)`. O caractere `é` é U+00E9 (233),
sem a sequência U+00C3/U+00A9 da rodada anterior. A resolução do achado está em
`finding-registration-name-resolved.json`; o achado/recibo original não foi
alterado. Mensagens do assistente não foram conferidas visualmente nesta rodada.

`install-verification.json` confere os 710 arquivos por tamanho/SHA-256,
manifesto, nome, atalho, ausência do perfil padrão/processos e HKCU Run
preservado. Nenhum painel/agente foi iniciado automaticamente.

| Passo | Resultado conferido | Recibo |
| --- | --- | --- |
| Instalação limpa | retorno 0; 710 hashes; nome Unicode correto, registro/atalho próprios; nenhum perfil, Run ou processo automático | `install-verification.json`, `output/install.json` |
| UI instalada | Release comum em `--demo --profile C:\NetLaneRustQaDemo`; cinco páginas conferidas; saída normal por Sair; payload íntegro e perfil padrão ausente | `ui-verification.json`, `output/afterdemo.json`, cinco `output/ui-*.png` |
| Reinstalação 0.4.0 sobre 0.4.0 | retorno 0; 710 arquivos íntegros; três fixtures preservadas | `output/reinstall.json`, `output/fixtures.json` |
| Remoção | launcher retornou 0; remoção efetiva de 711 arquivos conhecidos, incluindo o desinstalador; registro/atalho removidos; três fixtures preservadas | `removal-verification.json`, `output/remove.json`, `output/after-removal.json` |

A UI foi conferida em Visão geral, Conexões, Regras, Diagnóstico e Configurações.
Não foram acionados controles de rede, qualidade, agente ou inicialização;
nenhuma preferência foi alterada. A ausência de processos foi confirmada após
Sair, antes da reinstalação; nenhum processo foi encerrado à força.

O estado final do convidado contém apenas `qa-unknown-preserve.txt` na pasta do
programa e `qa-preserve.txt`/`netlane-rules.json` no perfil. Seus hashes foram
comparados antes de descartar a sessão. O fechamento do Sandbox não substitui
a prova de remoção pelo desinstalador. HKCU Run e o estado legado permaneceram
iguais; não houve ensaio com Run estrangeiro ou .NET instalado no convidado.

`host-verification.json` confirma os 14 grupos do host iguais ao baseline e
estado Rust ausente no host. .NET instalado continua 0.3.4.0 e HEAD
`3a99a1a4e81e6fad52ec587a2aa5f6fe034a3bc5`. Os 14 arquivos preexistentes,
quatro recibos/insumos históricos protegidos, nove insumos desta sessão e as
40 evidências do checkpoint anterior mantêm os hashes. Nenhum instalador foi
executado no host. `sandbox-after.json` confirma a sessão encerrada.

O consolidado `result.json`, SHA-256
`6E6A3F2B1F1A570BBB496CCD2F6F635548B79B8AAFBAAD5A4DDE895C55C808C1`, registra
`BasicSandboxQaPassed=true`, `FullInstallerQaVerified=false` e
`PublicReleaseReady=false`. `evidence-inventory.json` preserva o inventário
final por hash. Assinatura, usuário comum/UAC, assistente/cancelamento,
upgrade entre versões, downgrade/reparo, guardas, autorun/logon, teclado/DPI,
bandeja completa, convivência instalada com .NET e ensaios de rede/agente
continuam pendentes. A reinstalação da mesma versão não comprova upgrade real.

## Interrupção e retomada preservadas

A primeira captura da janela encontrou a tela de bloqueio do Windows. A
automação visual foi interrompida e o desbloqueio solicitado. O usuário informou
“Pronto”; a janela foi novamente identificada e conferida antes da UI. Os passos
restantes foram executados na mesma sessão, sem repetir a instalação limpa ou
o redistribuível. `checkpoint-before-unlock.json`,
`evidence-inventory-before-unlock.json` e `output/ui-deferred.json` permanecem
como evidências históricas da interrupção, sem serem substituídos pelo resultado
concluído. A primeira rodada não é usada como aprovação automática desta rodada.

O primeiro coletor complementar representou uma entrada Run ausente como
propriedade nula; `host-rust-current-v2.json` usa o objeto vazio do baseline.
Não houve alteração do registro. A rodada com 8.192 MB não registrou os erros
de falta de memória dos auxiliares encontrados na primeira sessão de 4.096 MB.

Sem instalação no host, assinatura, commit, push ou gravação de memória.
