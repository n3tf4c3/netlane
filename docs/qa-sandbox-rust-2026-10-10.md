# QA Rust no Windows Sandbox — 2026-10-10

Atualização posterior: o [novo pacote com UTF-8 explícito](qa-sandbox-rust-utf8-2026-10-10.md)
resolveu o nome no registro em outra instalação limpa e repetiu os quatro
fluxos básicos, com Sandbox encerrado e host preservado. O relato e os recibos
abaixo descrevem a primeira rodada e preservam o achado original.

A rodada autorizada concluiu instalação limpa, UI em demonstração,
reinstalação da mesma versão e remoção do Rust 0.4.0. O Sandbox foi encerrado;
o .NET 0.3.4.0 instalado e o estado comparado do host foram preservados.
Esta é uma aprovação dos quatro passos básicos neste ambiente, com token
administrativo do usuário padrão do Sandbox. Não conclui todos os gates do
[instalador Rust](instalador-rust.md).
Foi encontrado um defeito visual de acentuação no nome gravado no registro;
ele está documentado abaixo e permanece pendente.

## Pacote e ambiente

- Instalador: `NetLane-Rust-0.4.0-preview-win-x64-setup.exe`, 6.698.222 bytes,
  sem assinatura, SHA-256
  `26F97A61A381404325A4FB70D9DBF7044A2B7C335FF08B65C39EF4C0B35169C2`.
- Origem: `artifacts/rust-installer-20261010-license-texts`; pacote já validado
  estaticamente, sem reconstrução nesta rodada.
- Sessão: `90d7b1da-f922-4726-b5f8-fdb2b7750903`; convidado
  `143415CC-96E6-4`, Windows 11 Enterprise x64, build 26100.
- Rede, áudio/vídeo de entrada, clipboard e impressoras desativados; vGPU
  habilitada para o GPUI; memória configurada em 4.096 MB.
- Apenas insumos Rust compartilhados em leitura e uma pasta de evidências
  compartilhada para gravação. Checkout, dados pessoais e instaladores .NET
  não foram compartilhados.
- `WDAGUtilityAccount` estava com token administrativo. Instalação com usuário
  comum e ausência de UAC nesse token continuam sem prova.

O preflight encontrou ICU, mas não Visual C++ x64. O redistribuível foi obtido
pelo [link oficial da Microsoft](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170),
com assinatura Microsoft válida conferida no host e no convidado. Somente o
Sandbox recebeu `/install /quiet /norestart`: versão 14.51.36247.0, SHA-256
`843068991DAAA1F73AD9F6239BCE4D0F6A07A51F18C37EA2A867E9BECA71295C`.
Logs do bootstrapper e das duas transações MSI confirmam sucesso e nenhum reboot.

O coletor leu `VerifiedAndReputablePolicyState=2`, mas `CiTool -lp -json`
retornou `-2147009597`; o inventário completo de políticas não foi obtido.
Nenhuma política/proteção foi modificada. O setup Rust desta rodada iniciou
e concluiu; isso não libera nem repete o pacote .NET bloqueado em 2026-10-08.

## Resultados

Base das evidências: `artifacts/rust-sandbox-20261010-092609`.

| Passo | Resultado conferido | Recibo |
| --- | --- | --- |
| Instalação limpa | retorno 0; 710 arquivos por tamanho/SHA-256; registro e atalho próprios; nenhum painel/agente automático, perfil padrão ou Run criado | `install-verification.json`, `output/install.json` |
| UI instalada | executável Release comum em `--demo --profile C:\NetLaneRustQaDemo`; cinco páginas renderizadas e navegação conferida; Sair encerrou normalmente; perfil padrão ausente e payload íntegro | `ui-verification.json`, `output/afterdemo-v2.json`, cinco `output/ui-*.png` |
| Reinstalação 0.4.0 sobre 0.4.0 | retorno 0; mesmos 710 arquivos íntegros; arquivo desconhecido no programa e dois arquivos fictícios no perfil preservados | `output/reinstall-v2.json`, `output/fixtures.json` |
| Remoção | launcher retornou 0; aguardada remoção real dos 711 arquivos conhecidos, incluindo o desinstalador; registro/atalho removidos; três fixtures preservadas | `output/remove-v2.json`, `output/after-removal.json` |

A UI foi conferida em Visão geral, Conexões, Regras, Diagnóstico e Configurações.
Não foram acionados controles de rede, qualidade, agente ou inicialização;
nenhuma preferência foi alterada. O perfil fictício foi criado pelo executor
somente depois da saída normal da demonstração, para o caso de preservação.
HKCU Run permaneceu vazio no convidado; não houve ensaio com Run estrangeiro.

O estado final contém somente `qa-unknown-preserve.txt` na pasta do programa e
`qa-preserve.txt`/`netlane-rules.json` no perfil. Seus hashes foram conferidos
antes de descartar a sessão. O fechamento do Sandbox não foi usado como prova
da remoção: o desinstalador já havia concluído a limpeza do payload.

## Incidentes dos auxiliares, preservados

O acompanhamento inicial do redistribuível atingiu 120 segundos. A mesma
instalação continuou, sem repetição ou encerramento forçado: os logs registram
sucesso do runtime mínimo às 09:31:22 e do adicional às 09:33:23 (America/Cuiabá).
Um observador posterior terminou com `System.OutOfMemoryException`; o inventário
independente confirmou ausência dos processos do redistribuível, e o novo
preflight confirmou runtime compatível antes do setup Rust.

A primeira chamada do coletor de UI falhou antes de tentar abrir o produto:
uma lista vazia de processos foi tratada como objeto sem `Count`. A correção
preserva a lista como array. A primeira inicialização do PowerShell para a
revisão corrigida reportou `Thread failed to start`, também antes da tentativa
de UI. Uma consulta independente confirmou PowerShell funcional e nenhum
processo/registro de tentativa do produto; só então a etapa foi executada.
Os recibos, fontes, planos e consoles anteriores foram preservados.

Nenhuma tentativa de Rust foi bloqueada pela proteção. Não houve repetição,
reconstrução ou liberação de arquivo bloqueado.

## Achado pendente: acentuação do nome no registro

`DisplayName` foi gravado como `NetLane Rust (prÃ©via local)`, em vez de
`NetLane Rust (prévia local)`. O JSON foi relido explicitamente como UTF-8 e
os caracteres foram conferidos: o campo contém U+00C3/U+00A9 no lugar de U+00E9.
Portanto, o defeito está no metadado instalado, não apenas na exibição do console.
Versão, caminho, comando de remoção e atalho passaram nas verificações.

O fonte NSIS declara o nome correto em UTF-8 sem BOM. A leitura de charset na
compilação precisa ser corrigida e comprovada no próximo pacote; mensagens
acentuadas do assistente também precisam de conferência. Nenhum novo instalador
foi gerado nesta rodada. `finding-registration-name.json` preserva o achado.
Os quatro fluxos básicos foram concluídos com essa pendência visual; o recibo
consolidado não aprova a qualidade completa nem a entrega do instalador.

## Preservação e limites

`host-verification.json` confirma igualdade nos 14 grupos comparados: registro,
startup, programa, perfil, menu Iniciar, processos, adaptadores, bindings,
interfaces/métricas atuais e persistentes, endereços, rotas padrão, DNS e flags
globais de rede. O estado Rust do host também é igual ao baseline; os 14 arquivos
preexistentes mantêm seus hashes. HEAD continua
`3a99a1a4e81e6fad52ec587a2aa5f6fe034a3bc5` e a versão instalada .NET é 0.3.4.0.
Nenhum instalador foi executado no host.

`sandbox-after.json` confirma a sessão encerrada. O recibo consolidado é
`result.json`; fontes e planos usados ficam em `input`, com inventário final
por hash em `evidence-inventory.json`. Os novos executores são
`scripts/test-rust-installer-sandbox.ps1` e
`scripts/test-rust-sandbox-lifecycle.ps1`; recusam o host, vinculam o convidado
ao baseline e não encerram processos à força. Jornadas de execução e recibos
são criados sem substituir os anteriores.

Continuam pendentes correção da acentuação no registro/assistente, cancelamento,
usuário comum/UAC, upgrade entre versões,
downgrade, reparo de falha parcial, guardas runtime, autorun/logon, teclado/DPI,
bandeja completa e convivência instalada com .NET. Agente elevado, WFP,
qualidade e alterações/restauração de rede exigem ensaios próprios autorizados.
Assinatura, revisão final das licenças e entrega pública também continuam
pendentes. `BasicSandboxQaPassed=true`, `FullInstallerQaVerified=false` e
`PublicReleaseReady=false`. Não houve commit, push ou gravação de memória.
