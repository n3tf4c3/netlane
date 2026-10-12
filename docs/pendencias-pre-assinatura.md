# Pendências do NetLane antes da assinatura

**Atualização de 2026-10-12:** gerada e aprovada a **0.3.6 assinada** — ver [pacote 0.3.6](#pacote-036-assinado--2026-10-12). Ela inclui todo o código .NET atual (a 0.3.5 testada era do commit `687f016` e não tinha `a95979d`/`97d11df`), a guarda de processo antes da de destino e o ícone da identidade nova. As três decisões abertas foram resolvidas: ordem das guardas corrigida, reparo por nova execução aceito sem rollback automático e ícone trocado. A 0.3.5 fica substituída e não será assinada. Permanecem fora do instalador o ensaio de conexão padrão + roteamento por aplicativo e as escalas 125%/150%.

**Atualização de 2026-10-11:** os quatro casos iniciais (instalação limpa 0.3.4, upgrade para 0.3.5 com fixtures, downgrade recusado e remoção) foram **aprovados na VM de laboratório** — ver [rodada na VM](#rodada-na-vm-de-laboratório--2026-10-11). Na mesma VM, a [matriz sem interface](#matriz-sem-interface-na-vm--2026-10-11) também passou: guarda de processo, arquivo em uso, destinos inválidos, falhas na remoção e falha de cópia com reparo. Os [casos interativos](#casos-interativos-na-vm--2026-10-11) (cancelamento, UI real, autorun pela UI, logon real e UI instalada) também foram exercitados. A 0.3.5 segue sem assinatura até decidir três pontos: ordem das guardas (recusa 7 em vez de 2 com a UI aberta), reparo sem rollback automático e ícone antigo. O SimplySign já assina normalmente (pacote Rust de 2026-10-11).

Atualizado em 2026-10-08. A orientação vigente é resolver as pendências antes de assinar a nova versão. **A 0.3.5 permanece sem assinatura e o QA completo do instalador permanece pendente.** A rodada autorizada no Windows Sandbox foi bloqueada pelo Controle de Aplicativo antes da execução do instalador 0.3.4: **0 de 4 casos concluídos**, sem tentativa da 0.3.5. O Sandbox foi encerrado e o estado do host foi conferido como preservado.

## Correções preparadas e verificadas localmente

| Item | Resultado | Limite da verificação |
| --- | --- | --- |
| Dependência JSON vulnerável | `System.Text.Json` fixado em 8.0.6; consulta NuGet sem vulnerabilidades conhecidas | Não substitui ensaios de instalação ou rede |
| Regressão do produto | 283 testes de produção, 75 QUIC e 37 de bandeja aprovados na revisão de 08/10 | Sessões e tráfego simulados; não repetidos após ajustes somente no empacotamento |
| Downgrade | Recusa no início e repetida antes da cópia | Compilação aprovada; recusa runtime pendente |
| Links e arquivos em uso | Guardas ampliadas para desinstalador, pasta e atalho do menu Iniciar; arquivo substituído por pasta também recusado | Compilação aprovada; cenários runtime pendentes |
| Falhas na remoção | Erros na exclusão do atalho, registro do produto e autorun deixam de passar como sucesso; marcador e desinstalador mantidos até essas etapas terminarem | Compilação aprovada; falhas induzidas pendentes |
| Arquivos retirados do pacote | Quatro avisos de `System.Text.Json`/`System.Text.Encodings.Web` 8.0.0 entram em lista compilada de limpeza | Não usa manifesto anterior como lista de exclusão; upgrade runtime pendente |
| Origem da compilação | Duas fontes NSIS e quatro includes gerados preservados com SHA-256 | Verifica os insumos registrados; não prova as guardas em execução |
| Verificador estático | Fonte alterada, inventário duplicado e caminho externo recusados em três casos isolados | Nenhum instalador executado ou extraído nesses casos negativos |
| Coleta de QA | Script de estado testado somente em leitura no host | Registra hashes e configurações; não aprova um cenário automaticamente |

Prévia para os ensaios: `artifacts/installer/pre-sign-20261008-035-v2/NetLane-0.3.5-preview-win-x64-setup.exe`, SHA-256 `020EE3A9EACF2A991707F032A7F898448E4A86CF1B886CB3C19B0EAFC36BB281`, 69.485.332 bytes. Extração aprovada em **757 arquivos**, com as seis fontes de empacotamento conferidas, UI x64 `asInvoker` e instalador/desinstalador administrativos. `InstallerExecuted=false`, `RuntimeGuardVerified=false`, `InstallationVerified=false` e `Signed=false`.

Evidências locais: `archive-check-c610ce5e/verification.json` nessa pasta; três recusas em `artifacts/pre-sign-negative-20261008/result.json`; testes/dependências em `artifacts/revisao-assinatura-20261008/`. Builds anteriores permanecem históricos.

## Ensaios que ainda precisam ser concluídos

Os cenários destrutivos devem começar em uma **VM Windows x64 de laboratório**, com snapshot recuperável e perfil de QA. A primeira rodada autorizada no Windows Sandbox foi interrompida antes de executar qualquer instalador, conforme o registro abaixo. O próximo alvo precisa permitir aplicativos de desenvolvimento sem assinatura em sua configuração normal; não alterar a proteção do Sandbox ou do host para contornar o bloqueio. Logon/reboot e ensaios de rede exigem uma VM persistente e a definição dos caminhos de rede. A escolha de uma VM não autoriza alterações na rede do computador atual, reboot/logoff ou retomada de ensaios pausados.

| Caso | Procedimento no laboratório | Critério de conclusão |
| --- | --- | --- |
| Cancelamento | Abrir a prévia e cancelar na página inicial, antes de instalar | Saída de cancelamento e igualdade de arquivos, perfil, registro, atalho e rede; abrir o assistente não basta |
| Instalação limpa | Instalar a 0.3.5 em snapshot sem NetLane | Todos os hashes do manifesto iguais, versão/atalho corretos, UI sem UAC, serviço parado e perfil inicial sem regras importadas |
| Upgrade com dados | Instalar a 0.3.4, criar regras desabilitadas e preferências fictícias, fechar normalmente e atualizar | Perfil/backup iguais, payload 0.3.5 conferido e quatro avisos antigos removidos |
| Arquivos desconhecidos | Adicionar arquivos de QA com nomes próprios na raiz e em `notices`; atualizar e remover | Arquivos desconhecidos permanecem iguais; só itens compilados são removvidos |
| Downgrade | Na VM com 0.3.5, abrir a 0.3.4 | Recusa com código 6 e estado preservado; a segunda checagem exige cenário controlado adicional, sem concorrência de cópia real |
| Processos e segunda checagem | Usar a UI real, inclusive na bandeja e em outra sessão de usuário; abrir o instalador antes/depois dessa UI | Recusa com código 2, nenhum encerramento forçado ou alteração; testar também o desinstalador |
| Arquivo em uso | Manter um arquivo conhecido aberto exclusivamente e tentar instalar/remover | Recusa com código 7 e conteúdo preservado |
| Destinos inválidos | Em snapshots separados, substituir raiz/subpasta/arquivo/atalho por links ou tipos incompatíveis; testar `/D` e desinstalador movido | Recusa ou uso exclusivo do destino fixo; arquivos externos intactos |
| Falha de cópia e reparo | Induzir falha controlada somente na VM e verificar o estado parcial; retirar a condição e executar novamente | Código 7, perfil preservado e reparo completo com inventário conferido; rollback automático não está implementado nem será declarado aprovado por esse ensaio |
| Falha na remoção | Induzir erro em etapa conhecida, incluindo atalho/registro/autorun, em snapshot próprio | Código 7, aviso de operação incompleta e perfil preservado; recuperação/reparo acompanhado |
| Autorun e remoção | Marcar/desmarcar na UI instalada; remover com o comando esperado e repetir com comando de outro local | Registro coerente com a UI; entrada esperada removida e entrada estrangeira preservada; outros usuários preservados |
| Logon real | Reiniciar ou sair/entrar **somente na VM autorizada**, com autorun marcado | Uma UI na bandeja, painel restaurável, serviço e sondagem de qualidade sem início automático |
| UI instalada | Conferir ícones, teclado, redimensionamento e escalas disponíveis na VM | Controles acessíveis, diálogos legíveis e ausência de travamentos/erros; registrar escalas efetivamente usadas |

A falha de cópia hoje aborta sem restaurar automaticamente os arquivos substituídos. Recuperar o snapshot da VM não é prova de rollback do instalador. A recuperação por repetição/reparo precisa ser ensaiada; se não devolver o pacote a um estado consistente, permanece um defeito a corrigir antes da assinatura.

Para cada caso, guardar identificação da VM/snapshot, versão/hash do EXE, início/fim, código de saída, relato/captura dos passos manuais e estados antes/depois. UAC é confirmado manualmente. O auxiliar anteriormente bloqueado pelo antivírus permanece fora dos ensaios; não recriar, restaurar ou liberar esse arquivo.

Coleta de estado, **sem executar instalador ou alterar rede**:

```powershell
.\scripts\capture-installer-qa-state.ps1 -Label 'cancel-before'
# Executar somente o cenário autorizado, no alvo de laboratório definido.
.\scripts\capture-installer-qa-state.ps1 -Label 'cancel-after'
```

O coletor cria pastas novas em `artifacts/installer-qa/`, registra hashes do programa/perfil/atalhos, registro do produto, autorun do usuário atual, processos e configurações de rede. Não segue links na árvore de arquivos e mantém `FullInstallerQaVerified=false`. Comparações devem usar caminhos e hashes, sem exigir igualdade de horários, PIDs ou dados voláteis. Criar backup/snapshot antes dos casos que alteram o produto; uma lista de hashes não é backup.

## Primeira rodada bloqueada no Windows Sandbox

O Windows Sandbox já está habilitado no host. Configuração local: `artifacts/sandbox-qa-20261008/netlane-installer-qa.wsb`, com **3.072 MB**, rede desligada, entrada somente leitura contendo apenas os dois instaladores/manifestos e roteiro, e saída gravável limitada à pasta nova de recibos. Não compartilha o perfil, checkout ou instalação do host. [Opções oficiais do Sandbox](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file).

A configuração foi aberta após o usuário autorizar os quatro casos com “Vamos testar”. O roteiro `scripts/test-installer-sandbox.ps1` previa instalar a 0.3.4, atualizar para a 0.3.5, tentar downgrade e desinstalar verificando três fixtures preservadas e limpeza do autorun esperado. São quatro casos iniciais, sem abrir painel/serviço ou executar sondagem de rede. Fixtures incluem uma lista vazia de regras, não o cenário completo de preferências preenchidas. O roteiro recusa execução no host e exige usuário/modelo do Sandbox e token administrativo. A política de execução PowerShell `RemoteSigned` foi usada somente pelo processo no convidado; não alterou política persistente ou antivírus.

O convidado iniciou em **2026-10-08 às 21:52:40 UTC**. A tentativa de criar o processo do instalador 0.3.4 foi recusada às **21:52:42 UTC** com “Uma política de Controle de Aplicativo bloqueou este arquivo”. Não houve processo do instalador nem código de saída dele. Os eventos **3077 e 3033** de Integridade de Código registraram a exigência de assinatura; o XML do evento 3077 identifica `VerifiedAndReputableDesktop`, GUID `{0283ac0f-fff1-49ae-ada1-8a933130cad6}`, status `0xc0e90002`. O SHA-256 flat registrado corresponde ao pacote 0.3.4 original, `4468E105CCAEB4EC09FCA23A614E158183476B5ED32AED12642AB8390F88604B`. [Interpretação oficial dos eventos de Controle de Aplicativo](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/operations/event-id-explanations).

**Resultado: 0 de 4 casos concluídos; 0.3.5 não tentada; `FullInstallerQaVerified=false`.** A inspeção somente leitura confirmou ausência de programa, perfil, registro e processos NetLane no convidado. A consulta de inventário de políticas pelo CiTool falhou com `-2147009597`; o nome da política acima veio do evento, sem alegar inventário completo. Não houve nova tentativa, liberação, alteração da proteção, build ou assinatura para contornar o bloqueio. O Sandbox foi encerrado normalmente, com inventário posterior vazio.

Evidências preservadas em `artifacts/sandbox-qa-20261008/`: `output/result.json` é o recibo original de falha; `output/block-diagnostics.json` contém eventos/XML e estado do convidado; `runtime-review.json` consolida a comparação do host. Os **13 conjuntos** de programa/perfil/menu/registro/autorun e configurações de rede permaneceram iguais entre os estados `sandbox-before` e `sandbox-after-stop`; **8 arquivos protegidos** mantiveram seus hashes. A versão instalada do host continua **0.3.4**, sem processos NetLane, e o Defender permanece com antivírus, proteção em tempo real e monitoramento de comportamento ativos. O estado de proteção do host não é usado para inferir o estado da política no convidado.

Não reabrir esta configuração para repetir a execução bloqueada. Os quatro casos e os demais cenários da matriz continuam pendentes; um futuro resultado positivo desses quatro casos ainda não fechará o QA completo.

## Alvo de laboratório informado — 192.168.184.128

O usuário autorizou os mesmos quatro casos na VM `192.168.184.128`. O IP foi associado à VM local **Win11 - Work**, `C:\VMs\Win11 - Work\Win11 - Work.vmx`, pelo MAC coincidente `00:0c:29:2f:4a:d0`. RDP respondeu; SSH, WinRM e SMB não responderam nas portas habituais. Isso comprova disponibilidade de acesso, não a versão efetiva do Windows, privilégio administrativo ou compatibilidade com o instalador.

O `vmrun` local reconhece a VM em execução, mas recusou abrir seu estado com **“A password is required for this operation”**. A senha de criptografia exigida pelo VMware não foi informada; a senha de login do Windows não foi reutilizada por inferência. Não houve autenticação no convidado, transferência de arquivos, criação de snapshot ou execução de instaladores. Registro em `artifacts/vm-qa-20261008-184128/access-review.json`; nenhum recibo ou roteiro contém senha.

Roteiro `scripts/test-installer-lab.ps1` preparado, com sintaxe conferida e recusa no host verificada. Exige plano novo com computador, SID, UUID, IP e snapshot confirmado; recusa instalação/perfil/atalhos/autorun anteriores do NetLane. Confere os hashes dos dois pacotes e manifestos, payload por etapa, preservação de três fixtures, quatro avisos antigos removidos, downgrade sem mudança de estado, remoção do autorun esperado e preservação das outras entradas do usuário. Aguarda também a remoção real dos arquivos após encerrar o lançador do desinstalador e registra estados antes/depois da rede, sem reconfigurá-la.

Próximo: obter o acesso exigido pelo VMware, verificar o convidado somente em leitura e criar/conferir snapshot antes de alterar o produto. Proteções e dados existentes serão preservados; um impedimento de acesso ou um novo bloqueio interrompe a execução. Logon real, rede, demais casos da matriz e assinatura continuam etapas separadas. Os quatro casos nesta VM ainda não foram executados.

## Conexão padrão e tráfego por aplicativo

Permanecem pendentes a restauração real da conexão padrão, sua persistência após reboot e a convivência com roteamento por aplicativo. O ensaio exige alvo com dois caminhos de rede utilizáveis e autorização específica para cada alteração. Recolher baseline novo de GUIDs, rotas/métricas, estado ativo/persistido, bindings, flags e recuperação; não reaproveitar índices ou prioridades dos registros de setembro.

Usar um executável de prova e destino explícitos, com conexões novas e dados sintéticos. Registrar separadamente política aceita, tráfego observado e interface de saída. Ao terminar, remover apenas políticas/opções do ensaio, verificar serviço parado e comparar com o baseline. Uma transferência sustentada controlada também permanece sem comprovação; não reiniciar OneDrive/Steam ou usar arquivos reais por inferência.

Os ciclos históricos de perda/retorno da Wi-Fi, suspensão/retomada, QUIC/IPv4 concorrente e continuidade na bandeja estão concluídos nos seus alvos registrados. IPv6 e VPN continuam limites sem validação neste ambiente; não habilitar bindings ou ampliar esses ensaios automaticamente. [Recuperação](testes-recuperacao.md), [conexão padrão](conexao-padrao.md) e [limites dos protocolos](ampliacao-ensaios-rede.md) mantêm os resultados e os escopos específicos.

## Rodada na VM de laboratório — 2026-10-11

Alvo `Win11 - Work` (`DESKTOP-DC6M247`, VMware, UUID `3F1D4D56-…-12D6082F4AD0`,
IPv4 `192.168.184.128`), Smart App Control desligado, snapshot de retorno `PC-OK`;
acesso por OpenSSH com chave dedicada e sessão administrativa. Os pacotes 0.3.4
(`4468E105…604B`) e 0.3.5 (`020EE3A9…B281`) e os manifestos foram conferidos por
hash no host e no convidado. Não houve logon real, abertura de UI/serviço, teste
de rede nem alteração de proteção. O `test-installer-lab.ps1` passou a guardar o
handle do processo logo após iniciar: sem isso, recusas rápidas (downgrade, código
6) podem chegar com código de saída vazio no PowerShell.

| Caso | Resultado |
| --- | --- |
| Instalação limpa 0.3.4 | retorno 0; payload por hash e atalho conferidos |
| Upgrade 0.3.4 → 0.3.5 com dados | retorno 0; perfil (`netlane-rules.json`) e arquivos desconhecidos na raiz e em `notices` preservados; quatro avisos `System.Text.Json`/`Encodings.Web` 8.0.0 removidos |
| Downgrade 0.3.5 → 0.3.4 | **recusado com código 6**; registro, autorun, programa, perfil e menu Iniciar sem alteração |
| Remoção | retorno 0; payload removido; fixtures preservadas; autorun próprio removido e demais entradas Run preservadas |
| Rede | adaptadores, bindings, métricas, rotas, DNS e flags globais iguais antes/depois |

Evidências: `artifacts/dotnet-lab-20261011-113752` (pacote, inventário, recibos e
estados `lab-before` → `lab-after`). Continuam pendentes: cancelamento, processos
e segunda checagem, arquivo em uso, destinos inválidos, falha de cópia e reparo,
falha na remoção, autorun pela UI, logon real e UI instalada. A VM mantém, por
desenho, as fixtures preservadas em `C:\Program Files\NetLane` e no perfil.

## Matriz sem interface na VM — 2026-10-11

Mesmo alvo (`DESKTOP-DC6M247`), agora pela conta administrativa `termiax` via OpenSSH.
Por decisão do usuário, em vez de reverter ao `PC-OK` foram apagadas apenas as
fixtures da rodada anterior (hashes em `C:\NetLaneDotnetQa\cleanup-20261011\removed-fixtures.json`
no convidado). Roteiro novo `scripts/test-installer-lab-matrix.ps1`, com os mesmos
pacotes 0.3.4/0.3.5 conferidos por hash. O desinstalador roda copiado para fora de
`$INSTDIR` com `_?=`, o mesmo mecanismo do lançador NSIS, preservando o código real de saída.

**Armadilha:** a sessão OpenSSH de administrador sai com `SeBackupPrivilege`,
`SeRestorePrivilege` e `SeTakeOwnershipPrivilege` **ativados**, o que ignora ACEs de
negação. A primeira execução (`run1-acl-bypassed`) removeu tudo com código 0 em vez de
falhar. O roteiro agora desativa esses privilégios no próprio token, como num PowerShell
elevado pelo UAC, e prova que a negação bloqueia uma exclusão antes de induzir falhas.

| Caso | Resultado |
| --- | --- |
| Destino limpo é junção / pasta sem marcador | recusa **4**; arquivos externos e pasta intactos; sem registro |
| `/D=` apontando outro destino | retorno 0 só em Program Files; destino alternativo não criado; 757 arquivos conferidos |
| Processos (prova `NetLane.QaProbe.exe`, cópia do `PING.EXE`) | instalador e desinstalador recusam com **2**; processo sobrevive; estado igual |
| Arquivo em uso (`NetLane.Core.dll` exclusivo) | instalador e desinstalador recusam com **7**; estado igual |
| `notices` como junção, DLL virando pasta ou symlink, atalho virando pasta, menu Iniciar como junção | instalador e desinstalador recusam com **4**; arquivos externos e estado iguais |
| Desinstalador movido | recusa **4**; nada removido |
| Falha ao apagar o atalho | **7**; payload removido, mas marcador, `Uninstall.exe`, registro e perfil preservados; repetição retorna 0 e remove tudo |
| Falha ao apagar o registro | **7**; marcador e `Uninstall.exe` preservados; repetição retorna 0 |
| Falha de cópia (criação negada em `notices`), 0.3.4 → 0.3.5 | **7**; estado parcial com 749 arquivos 0.3.5, 5 da 0.3.4, 2 ausentes e registro ainda 0.3.4; repetição da 0.3.5 retorna 0, 757 arquivos conferidos e avisos antigos removidos |
| Rede | adaptadores, bindings, métricas, rotas, DNS e flags iguais antes/depois |

A VM terminou sem instalação, perfil, atalho, autorun ou ACE de negação. Evidências em
`artifacts/dotnet-matrix-20261011` (pacote, recibos, estados e a execução anulada).

Limites: a guarda de processo usou um executável de prova, não a UI real na bandeja ou em
outra sessão. A segunda checagem dentro da Section não foi isolada; no modo silencioso ela
repete a de `.onInit`. O reparo depende de executar de novo: a falha de cópia deixa
binários de versões misturadas com registro 0.3.4, e **não há rollback automático**.
Continuam pendentes: cancelamento na página inicial, guarda com UI real, autorun pela UI,
logon real e UI instalada.

## Casos interativos na VM — 2026-10-11

Executados pelo usuário via RDP na sessão do `pepg` (administrador), com conferência por
SSH a cada passo. Mesmo pacote 0.3.5 (`020EE3A9…B281`).

| Caso | Resultado |
| --- | --- |
| Cancelamento na página inicial | saída **1** (cancelamento NSIS), duas vezes; programa, registro, atalho, perfil e autorun inalterados |
| Instalação pelo assistente | saída 0; 756 arquivos conferidos por hash; atalho presente |
| Abertura pelo menu Iniciar | sem UAC; processo como `pepg`, sessão interativa; vai para a bandeja; nenhum serviço iniciado |
| Instalador com a UI aberta | recusa com **7** (arquivo em uso), não 2 — ver ressalva; estado e processo preservados |
| Desinstalador com a UI aberta | recusa (o lançador retorna 0; estado intacto confirma); processo não encerrado |
| Autorun marcado pela UI | entrada exata `"C:\Program Files\NetLane\NetLane.UI.exe" --startup`; outras entradas intactas |
| Logon real (sair/entrar) | uma instância na bandeja e painel restaurável, sem UAC nem erro; nova execução manual com `--startup` sai com 0 por instância única |
| Autorun desmarcado pela UI | só a entrada `NetLane` removida |
| UI instalada | ícones, teclado (Tab/Espaço/Enter), redimensionamento e fechar para a bandeja ok a **100%**; 125%/150% não testados |
| Remoção por Configurações → Aplicativos | programa, registro, atalho e autorun removidos; rede igual à `matrix-after` |

**Ressalva — ordem das guardas:** com a UI aberta, `RequireSafeDestination` (abre os arquivos
do payload com compartilhamento 0) roda antes de `RequireNetLaneClosed`, então a recusa sai
como 7 ("arquivos em uso") em vez de 2 ("feche o NetLane"). É seguro e a mensagem também
pede para fechar o painel, mas difere do critério da matriz. Corrigir exige inverter as
chamadas em `.onInit`/`un.onInit`/Section, recompilar e repetir o QA do pacote.

**Observações:** no logon o Windows executa as entradas `Run` em série, esperando até 30 s
cada (OneDrive, Lists e Teams vêm antes), então o NetLane apareceu cerca de 2 minutos após
o logon (log `Shell-Core/Operational`, eventos 9707/9708). O ícone do .NET é o `netlane.ico`
anterior; a identidade nova de 10/10 está só na versão Rust (`assets/brand/README.md`).

Com isso, todos os cenários da matriz do instalador .NET foram exercitados. Decisões abertas
antes de assinar a 0.3.5: aceitar ou corrigir a ordem das guardas, aceitar o reparo por
nova execução sem rollback automático e manter ou trocar o ícone antigo. Qualquer mudança
no pacote exige nova rodada de QA.

## Pacote 0.3.6 assinado — 2026-10-12

Decisões do usuário: corrigir a ordem das guardas, aceitar o reparo por nova execução
(sem rollback automático) e usar o logo novo. Mudanças:
- `NetLane.nsi`: `RequireNetLaneClosed` passa a rodar antes de `RequireSafeDestination` em
  `.onInit`, Section, `un.onInit` e Uninstall;
- `src/NetLane.UI/Assets/netlane.ico` substituído por `assets/brand/netlane-diagnostics.ico`;
- versão padrão do build: 0.3.6.

Build Release sem avisos; 288 testes aprovados.

Pacote em `artifacts/installer/signed-20261012-036-b/NetLane-0.3.6-preview-win-x64-setup.exe`,
SHA-256 `6835D5BC43F097D40FB83C2A5B20F531CAC71AE150B6D36D8FAF22D11D5E083E`, revisão
`df9eca7` com as mudanças acima ainda não commitadas. A primeira tentativa
(`signed-20261012-036`) falhou com o SimplySign desconectado. Depois do login, o verificador
estático passou: 757 arquivos, **10 assinaturas válidas com timestamp Certum** e ícone do
`NetLane.UI.exe` igual ao logo novo, sem diferença de pixels.

QA na VM `DESKTOP-DC6M247`, com evidências em `artifacts/dotnet-qa-036-20261012`:

| Rodada | Resultado |
| --- | --- |
| `test-installer-lab.ps1` (anterior 0.3.5) | instalação limpa 0.3.5, upgrade para 0.3.6 com fixtures, downgrade recusado com **6** e remoção preservando fixtures/autorun alheio. Os nomes dos casos no recibo são fixos no roteiro ("0.3.4/0.3.5"); os pacotes executados são os do plano |
| `test-installer-lab-matrix.ps1` (anterior 0.3.4) | os mesmos 14 casos da 0.3.5 aprovados, inclusive falha de cópia 0.3.4 → 0.3.6 com reparo; rede igual |
| Interativo (RDP, `pepg`) | instalação 0; ícone novo na janela, barra e bandeja; instalador com a UI aberta recusa com **2** e a mensagem "Feche o NetLane…"; remoção por Configurações limpa; rede igual |

O PowerShell da rodada interativa já estava elevado, então o UAC não apareceu. O editor
verificado foi conferido pela assinatura no host, não pela tela do UAC. Fixtures do roteiro
de 4 casos foram registradas em `removed-fixtures.json` e apagadas antes da matriz. A VM
terminou sem NetLane.

## Assinatura depois dos ensaios

Somente após fechar os cenários anteriores e eventuais defeitos encontrados, retomar o diagnóstico do SimplySign e gerar o pacote final em pasta nova. A presença do certificado e da cadeia válida não resolveu a recusa do SignTool registrada em 08/10. [Procedimento SimplySign](assinatura-simplysign.md).

Conferir as dez assinaturas/timestamps e os hashes após assinar. A assinatura não substitui o QA anterior. Distribuição, commit/push e contato com suporte permanecem ações com escopo próprio; nenhum desses passos foi executado nesta revisão.
