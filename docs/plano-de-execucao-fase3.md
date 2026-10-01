# Fase 3 — Interface de regras

> **Marco de 2026-10-01 — 0.3.4:** ícones conferidos pelo usuário e entrada NetLane.UI Ativado confirmada por captura de Aplicativos → Inicialização; comando esperado do Run consultado localmente. [Confirmações e limites](inicializacao-e-icones.md#confirmações-do-usuário--2026-10-01): logon real e limpeza do autorun pelo desinstalador ainda pendentes. Usuário adia reboot e solicita ai-memory, commit e push das fontes/testes/documentação. Continuidade default/netlane, notes/netlane-0-3-4-instalado-2026-10-01.md; hash/sincronização devem ser conferidos no Git. Registros abaixo são históricos.

> **Prévia 0.3.4 instalada em 2026-09-30 às 23:53:** [upgrade sobre a 0.3.3](instalador-local.md) aprovado, retorno 0, StageVerified=true, inventário/atalho/ícone corretos. Perfil de cinco arquivos, rede e autostart preservados; opção de inicialização permanece desativada. [Ícones e inicialização opcional](inicializacao-e-icones.md) aguardam QA pela UI instalada e logon real. Código local, sem novo commit/push. Registros abaixo são históricos.

> **Prévia 0.3.4 preparada em 2026-09-30:** [ícones dos executáveis em Regras/Conexões e inicialização opcional com o Windows](inicializacao-e-icones.md) implementados, com 283 testes de produção aprovados e layout final revisado. Pacote validado estaticamente; 0.3.3 segue instalada, aguardando atualização e QA instalado/logon. Alterações locais após 7c27738, sem novo commit/push. Registros abaixo são históricos.

> **Teste da 0.3.2 confirmado em 2026-09-30 às 21:25:** usuário respondeu **“testado, tudo certo”** após X → bandeja/reabrir pelo ícone/Sair. [Confirmação registrada como relato](bandeja-windows.md#confirmação-do-usuário--2026-09-30), com perfil/interfaces iguais ao upgrade e UI instalada aberta/respondendo na leitura seguinte, sem serviço. Rodada de instalação/correções pronta para encerrar; restauração/reboot da conexão padrão, tráfego por app, QA restante do instalador e publicação ainda separados. Sem alteração de rede ou commit/push. Registros abaixo são históricos.

> **0.3.2 instalada em 2026-09-30 às 21:19:** [upgrade autorizado sobre a 0.3.1](instalador-local.md#atualização-para-a-032--2026-09-30), UAC manual, retorno 0 e `StageVerified=true`. Inventário de 758 hashes, registro/atalho e logo da UI instalada conferidos. Quatro arquivos do perfil, rede/autostart e 19 arquivos protegidos iguais; Defender ativo e nenhum processo NetLane restante. Conferência visual de menu Iniciar/X/bandeja ainda pendente, assim como restauração/reboot da conexão padrão e tráfego por app. Wi-Fi segue prioritária; sem mudança de rede, serviço ou commit/push. Registros abaixo são históricos.

> **Correções locais em 2026-09-30:** [X oculta na bandeja e ícone do aplicativo corrigido na prévia 0.3.2](bandeja-windows.md#ajuste-do-x--2026-09-30), mantendo sessão e edições. Sair preserva o encerramento seguro; falha da bandeja mantém saída pelo X. **245 testes do fechamento aprovados**, build final com ícone e pacote v2 validados estaticamente, ainda não executado. Usar `preview-20260930-close-to-tray-v2`; primeiro pacote preservado como anterior ao ícone. 0.3.1 segue instalada; atualização aguardando confirmação. Sem alteração de rede, serviço ou commit/push. Registros abaixo são históricos.

> **Troca real exercitada pelo usuário em 2026-09-30:** dois `tracert` concluídos até 8.8.8.8, primeiros saltos distintos. [Estado lido às 18:49](conexao-padrao.md#troca-real-exercitada-pelo-usuário--2026-09-30): Wi-Fi 5 / Ethernet 55 em manual, Wi-Fi prioritária, sem processos NetLane abertos, recuperação persistida e regras/IPv6 preservados. Restauração, reboot e tráfego real por app ainda não verificados. Agente apenas leu o estado; sem alteração de rede, commit/push ou distribuição. Registros abaixo são históricos.

> **Instalação concluída em 2026-09-30 às 18:19:** [0.3.0 removida e prévia 0.3.1 instalada](instalador-local.md#remoção-da-030-e-instalação-da-031--2026-09-30) após autorização específica, com UAC manual. Perfil/backup preservados, 758 hashes do inventário iguais, registro/atalho corretos, rede/autostart iguais e Defender ativo. A nova escolha de conexão padrão está no pacote instalado; abertura da UI e troca/restauração real ainda não ensaiadas. Sem processo NetLane restante ou commit/push. Registros abaixo são históricos.

> **Entrega local em 2026-09-30:** [conexão padrão IPv4 escolhida entre cabo e Wi-Fi](conexao-padrao.md), com UAC, persistência e restauração, implementada para a prévia 0.3.1 após o pedido do usuário. Testes sintéticos, canal entre processos, leitura real e layout validados. Instalação dessa versão e troca real ainda pendentes; rede atual preservada no desenvolvimento, sem serviço real, IPv6 ou publicação.

> **Desinstalação/reinstalação concluída em 2026-09-30 às 16:56:** [prévia 0.3.0 removida e reinstalada após autorização específica](instalador-local.md#desinstalação-e-reinstalação-preservando-o-perfil--2026-09-30), com UAC manual. `StageVerified=true` nas duas etapas; perfil e backup iguais, 757 hashes do inventário reinstalado conferidos, atalho/registro corretos, Defender ativo e campos de rede iguais. UI reinstalada abriu sem elevação; fechamento final em conferência. QA completo, cancelamento, outras guardas e atualização entre versões ainda pendentes. Sem serviço/roteamento ou commit/push; registros abaixo são históricos.

> **Persistência concluída em 2026-09-30:** [regra temporária salva, recarregada e removida pela UI instalada](instalador-local.md#persistência-no-perfil--2026-09-30), `curl.exe`, Automático/desabilitada. Lista principal vazia e backup idêntico à regra de QA anterior. Programa/checkout preservados, Defender ativo e fechamento normal **0**. Métrica automática da Wi-Fi variou **40 → 35** no salvamento; opções de rota/IPv6 iguais e recibo original mantém comparação integral da rede false. Na remoção da regra, rede conferida igual. `UiPersistenceCycleVerified=true`, QA completo do instalador ainda false. Próximo cenário proposto: desinstalação/reinstalação preservando o perfil, ainda sem autorização/execução. Sem serviço/roteamento, alteração de métrica ou commit/push. Registros abaixo são anteriores.

> **QA retomado em 2026-09-30:** [recusa inicial com painel aberto](instalador-local.md#recusa-inicial-com-painel-real-aberto--2026-09-30) **2**, painel ainda vivo e fechamento normal posterior **0**; confirmação visual do aviso pendente. Usuário confirmou que [concluiu a instalação da prévia 0.3.0](instalador-local.md#instalação-observada-na-abertura-seguinte--2026-09-30): instalador **0**, atalho/registro presentes e 757 arquivos inventariados íntegros. Cancelamento não exercitado. [UI instalada abriu pelo atalho sem elevação e fechou normalmente](instalador-local.md#abertura-da-ui-instalada-pelo-atalho--2026-09-30), retorno **0**. Defender ativo, perfil ausente, rede/regras/recibos preservados e nenhum processo NetLane ou autostart encontrado. Lista vazia/serviço parado aguardam confirmação visual. Usuário adiou suporte; auxiliar bloqueado não foi restaurado/recriado e instalador não foi recompilado. Persistência, outras guardas, atualização e remoção seguem pendentes. Sem commit/push; entradas abaixo são históricas.

> **Atualização em 2026-09-14 às 18:25 (America/Cuiaba):** [identificada a detecção no log local da McAfee](instalador-local.md#identificação-da-detecção-no-log-da-mcafee--2026-09-14): `ti!B50CBFE4C7E2`, resultado `infection quarantined`, com nome/hash correspondentes ao auxiliar. A captura antes solicitada deixou de ser necessária para essa identificação; o próximo passo é encaminhar o registro ao fornecedor. Rascunho local atualizado, sem envio. Não há parecer sobre esta amostra nem nova execução, restauração/recriação, instalação, alteração de proteção ou commit/push. Guardas integradas e QA de instalação seguem pendentes.

> **QA interrompido em 2026-09-14 às 17:18 (America/Cuiaba):** o [novo auxiliar interativo foi bloqueado antes de iniciar](instalador-local.md#bloqueio-do-auxiliar-interativo--2026-09-14). A autorização era para testar a recusa do instalador com UAC manual, sem instalar; o bloqueio ocorreu antes desse ponto, com zero tentativas de abrir o instalador. Arquivos/estado preservados e nenhum processo restante. Auxiliar indisponível no caminho, sem restauração/recriação ou repetição. Próximo passo: obter os detalhes da nova detecção, sem alterar a proteção. O teste isolado anterior continua aprovado; guardas integradas, instalação e demais cenários continuam pendentes. Registros locais, sem novo commit/push após `c0c3747`.

> **Validação em 2026-09-14 às 12:50 (America/Cuiaba):** o [detector separado passou com processo fictício presente e depois encerrado](instalador-local.md#detecção-isolada-com-processo-presente-e-encerrado--2026-09-14): **2 → 0**, com uma tentativa por cenário, sem elevação e sem recompilar o verificador. A fixture encerrou sozinha após cerca de 15 segundos. Proteção reportada saudável, política de scripts inalterada, hashes preservados e nenhum processo restante. Próxima etapa sujeita a autorização: QA das guardas dentro do instalador, antes da instalação real. Outras sessões e demais guardas não foram validadas. Sem painel, serviço, instalação, alteração de proteção ou novo commit/push na rodada de teste; não presumir liberação da McAfee para novos hashes. O registro anterior foi publicado em `86947f8`; esta atualização versiona somente o resumo documental, autorizado posteriormente, mantendo os artefatos locais e sem repetir testes runtime.

> **Diagnóstico anterior em 2026-09-14:** [inspeção estática](instalador-local.md#diagnóstico-do-código-3--2026-09-14) confirmou leitura tardia do erro na amostra antiga, causa provável do código 3. A captura imediata já existia no código atual, sem alteração de fonte nesta investigação. A consulta complementar x86 foi bloqueada pela política de scripts do PowerShell, sem alteração dessa política. O verificador separado foi autorizado e testado posteriormente, conforme o resultado acima; a amostra original permanece intacta.

> **Reteste em 2026-09-14:** resposta da McAfee apresentada pelo usuário informa inclusão do ZIP na lista de permitidos. Um [reteste autorizado do verificador original](instalador-local.md#resposta-da-mcafee-e-reteste-original--2026-09-14) iniciou sem bloqueio, mas retornou **código 3**, sem concluir a checagem de processos. A validação funcional permaneceu pendente, dando origem ao diagnóstico acima. Proteção reportada saudável, hashes preservados, sem processos restantes. Sem recompilação, instalação, serviço, alteração de proteção ou novo commit/push. A resposta não aprova o instalador completo nem futuras compilações.

> **Histórico de 2026-09-13:** [preparação do instalador local](instalador-local.md) implementada; 336/336 testes locais aprovados. Pacote x64 gerado, sem instalar/iniciar serviço. O usuário informou envio da amostra à McAfee, sem protocolo, e o parecer ainda estava pendente naquele momento. Commit/push da preparação concluído em `16a0c49`, sem incluir binários ou alterações de dados locais. Assinatura, distribuição, VPN e ensaios elevados continuaram separados. Os registros abaixo são históricos.

> Concluído em **2026-09-12 às 07:56:33**: [ensaio com painel na bandeja](ensaio-bandeja-roteamento.md#resultado-completo-da-rodada-de-30-minutos), seis fases e verificação após fechamento aprovadas. Novas conexões QUIC/IPv4 do probe pela Wi-Fi com janela visível, oculta e restaurada; retorno à Ethernet após parada normal. Mesma janela/serviço, rede/regras preservadas e nenhum processo restante. Início/UAC e cliques manuais. Limite de 30 minutos; serviço encerrado após cerca de 9 min 31 s. Próxima etapa: preparação do instalador local, ainda não iniciada; instalar, assinar ou publicar são decisões separadas.

> Histórico deste ciclo: primeira rodada parcial por limite de cinco minutos, seguida de nova decisão explícita do usuário para 30 minutos e uma preparação recusada antes de abrir a janela. Recibos/binários anteriores preservados; nenhum resultado antigo completou a nova rodada. Build final v2 sem avisos/erros, 37 + 75 testes reexecutados; os 216 testes de produção passaram no build anterior de 30 minutos, antes dos recibos adicionais de diagnóstico. Sem alteração de código de produção neste ciclo, commit ou push.

> Histórico de 2026-09-11: interface e bandeja revisadas no escopo básico; [QUIC/IPv4 sequencial](ensaio-quic-ipv4.md#resultado-elevado-real-e-conferência-final) e [concorrência de dois executáveis isolados](ensaio-quic-concorrente.md) concluídos com autorização/UAC manual, rotas distintas/invertidas e limpeza confirmada. O ensaio concorrente passou 61 verificações independentes. Naquele ponto, o próximo cenário era continuidade com o painel minimizado, agora concluído acima. O [ponto salvo em 2026-09-10](continuidade-2026-09-10.md) preserva o histórico da pausa.

Estado atualizado em 2026-09-09: editor e [interface renovada](interface-desktop.md) entregues; [controle de sessão pelo painel](controle-servico.md) implementado localmente, com início elevado e parada/restauração reais validados. O [ensaio do OneDrive](teste-onedrive-wifi.md#conclusão-e-parada-normal) teve download em progresso, conclusão confirmada pelo cliente/usuário e conexões TCP/IPv4 observadas na Wi-Fi. A [interface observada por processo](conexoes-por-processo.md) já foi implementada, separada das regras e da confirmação do serviço. A [perda e o retorno da Wi-Fi](testes-recuperacao.md#perda-e-retorno-da-wi-fi--concluído-com-limpeza-final) foram ensaiados, com retirada/reaplicação da política, nova conexão TCP/IPv4 na Wi-Fi e limpeza final confirmada; a variação anterior da métrica automática ficou registrada. A revisão visual básica de Conexões foi concluída antes/depois da suspensão. O [ensaio de suspensão/retomada](testes-recuperacao.md#conclusão-da-suspensãoretomada-e-limpeza-final) confirmou recuperação na mesma sessão, novas conexões TCP/IPv4 do OneDrive na Wi-Fi e parada normal com restauração final, totalizando 28 verificações aprovadas. IPv6 e UDP/QUIC do OneDrive continuam pendentes. Steam permanece fora dos próximos testes. Os resultados e contagens abaixo registram as entregas históricas, não o total atual da suíte.

## Painel de consumo e seleção de interfaces

Entrega adicional solicitada pelo usuário:

- Aba inicial **Consumo das interfaces** com leitura de download/upload em Mbps, gráfico dos últimos 60 segundos e volume recebido, enviado e total por interface.
- Seleção por checkbox em **Selecionar interfaces**, com Ethernet e Wi-Fi selecionáveis de forma independente. A seleção inicial prioriza interfaces conectadas com gateway; as demais podem ser incluídas manualmente.
- Preferências por GUID em `%LOCALAPPDATA%\NetLane\interface-selection.json`, preservando uma seleção vazia e interfaces escolhidas que estejam temporariamente ausentes.
- A seleção filtra o painel e as opções de novas escolhas na UI. Não habilita/desabilita adaptadores no Windows, não modifica políticas salvas e não muda o comportamento do serviço por si só.
- Regras que já usam uma placa desmarcada continuam visíveis com a indicação “fora da seleção (regra existente)”.
- Coleta a cada 2 segundos em segundo plano; apenas mudanças na lista/estado das placas ou na seleção reconstroem as opções das regras. A atualização de consumo preserva edições pendentes e seletores abertos.
- Velocidade calculada com tempo monotônico e diferença entre contadores; desconexão, falha de leitura e queda dos contadores iniciam nova referência de cálculo, preservando totais já medidos.
- A primeira amostra estabelece a referência. Totais incluem todo o tráfego da interface observado na sessão, inclusive rede local. Não representam o histórico do Windows, consumo mensal ou consumo de um aplicativo isolado.
- **Reiniciar medição** afeta somente a interface exibida. Fechar a UI encerra a medição; preferências permanecem, mas volumes e gráfico não são persistidos.

Validação adicional: testes de cálculo com intervalos irregulares, contador zerado, ociosidade, desconexão/falha, histórico limitado, seleção persistida e vazia, atualização sem perder edições e controles WPF. O smoke test nativo leu 11 interfaces neste computador e confirmou contadores disponíveis para Ethernet e Wi-Fi, em modo somente leitura. A tela foi renderizada com dados sintéticos para revisão visual.

Fonte dos contadores: [IPInterfaceStatistics do .NET](https://learn.microsoft.com/en-us/dotnet/api/system.net.networkinformation.ipinterfacestatistics). Volumes são exibidos em unidades decimais (KB/MB/GB) e taxas em megabits por segundo.

## Entrega deste ciclo

- Visualização das placas Ethernet/Wi-Fi, IP IPv4 e estado conectado/desconectado, com atualização automática e manual.
- Seleção única de conexão por aplicativo: Automático ou um adaptador identificado por GUID. Modo e identificador são gravados juntos.
- Adição por seletor de executável, remoção, habilitação/desabilitação, pesquisa por nome/caminho e filtro de habilitação.
- Placas desconectadas ou ausentes continuam identificadas na escolha existente; atualizar as conexões preserva edições pendentes.
- Confirmação antes de descartar alterações ao fechar ou recarregar.
- Modelo `RoutingPolicy` compartilhado entre UI e serviço, incluindo preservação de fallback, hints e propriedades JSON desconhecidas.
- Arquivo vazio representa nenhuma regra. Arquivo ausente começa vazio; erro de leitura bloqueia a edição e não gera regras de exemplo.
- Salvamento por arquivo temporário no mesmo diretório e substituição com backup `.bak`. A versão lida é comparada antes de salvar; janelas do editor compartilham um lock de escrita. Um editor externo que ignore esse lock ainda pode concorrer após a última comparação.

O arquivo padrão do checkout continua sendo `src/NetLane.Service/netlane-rules.json`. As duas regras locais existentes não foram alteradas durante este ciclo. O backup contém a versão anterior ao último salvamento bem-sucedido; não é um histórico completo.

## Integração com o serviço

- GUIDs com/sem chaves e diferenças de maiúsculas são equivalentes. Isso corrige a comparação entre o JSON existente e os identificadores retornados pelo Windows.
- Uma placa explicitamente escolhida precisa estar conectada e ser compatível com o modo. Caso contrário, a regra anterior é removida no ciclo e o Windows reassume a escolha. A configuração continua gravada para o retorno da placa.
- Regras antigas sem GUID mantêm seleção por tipo indicado no `RouteMode`.
- Um caminho absoluto existente pode receber regra mesmo sem o aplicativo aberto. Um caminho explícito ausente não é substituído por outro executável com o mesmo nome.
- Alterar o caminho do executável também exige reaplicação da regra.
- Automático, desabilitação e remoção eliminam a regra aplicada anteriormente.
- O JSON passa a ser a fonte de regras após o bootstrap. Uma lista `[]` não recupera regras de `appsettings`.
- O motor ainda mantém uma regra por nome de executável. A UI impede adicionar dois executáveis com o mesmo nome, mesmo de pastas distintas.

## Validação realizada

- `dotnet build NetLane.sln --configuration Release`: 0 erros, 0 avisos.
- `dotnet test NetLane.sln --configuration Release --no-build`: 58 testes aprovados, incluindo o painel de consumo e seleção.
- Persistência: backup, exclusão da última regra, JSON inválido, GUID inválido, duplicidade, BOM UTF-8, campos adicionais, arquivo bloqueado e alteração externa.
- Seleção: GUID formatado de formas diferentes, placa ausente/desconectada, incompatibilidade de tipo e seleção legada.
- UI → arquivo → worker: executável fora do catálogo, mudança de caminho, retorno a Automático, desabilitação e retirada da placa.
- WPF em thread STA: carregamento do XAML, ligação dos seletores, pesquisa, salvamento das linhas ocultas e ausência de avisos de binding. Renderização em memória revisada.

Os testes de comportamento usam arquivos descartáveis e adaptadores/motor simulados; o smoke test `WindowsCounters` faz leitura dos contadores locais. Não foi iniciado um serviço elevado nem aplicada uma política real à rede nesta execução. O teste em memória não substitui a validação interativa de diálogos, DPI e operação prolongada.

Render opcional durante o teste WPF:

```powershell
$env:NETLANE_TEST_RENDER_PATH = 'C:\Codes\netlane\artifacts\tests\policy-editor.png'
dotnet test tests\NetLane.Tests --filter FullyQualifiedName~MainWindowTests
```

## Próximas entregas

Ordem acordada para este ciclo, sem Microsoft Store:

1. Controle seguro de iniciar/parar/reiniciar pela UI — código e testes sintéticos implementados; ciclo real completo com UAC manual, troca de PID no reinício e parada/restauração final validados.
2. Sincronização do OneDrive — concluída neste ensaio, conforme status do cliente e confirmação do usuário; novas conexões TCP/IPv4 observadas na Wi-Fi, com coleta por amostras.
3. [Conexão/interface observada por processo](conexoes-por-processo.md) — implementada com coleta TCP/IPv4 somente leitura, separada da configuração e da política aceita; suíte ampliada para 190 testes.
4. [Ensaios de recuperação](testes-recuperacao.md): início/reinício/parada real concluído, com oito verificações finais aprovadas e 43 testes sintéticos de preparação. Perda/retorno da Wi-Fi concluído em 2026-09-09, com nova conexão TCP/IPv4 do OneDrive na Wi-Fi, limpeza final e 18 verificações aprovadas; diferença anterior da métrica automática registrada separadamente. **Suspensão/retomada concluída**, com recuperação na mesma sessão, Conexões conferida antes/depois, parada/restauração final e 28 verificações aprovadas (16 de recuperação + 12 de conferência/encerramento). Não há etapa pendente desse ciclo; a matriz interativa completa da tela e os demais protocolos não foram declarados validados.

Bandeja, instalador, caminho compartilhado protegido e distribuição foram reservados para depois desses critérios. A evolução da bandeja está registrada abaixo.

Em 2026-09-10, o usuário autorizou seguir esta ordem: **validação complementar da interface → bandeja do Windows → ampliação dos ensaios de rede → instalador local**. A [primeira etapa](validacao-interface.md) teve build isolado sem avisos/erros e 199 testes aprovados, incluindo o temporizador real de Conexões em uma janela WPF em memória. Na janela real, foram confirmados o avanço automático do horário sem clicar em **Atualizar**, a pesquisa/limpeza, o filtro de regra direta, os tooltips de saída/caminho e o foco visível na tabela. O usuário avaliou a janela como aparentemente OK após o roteiro de teclado e redimensionamento; os limites dessa revisão básica e a ausência de informação sobre a escala estão registrados. O descarte com serviço parado, o fechamento normal após esperar a parada simulada e a preservação de uma edição feita durante essa espera foram confirmados por eventos e captura posterior. O controle de janela está indisponível e os cliques de revisão são manuais. Essa validação não reabre os ensaios de Wi-Fi/suspensão já concluídos.

O host de revisão usou as mesmas DLLs, uma regra fictícia e atraso simulado de 15 segundos. O último ensaio registrou a edição dentro da espera e a captura mostrou a janela aberta com a alteração pendente após a parada. O clique na resposta do diálogo não foi capturado isoladamente. O host não ativa roteamento nem requer UAC; seu encerramento manual é uma limpeza separada. A implementação da [bandeja do Windows](bandeja-windows.md) começou em outra compilação, preservando o fechamento seguro já existente.

A bandeja foi implementada localmente: minimizar esconde o painel, o menu restaura a janela e reutiliza os controles do serviço, e X/Sair mantêm as confirmações e a espera pela parada. Build sem avisos/erros e **216 testes aprovados**, sendo 17 novos cenários da bandeja. O usuário confirmou **“Deu certo”** após minimizar/restaurar e enviou capturas do ícone e do menu; essa revisão básica está concluída, com os limites registrados. O host fictício anterior foi encerrado sem salvar alterações. A [preparação dos ensaios adicionais de rede](ampliacao-ensaios-rede.md) encontrou IPv6 desativado nas duas placas; não houve ativação de serviço, alteração de conectividade, instalador, commit ou push.

## Histórico do limite de viabilidade

O registro anterior de filtros `2/2` prova instalação de filtros, não redirecionamento de novas conexões. Esse motor foi substituído por `FwpmConnectionPolicyAdd0`, API nativa presente neste Windows. Ela permite selecionar a interface sem callout/driver próprio. As políticas dependem da opção global `routepolicies`, que esta implementação não habilita automaticamente.

`CheckPublicIpPerMappedApplication` na PoC executa `curl --interface` com o IP local de cada placa. Os resultados são uma referência das conexões disponíveis, não medições de tráfego originado em `explorer.exe` ou `steam.exe`.

A pendência original de executar a prova elevada `--verify-routing` foi resolvida para IPv4 TCP/UDP, conforme [evidências do motor nativo](roteamento-nativo.md). Isso não comprova IPv6 nem sincronização completa do OneDrive. Steam deixou de ser o alvo ativo por decisão do usuário. A UI diferencia escolha salva, política aceita e tráfego ainda não verificado; os 58 testes acima e a contagem posterior de 95 pertencem às entregas anteriores.

Referências de implementação: a API [File.Replace](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.replace) permite substituir o arquivo criando backup; o [roteamento nativo](roteamento-nativo.md) documenta a API e os pré-requisitos adotados no motor atual.
