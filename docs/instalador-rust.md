# Empacotamento local Rust

O pacote Rust 0.4.0 é uma prévia para QA, separada da instalação .NET.
Compilar e extrair o pacote são verificações estáticas. Não executar o instalador
no host; instalação, atualização e remoção exigem laboratório autorizado com
baseline e snapshot. Assinatura e distribuição pública continuam pendentes.

## Layout e ciclo de vida

| Item | Contrato do pacote |
| --- | --- |
| Destino | `%ProgramFiles%\NetLane Rust`, fixo, para todos os usuários (gravável só por administradores) |
| Executáveis | `netlane.exe` e `netlane-agent.exe`, x64, build Release comum |
| Elevação | instalação e remoção pedem administrador (`requireAdministrator`; código 9 sem token administrativo); o painel roda sem UAC e só as ações manuais do agente solicitam UAC |
| Perfil | `%LOCALAPPDATA%\NetLane-Rust` de cada usuário; instalador não lê, importa ou remove perfis |
| Identidade | `netlane-rust-installed.layout` com `1` e `rust-package-version.txt` |
| Menu Iniciar | `NetLane Rust.lnk`, no menu de todos os usuários |
| Registro | HKLM Uninstall `NetLaneRustPreview`; produto .NET preservado |
| Inicialização | opção manual `NetLaneRust` em HKCU Run de cada usuário, desligada em instalação limpa |
| Upgrade | mesma pasta e versão igual/mais recente; perfis e autorun preservados |
| Prévia per-user anterior | instalação recusada (código 4) enquanto existir `%LOCALAPPDATA%\Programs\NetLane Rust` ou o registro HKCU; remova-a antes, o perfil é preservado |
| Remoção | lista compilada de arquivos; remove apenas o comando Rust esperado do usuário que executa a remoção |

O agente elevado fica numa pasta que só administradores alteram: um processo
do usuário não consegue trocar `netlane-agent.exe` nem plantar DLL ao lado dele
antes do próximo UAC. A versão portátil não leva o agente (ver
[migração](migracao-rust.md)).

O instalador não abre o painel, não inicia agente, não altera rede e não grava
Run. A remoção preserva entradas Run de outros caminhos, o valor legado NetLane,
perfil/backup e arquivos desconhecidos. Entradas Run de outros usuários não são
lidas (os perfis deles não estão carregados); um comando de inicialização para
arquivo ausente é ignorado pelo Windows. Nunca faz exclusão recursiva nem usa o
manifesto instalado como instruções de remoção.

As guardas recusam processos Rust ou .NET em qualquer sessão, enumeração
incompleta, downgrade, desinstalador de outra pasta/versão, links/junções nos
ancestrais e arquivos gerenciados, tipos incompatíveis e arquivos em uso.
O destino `/D` não é aceito. As verificações são repetidas após a página inicial.
Falha de cópia aborta com código 7 e mantém identificação para reparo por nova
execução. **Não há rollback automático**, e reparo ainda exige prova runtime.
Arquivos retirados de uma futura versão precisam de lista explícita de limpeza;
não há migração automática da versão .NET nem cutover nesta etapa.

## Compilar e verificar sem instalar

Use a distribuição portátil NSIS 3.12 já conferida. O script não instala
ferramentas, não assina e não executa o instalador. A pasta de saída deve ser
nova, dentro de `artifacts`, sem links/junções. A versão vem de `Cargo.toml`.

Os executáveis usam CRT dinâmico. O builder registra imports PE e a versão do
`vcruntime140.dll` x64 da redistribuição correspondente ao MSVC local. A referência
é encontrada via vswhere, ou informada com `-VcRuntimeReferencePath`.
O instalador verifica a versão do runtime x64 do sistema antes de copiar;
ausência/versão inferior retorna 8. Não inclui, baixa ou instala o runtime.
Microsoft exige runtime da arquitetura correspondente e pelo menos tão recente
quanto o toolchain: [pré-requisitos oficiais](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170).
Essa checagem preliminar não substitui abrir o painel no laboratório nem prova
compatibilidade de drivers/GPU ou dos demais imports nativos.
O import `icuuc.dll` também exige ICU do Windows x64; o instalador recusa sua
ausência. [ICU no Windows](https://learn.microsoft.com/en-us/windows/win32/intl/international-components-for-unicode--icu-)
é outro pré-requisito para QA em versões antigas do sistema.

```powershell
.\scripts\build-rust-installer.ps1 `
  -NsisCompilerPath '.\artifacts\installer-tools-20260913\compiler\nsis-3.12\makensis.exe'
.\scripts\test-rust-installer.ps1 `
  -BuildDirectory '.\artifacts\rust-installer-AAAAmmdd-HHMMSS' `
  -SevenZipPath 'C:\Program Files\7-Zip\7z.exe'
```

O builder executa formatação, testes, Clippy e build Release comum. `-SkipChecks`
é explícito no recibo e nunca pula o build. Não habilita `gui-test`. O marcador
de instalação existe somente no payload do instalador; prévias portáteis
continuam sem marcador. Não abra o payload extraído como se estivesse instalado.

A compilação NSIS usa `/INPUTCHARSET UTF8` para os fontes/includes sem BOM e
registra `CompilerInputCharset` no recibo. Isso preserva nomes e mensagens
acentuadas independentemente da página de código do Windows, conforme a
[documentação do compilador](https://nsis.sourceforge.io/Docs/Chapter3.html).

São preservados fontes NSIS, includes gerados, scripts, metadados Cargo,
manifesto por caminho/tamanho/SHA-256 e hash do setup. O verificador confere
esses insumos e o payload antes da extração por 7-Zip, rejeita caminhos externos,
duplicatas, arquivos extras e dados pessoais, e verifica executáveis x64 e
manifestos `asInvoker` da UI/instaladores e ausência de elevação automática do
agente, que é lançado explicitamente com `runas`. Esses recibos estáticos mantêm `InstallerExecuted=false`,
`RuntimeQaVerified=false` e `PublicReleaseReady=false`.

## Licenças

O pacote inclui inventário das dependências transitivas e os textos encontrados
nos fontes Cargo restaurados, incluindo avisos em subpastas de bibliotecas
nativas e licenças individuais dos temas. Não inventa textos de licença para
uma crate que só declara a expressão: registra a ausência em `Issues`.
Textos omitidos pela publicação de uma crate podem ser suplementados em
`assets/licenses/rust-supplements.json`: revisão exata de `.cargo_vcs_info.json`,
URL primária fixada por commit e SHA-256 do texto. O builder verifica esses três
vínculos e não acessa a rede para coletar licenças.

Um suplemento pode apontar para o filho direto da revisão da crate quando esse
commit adiciona **somente `LICENSE`**, sem alterar fontes ou manifesto. O campo
`LicenseOnlyFollowup` preserva a resposta primária da API por caminho/hash,
ou um recibo `git-object-proof` coletado do Git da origem. O recibo Git inclui
objetos brutos de commit/tree do pai e do filho; o builder recalcula seus IDs
e confere que a árvore inteira mudou somente pela adição de `LICENSE`.
O builder verifica commit, pai, arquivo único adicionado e hash Git do blob,
além do SHA-256 do texto. Esse vínculo fecha a coleta do texto, sem aprovar
a revisão final das obrigações para distribuição.

Os textos e recibos de origem em `assets/licenses/rust-supplements` preservam
seus bytes por `.gitattributes` (`-text`). A conversão automática LF/CRLF do Git
invalidaria os SHA-256 fixados no manifesto. A preparação da publicação confere
os 32 vínculos de texto/evidência em uma exportação do índice com
`core.autocrlf=true`; esta regra não muda o conteúdo dos avisos.

Usa `cargo metadata --locked --offline --filter-platform x86_64-pc-windows-msvc`,
formato 1, com travessia de arestas normais/build, excluindo arestas dev.
É um inventário conservador de fontes, que pode incluir ferramentas de build;
não é uma lista exata de código ligado ao executável. O formato e os limites
vêm da [documentação Cargo](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html).
NSIS e os recursos gráficos também acompanham o pacote.

Antes de distribuição pública, fechar textos ausentes, escolhas/obrigações das
expressões de licença, avisos de bibliotecas nativas, runtime Rust/MSVC/Windows
e a licença do próprio produto. A coleta automática não aprova essas decisões.

## Primeira rodada runtime no Sandbox

O [QA Rust no Sandbox de 2026-10-10](qa-sandbox-rust-2026-10-10.md) concluiu
instalação limpa, UI instalada em demonstração com saída normal, reinstalação
da mesma versão e remoção com preservação de dados fictícios/arquivo desconhecido.
O token padrão do Sandbox era administrativo; usuário comum/UAC, cancelamento,
upgrade entre versões, falha parcial e os demais gates abaixo continuam pendentes.
Também foi identificada acentuação incorreta no nome gravado no registro.
A [rodada posterior com UTF-8 explícito](qa-sandbox-rust-utf8-2026-10-10.md)
confirmou a correção do nome em nova instalação e repetiu os quatro fluxos no
novo pacote, com 710 hashes íntegros e preservação dos dados fictícios.
Mensagens do assistente e cancelamento ainda precisam de conferência visual.
O host e o .NET instalado foram preservados; a sessão foi descartada depois
de salvar a prova da remoção. Isso não constitui aprovação de entrega pública.

Essas rodadas usaram o layout **per-user** anterior. O pacote passou a ser por
máquina em 2026-10-11 (achado 1 da auditoria); a mudança tem apenas compilação,
extração estática e as oito recusas negativas aprovadas. Instalação, upgrade,
remoção, recusa da prévia per-user (código 4), recusa sem administrador
(código 9) e o UAC da instalação precisam de nova rodada no Sandbox/VM. A
[rodada de 2026-10-11](qa-sandbox-rust-permachine-2026-10-11.md) foi bloqueada
pelo Smart App Control antes de executar o instalador sem assinatura. A
[rodada na VM de laboratório](qa-laboratorio-rust-permachine-2026-10-11.md) aprovou
recusas por runtime antigo (código 8), prévia per-user (4) e falta de
administrador (9), instalação limpa com pasta e agente não graváveis por usuários
comuns, smoke do agente, reinstalação e remoção.

## Gates runtime do laboratório

| Cenário | Prova necessária |
| --- | --- |
| Cancelamento e instalação limpa | estado preservado ao cancelar; UAC da instalação; hashes, atalho comum, registro HKLM, ACL herdada de Arquivos de Programas, UI sem UAC e perfil vazio após instalar |
| Migração da prévia per-user | instalação recusada com código 4 e nada alterado; após remover a prévia per-user, instalação por máquina preserva o perfil |
| Runtime Visual C++ x64 | ausência/versão antiga recusadas sem cópia; runtime compatível permite abrir UI/agente |
| Upgrade/reparo/downgrade | perfil fictício e arquivos desconhecidos preservados; downgrade recusado; falha parcial reparável |
| Guardas | UI/bandeja/agente/outro usuário; links, arquivos em uso e tipos incompatíveis; nenhuma terminação forçada |
| Autorun/logon | marcar/desmarcar na UI; um painel oculto em observação após login; remoção só do comando esperado |
| Remoção/falhas | payload conhecido removido; perfil, arquivos desconhecidos e Run estrangeiro preservados; falha explícita |
| Interface instalada | teclado, escalas, ícones, bandeja, diálogos e janela mínima |

Qualidade, UAC do agente, WFP, métricas/restauração/reboot e convivência com a
implementação .NET exigem ensaios próprios. Não reutilizar evidência .NET como
aprovação Rust. Não executar nem recriar o instalador .NET bloqueado no Sandbox
para contornar a proteção. O pacote Rust não encerra esse histórico.

O [roteiro de laboratório Rust](qa-laboratorio-rust-2026-10-10.md) acompanha uma
preparação separada por `scripts/prepare-rust-installer-lab.ps1`. O coletor
`scripts/capture-rust-installer-lab-state.ps1` identifica o convidado e captura
baseline/estados posteriores em leitura; ele nunca executa instalador ou produto.
O modo de identificação exige VMware/MAC e recusa o host. A coleta de baseline
exige também computador/SID/UUID/pasta do convidado e registro do snapshot.
Esses recibos não confirmam os passos manuais nem autorizam sua execução.
