# QA estático do empacotamento Rust — 2026-10-10

Prévia Rust 0.4.0 preparada localmente, sem substituir o .NET 0.3.4.0 instalado.
Não houve execução de instalador/painel/agente de produção, instalação, UAC,
assinatura, teste de rede, alteração de autorun ou publicação Git nesta etapa.

Estado mais recente: **710 arquivos conferidos e nenhum texto ausente** no
inventário Cargo. Os recibos dos pacotes anteriores foram preservados; a
[atualização final](#atualização-final-textos-de-licença-coletados) detalha
a origem Git da licença de seahash e os novos recibos. Runtime/entrega pública
continuam pendentes.

## Resultado

| Verificação | Evidência |
| --- | --- |
| Regressão Rust | 66 testes aprovados: agente 10, app 9, core 25, Windows 22 |
| Compilação | formatação, Clippy comum e build Release aprovados |
| Instalador | NSIS 3.12, por usuário, sem assinatura; 6.696.660 bytes |
| Extração estática | 708 arquivos por tamanho/SHA-256; fontes e includes conferidos |
| Executáveis | dois PE x64; UI/instaladores asInvoker; agente sem elevação automática |
| Pré-requisitos | imports PE registrados; Visual C++ x64 mínimo 14.44.35211.0 e ICU do Windows |
| Licenças | 390 crates no inventário conservador; textos de 14 crates suplementados por commit/hash de origem |
| Recusas estáticas | oito casos aprovados antes de iniciar a extração |
| Preservação | 14 arquivos locais preexistentes com SHA-256 igual ao baseline |

Setup: `artifacts/rust-installer-20261010-final/NetLane-Rust-0.4.0-preview-win-x64-setup.exe`.
SHA-256: `F3985FF23889D0800F0F411387C2A1C30D86C1A4A507077F0C921563449F5680`.

Recibo de build: `artifacts/rust-installer-20261010-final/build-report.json`.
Extração: `archive-check-f6c3c3eb/verification.json` nessa mesma pasta.
Recusas: `artifacts/rust-package-negative-20261010-071704-9ff6e57e/result.json`.
Baseline: `artifacts/rust-packaging-20261010-065653/preexisting-files.json`.

As verificações Rust completas passaram na primeira tentativa. As tentativas
seguintes corrigiram somente scripts/empacotamento; o pacote final registra
`ChecksSkipped=true` porque os checks já tinham passado, e reconstruiu o Release
comum. Nenhum código funcional Rust mudou. Provas GPUI anteriores não foram
repetidas nem apresentadas como prova de instalação nesta rodada.

Os oito casos negativos cobrem fonte alterada, setup alterado, fonte duplicada,
manifesto com caminho externo, arquivo duplicado, inclusão de regras pessoais,
ADS e payload substituído por junção. Usam somente fixtures novas em `artifacts`.
São testes do verificador estático; não exercitam as guardas NSIS em execução.

## Pendências do primeiro pacote

Faltam os textos das licenças de **seahash 4.1.0** e **simd_helpers 0.1.0**.
Ambas declaram MIT nos metadados, mas não foi inventado um texto substituto.
O repositório seahash informado pela crate recusou leitura anônima com HTTP 403;
a revisão exata do repositório simd_helpers não contém arquivo de licença na raiz,
e o README restaurado não contém o texto. Os dois itens permanecem em `Issues`.
A licença do produto e a revisão final de obrigações/avisos nativos/runtime
também permanecem gates de distribuição pública.

`InstallerExecuted=false`, `RuntimeQaVerified=false`, `PublicReleaseReady=false`.
Instalação, cancelamento, upgrade/reparo/downgrade, remoção, guardas de processos,
runtime Visual C++/ICU, autorun/logon e UI instalada precisam do laboratório
autorizado, conforme [procedimento Rust](instalador-rust.md). WFP, sondagens,
métricas/restauração/reboot e convivência continuam ensaios separados.

A instalação por usuário é uma prévia paralela; não decide a embalagem do
cutover definitivo. Os scripts .NET e suas alterações anteriores foram
preservados, assim como o histórico do bloqueio no Sandbox. Não recriar ou
executar o pacote bloqueado para contornar a proteção.

## Atualização: suplemento de licença e laboratório

O pacote anterior e seus recibos foram preservados. A nova prévia está em
`artifacts/rust-installer-20261010-license-followup/NetLane-Rust-0.4.0-preview-win-x64-setup.exe`,
com **6.697.846 bytes** e SHA-256
`84D8F7CC3F536171706330FBEB36EA3253A1A849ED17E541D0D50CC67064AEF9`.
Release comum reconstruído; formatação/testes/Clippy anteriores continuam
registrados separadamente, sem repetir provas funcionais para alterações
somente no empacotamento. O build registra `ChecksSkipped=true`.

Foram conferidos **709 arquivos**, 390 crates e 693 textos/avisos no inventário.
`simd_helpers 0.1.0` recebeu o texto MIT publicado pelo próprio autor no
[commit posterior](https://github.com/lu-zero/simd_helpers/commit/82040194cd05affb060bf94d6f19f82a771d07fb).
O pai é exatamente a revisão da crate; a única mudança é a adição de `LICENSE`.
O builder confere a evidência da API, commit/pai/arquivo, SHA-256 e blob Git.
O inventário mantém a proveniência. **Resta um texto ausente: seahash 4.1.0**.
Nesta atualização, URLs raw/API da origem informada retornaram HTTP 404;
isso não comprova remoção do repositório nem resolve a pendência.

Validações adicionais:

- Um suplemento válido e cinco recusas: evidência adulterada, pai incorreto,
  alteração de fonte, blob incorreto e texto adulterado.
  Recibo: `artifacts/rust-license-guards-20261010-074405-ec606549/result.json`.
- Oito recusas do verificador estático aprovadas para o novo pacote.
  Recibo: `artifacts/rust-package-negative-20261010-074547-f71e01f2/result.json`.
- Sete arquivos do bundle de laboratório conferidos por tamanho/hash e ambos
  os modos do coletor recusados no host antes de consultar o convidado.
  Recibo: `artifacts/rust-lab-host-guard-20261010-074635-2e003872/result.json`.
- Sintaxe dos seis scripts alterados/adicionados conferida; `git diff --check`
  sem erros. Os 14 arquivos preexistentes continuam com os hashes do baseline;
  a instalação .NET continua em 0.3.4.0.

Bundle: `artifacts/rust-installer-lab-20261010-final`, com setup Rust, manifesto,
recibos, plano incompleto, roteiro e coletor. O [procedimento do laboratório](qa-laboratorio-rust-2026-10-10.md)
separa identificação em leitura, baseline/snapshot e quatro primeiros casos
propostos. A validação do coletor foi de sintaxe/recusa no host; sua captura
completa no Windows convidado ainda não foi exercitada.

`vmrun list` confirmou a VM em execução, mas `listSnapshots` continua exigindo
senha de criptografia. Identidade do convidado/snapshot não conferidos;
nenhuma transferência, execução de instalador/produto, mudança de rede/autorun,
assinatura ou publicação Git. A revisão de obrigações/licença do produto e
os gates runtime continuam pendentes. `RuntimeQaVerified=false` e
`PublicReleaseReady=false` continuam explícitos.

## Atualização final: textos de licença coletados

A leitura do Git da origem funcionou mesmo com as páginas HTTP inacessíveis.
A tag `v4.1.0` resolve para `94b632aeac099031c373599313d5b5f0acbbaec0`, igual à
revisão publicada da crate. Seu filho direto
[`3088c5c912b70b586d27bf553fbe964e025a2c89`](https://gitlab.redox-os.org/redox-os/seahash/-/commit/3088c5c912b70b586d27bf553fbe964e025a2c89)
adiciona somente `LICENSE`. O texto foi copiado literalmente da origem,
sem construir um substituto. O builder recalcula IDs dos objetos brutos
commit/tree do pai/filho, confere as árvores completas, o blob da licença
e o SHA-256 do texto. Não acessa a rede para essa conferência.

Origem coletada em `artifacts/seahash-license-source-20261010-085946/repository.git`.
O suplemento e sua prova estão em
`assets/licenses/rust-supplements/redox-os-seahash-3088c5c9`.
O texto publicado não contém uma linha explícita de copyright; esse fato
permanece em `ReviewNote` para a revisão de autoria/obrigações, sem inventar
uma linha de atribuição. A ausência de textos no inventário foi resolvida;
isso não aprova a licença do produto nem a distribuição pública.

Nova prévia: `artifacts/rust-installer-20261010-license-texts/NetLane-Rust-0.4.0-preview-win-x64-setup.exe`.
Tamanho: **6.698.222 bytes**. SHA-256:
`26F97A61A381404325A4FB70D9DBF7044A2B7C335FF08B65C39EF4C0B35169C2`.
Release comum reconstruído, `ChecksSkipped=true`, sem mudança de código
funcional Rust. Extração: **710 arquivos**, 390 crates, 694 textos/avisos,
nenhum item em `LicenseReview.Issues`. Assinatura continua ausente.

| Verificação desta atualização | Recibo |
| --- | --- |
| simd_helpers: um caso válido e cinco recusas | `artifacts/rust-license-guards-simd_helpers-20261010-090450-7b00675f/result.json` |
| seahash: um caso válido e dez recusas | `artifacts/rust-license-guards-seahash-20261010-090452-181e8437/result.json` |
| Extração do pacote e fontes | `artifacts/rust-installer-20261010-license-texts/archive-check-02a85ac9/verification.json` |
| Oito recusas do verificador estático | `artifacts/rust-package-negative-20261010-090732-c609b87a/result.json` |
| Sete arquivos do bundle e duas recusas do host | `artifacts/rust-lab-host-guard-20261010-090805-d730331f/result.json` |

As recusas seahash incluem também adulteração de cada um dos quatro objetos
Git brutos e da URL de origem, mesmo com SHA-256 do recibo recalculado na fixture.
O bundle atualizado é `artifacts/rust-installer-lab-20261010-license-texts`.
O plano permanece incompleto, sem conferência do convidado/snapshot e sem
execução runtime autorizada. `vmrun` voltou a confirmar a VM em execução e a
recusar a consulta de snapshots por exigir senha de criptografia.

Os 14 arquivos preexistentes continuam íntegros; .NET instalado 0.3.4.0 e HEAD
`3a99a1a4e81e6fad52ec587a2aa5f6fe034a3bc5` preservados. Não houve execução de
instalador, UI ou agente, alteração de rede/autorun, assinatura, commit ou push.
`RuntimeQaVerified=false` e `PublicReleaseReady=false` continuam nos recibos.

## Rodada posterior: QA básico no Sandbox

Depois desta coleta estática, o usuário autorizou continuar com o Sandbox.
A [rodada Rust de instalação, UI demo, reinstalação e remoção](qa-sandbox-rust-2026-10-10.md)
foi concluída com o pacote de SHA-256 `26F97A61A381404325A4FB70D9DBF7044A2B7C335FF08B65C39EF4C0B35169C2`.
Os recibos estáticos anteriores foram preservados; a nova evidência runtime
fica em `artifacts/rust-sandbox-20261010-092609`. Usuário comum, upgrade real,
rede/agente e outros gates continuam pendentes; não há aprovação pública.

## Correção posterior do charset NSIS

O [pacote com UTF-8 explícito](qa-sandbox-rust-utf8-2026-10-10.md), SHA-256
`325DEFDF57FFC276D4762D0C650603DFF2547D10BAE30CD0EC29E8900DA6DC8B`, passou na
extração dos 710 arquivos e nas oito recusas estáticas. Executáveis Rust iguais
por hash; `-SkipChecks` registrado e build Release executado. O nome no registro
foi confirmado com U+00E9 em nova instalação no Sandbox. UI demo, reinstalação
0.4.0 sobre 0.4.0 e remoção também passaram no novo pacote, com dados fictícios
preservados. A sessão foi encerrada e o host/.NET mantidos. Os recibos anteriores
permanecem íntegros. Não há aprovação completa nem entrega pública.
