# Validação complementar da interface

Iniciada em 2026-09-10, após o usuário autorizar a sequência: validação da interface, bandeja do Windows, ampliação dos ensaios de rede e instalador local. Os comportamentos básicos revisados foram confirmados no escopo e com os limites abaixo; a próxima entrega de desenvolvimento é a bandeja.

## Compilação e testes

Base: `4b13ca16e21d557f7a26a037515a050337d1890e`. Na compilação desta etapa, o código de produção é igual a esse commit. Foi acrescentado um teste WPF para uma lacuna da validação: a atualização recorrente de Conexões pelo temporizador da própria janela. O desenvolvimento posterior da bandeja usa outra compilação isolada.

- Build Release isolado: **0 erros e 0 avisos**.
- Suíte completa: **199 aprovados, 0 falhas e 0 ignorados**, em 40 segundos.
- Resultado: `artifacts/ui-validation-20260910/test-results/ui-validation.trx`.
- Executável: `artifacts/ui-validation-20260910/bin/NetLane.UI/release/NetLane.UI.exe`; serviço irmão na mesma compilação isolada.

O novo teste `LoadedWindowRefreshesConnectionsRecoversFromFailureAndStopsPollingAfterClose` dispara o evento `Loaded` de uma janela WPF em memória e aguarda o `DispatcherTimer` real, com seu intervalo de dois segundos. A fonte de conexões e o serviço são simulados. Ele verifica:

1. Leitura inicial e atualização posterior sem chamar manualmente o método de atualização.
2. Preservação da seleção, da pesquisa, do filtro de regra direta e de edições não salvas durante uma leitura válida.
3. Remoção das linhas e da seleção quando a fonte falha.
4. Recuperação automática na próxima leitura, com a saída observada atualizada e os filtros preservados.
5. Arquivo de regras inalterado e nenhuma chamada de início/parada do serviço simulado.
6. Ausência de novas consultas após fechar, aguardando mais de um intervalo do temporizador.

Os oito cenários anteriores de fechamento com parada assíncrona também passaram nesta suíte. Eles usam confirmações e serviço simulados; não são prova de operação do diálogo nativo na janela real.

## Revisão de layout

Foram geradas 18 imagens WPF em `artifacts/ui-validation-20260910/renders`, incluindo as quatro páginas em 1000×650, 1220×810 e 1440×920 unidades de layout.

Revisão visual realizada: Conexões em 1000 e 1440, estado de erro de Conexões em 1000, Regras em 1000, Diagnóstico em 1000 e Visão geral em 1000. Os controles principais estão legíveis; Conexões mantém as três colunas e as demais páginas usam rolagem vertical para o conteúdo excedente. As verificações automatizadas de limites dos controles nos três tamanhos passaram.

Essas imagens são renderizações em memória a 96 DPI. Elas não validam mudança de monitor, escala real de 125%/150%/200%, hover ou foco por teclado no desktop.

## Conferência interativa

O controle de desktop falhou ao listar aplicativos, inclusive após reinicializar a sessão JavaScript e importar novamente `@oai/sky`:

```text
Computer Use native pipe is unavailable: failed to connect native pipe: O sistema não pode encontrar o arquivo especificado. (os error 2)
```

Nenhum clique, tecla ou comando de início de roteamento foi enviado pela automação. A UI da compilação acima foi aberta sem elevação às 06:41, PID histórico `19756`, para conferência pelo usuário. Revalidar caminho e identidade do processo ao retomar; não reutilizar esse PID como alvo.

A captura enviada pelo usuário mostra **Leitura às 06:45:55**, **Serviço parado**, pesquisa vazia, filtro **Todos os processos** e uma tabela legível com saídas Ethernet. Após esclarecer que o campo apresenta o horário da última coleta, o usuário confirmou: **“Ele avança sem clicar em atualizar”**. A atualização automática da janela real fica confirmada por esse relato; a imagem isolada não é a prova do avanço. A consulta independente às 06:46:54 encontrou a mesma UI respondendo, serviço ausente e hash das regras preservado. Essa confirmação não encerra a conferência dos filtros, tooltips, teclado, escalas e diálogos.

As quatro capturas seguintes, enviadas pelo usuário, confirmaram:

- Pesquisa `onedrive`: aparecem o processo principal e `OneDrive.Sync.Service.exe`, em linhas distintas; o principal tem regra Wi-Fi com serviço parado e o auxiliar não tem regra direta. Ambos apresentam saída observada Ethernet naquela amostra.
- Limpeza da pesquisa: a lista volta a mostrar outros aplicativos no filtro **Todos os processos**.
- Filtro **Com regra direta**: aparecem `OneDrive.exe` e `steam.exe`, este identificado como **Regra desativada**. Esse resultado é esperado: o filtro inclui associações diretas desativadas e não inicia roteamento. O auxiliar sem regra direta deixa de aparecer.
- Retorno a **Todos os processos** e tooltip da saída: o popup mostra `Ethernet · 192.168.15.5 · 3 TCP`, coerente com a linha de `claude.exe` nessa captura.

Nesse conjunto inicial, o tooltip do caminho completo não estava aberto. As contagens de conexões de capturas diferentes são amostras temporais, sem expectativa de igualdade. A presença passiva do Steam na lista não amplia os ensaios de roteamento para esse aplicativo.

Nas duas capturas seguintes, o tooltip mostrou integralmente o caminho do executável de `ChatGPT Classic.exe`, e a tabela exibiu foco pontilhado visível na célula de `claude.exe`, com a linha selecionada. Após o roteiro de teclado e redimensionamento, o usuário informou **“Aparentemente a janela está ok”**. Foram registrados o tooltip completo, o foco da tabela e a avaliação visual básica positiva. Os recortes não documentam cada tecla, a seleção do filtro pelo teclado nem todas as posições/tamanhos da janela; a escala do Windows não foi informada. Esses limites não invalidam os itens efetivamente observados.

O usuário enviou quatro capturas e respondeu **“sim”** ao ensaio de descarte com serviço parado. A primeira mostra o diálogo nativo após tentar fechar com OneDrive desativado apenas no editor; a segunda mostra a janela preservada com a edição pendente após **Não**. A terceira registra a confirmação ao recarregar, e a última mostra OneDrive novamente habilitado, Steam desativado e **Salvar alterações** indisponível, sem edição pendente. A consulta independente às **09:46:06** confirmou a mesma UI respondendo, ausência do serviço e hash do arquivo real idêntico. Esse fluxo está concluído; ele não exercita edição durante uma parada em andamento.

| Item | Evidência disponível | Situação no desktop |
| --- | --- | --- |
| Atualização automática de Conexões | Temporizador WPF exercitado no novo teste | Confirmada pelo usuário: o horário avança sem clicar em Atualizar |
| Pesquisa e filtro Com regra direta | Bindings e preservação durante atualização/falha verificados | Confirmados pelas capturas: pesquisa, limpeza, regra direta e retorno a Todos os processos |
| Tooltip da saída observada | Conteúdo associado no template | Confirmado pela captura do popup com interface, IP e contagem TCP |
| Tooltip do caminho do executável | Conteúdo associado no template | Confirmado pela captura do caminho completo no popup |
| Navegação por teclado e foco visível | Layout e árvore de acessibilidade verificados em memória | Foco da tabela confirmado; avaliação básica positiva do usuário, sem registro de todo o percurso pelo teclado |
| Janela menor, maximizar e restaurar | Layout sintético em três tamanhos | Avaliação visual básica positiva após o roteiro; recortes não documentam todas as transições |
| Escalas reais de tela | Renderização a 96 DPI | Escala atual não informada; compatibilidade com outras escalas permanece não avaliada |
| Recusar descarte e preservar edição | Cenários WPF anteriores aprovados | Confirmado pelas quatro capturas e pelo usuário; Recarregar restaurou a regra e o arquivo real permaneceu idêntico |
| Fechar aguardando a parada da sessão simulada | Fluxo da DLL de produção, sem substituição do diálogo nativo | Confirmado no segundo ensaio: parada de aproximadamente 15 segundos, nova solicitação de fechamento adiada durante a espera e encerramento após a parada |
| Editar durante parada e preservar a edição ao recusar descarte | Oito cenários WPF anteriores aprovados | Confirmado no terceiro ensaio: evento de edição durante a parada e captura posterior com janela aberta, serviço simulado parado e edição preservada; o clique na resposta não foi capturado isoladamente |

### Roteiro manual inicial, com serviço parado

1. Em **Conexões**, aguardar 10 segundos sem pressionar **Atualizar**. Registrar se o horário avança e a janela continua respondendo.
2. Pesquisar um aplicativo que esteja na lista; verificar o resultado e limpar a pesquisa. Alternar **Todos os processos** e **Com regra direta**. Uma lista vazia pode ser correta quando nenhum processo observado tem regra direta.
3. Passar o ponteiro sobre o caminho do aplicativo e a saída observada; conferir se os detalhes aparecem e são legíveis.
4. Usar `Tab` e `Shift+Tab` para conferir foco visível na pesquisa, no filtro e na tabela. Abrir o filtro pelo teclado e retornar a **Todos os processos**.
5. Redimensionar a janela, maximizar e restaurar. Conferir acesso à navegação, aos filtros e à rolagem. Anotar a escala atual do Windows; não presumir validação de outras escalas.
6. Com **Serviço parado** confirmado, em **Regras de apps**, fazer uma alteração temporária sem salvar. Tentar fechar e responder **Não** ao descarte: a janela e a edição devem permanecer. Usar **Recarregar** e confirmar o descarte apenas dessa edição de teste para retornar ao arquivo original.

O cenário específico de edição durante parada será conferido separadamente no host de revisão descrito abaixo. A confirmação do roteiro com serviço parado não encerra esse cenário. Os ensaios anteriores de Wi-Fi e suspensão permanecem concluídos; não foram repetidos nesta etapa.

### Janela nativa com parada simulada

Foi preparado o host opt-in [`NetLane.CloseReview`](../tests/NetLane.CloseReview/README.md), fora da solução e da suíte automática. Ele copia as DLLs existentes da UI validada e utiliza o construtor interno já disponível para injetar uma sessão simulada, adaptador fictício e arquivo exclusivo de regras. O código de produção e seus diálogos não são alterados. A única regra é **Aplicativo de teste**, sem associação a aplicativos reais.

- Build do host: **0 avisos e 0 erros**.
- `--check`: saída 0 e relatório `Prepared`, janela instanciada/fechada em memória e arquivo sintético preservado. Esse modo não abre a janela nem executa a parada de 15 segundos.
- Hash SHA-256 da DLL de UI: `EF57D897A98B7393D289E752D852B3A5DBFA20E117B44A3E1B1E0A6C55F25811`, idêntico ao da compilação `ui-validation-20260910`. As DLLs Core e Network também foram comparadas e são idênticas.
- Binário: `artifacts/ui-close-review-20260910/bin/NetLane.CloseReview/release/NetLane.CloseReview.exe`.
- `--review` abriu a janela **ENSAIO DE FECHAMENTO — regra fictícia e serviço simulado**, PID histórico `28528`, sem elevação. O NetLane normal permaneceu aberto com o serviço parado.
- Relatórios por execução: `review-runs/<data-id>/review.json`, junto ao host. Eles registram início/fim da parada, edição durante a espera, estado do editor, revisão do arquivo sintético e encerramento da janela. Os diretórios de execuções anteriores são preservados.

O ensaio aberto está em `review-runs/20260910-135309-b5066b3893334534bcf6a262e12fdd57/review.json`, com estado inicial `AwaitingManualClose`, regra fictícia habilitada e eventos `PreparedWindow`/`WindowLoaded`. A última conferência de preparação encontrou o host e a UI normal respondendo, serviço real ausente, regras reais intactas e `routepolicies` IPv4/IPv6 desativados.

Roteiro na janela **ENSAIO DE FECHAMENTO**:

1. Clicar no X e confirmar **Sim** para parar a sessão simulada e fechar.
2. Durante a espera de 15 segundos indicada no título, desativar **Aplicativo de teste**, sem salvar.
3. Ao surgir o diálogo de alterações não salvas, responder **Não**. A janela deve permanecer aberta com a edição preservada e a sessão simulada já parada.
4. Recarregar, confirmar o descarte da edição fictícia e fechar pelo X.

O host não instancia `WindowsServiceSession`, não executa motor WFP, comandos de rede, processo de serviço ou UAC; início/reinício ficam indisponíveis. A comprovação de edição dentro da espera vem dos eventos, e a escolha no diálogo precisa de captura/relato do usuário. O ensaio verifica a janela e os diálogos nativos com serviço simulado, não restauração de rede real. Os 199 testes anteriores não foram repetidos, pois as DLLs de produção permanecem idênticas.

Após o usuário informar **“feito”**, a consulta às **10:04:24** encontrou esse primeiro host ainda aberto, no estado `AwaitingManualClose`, sessão simulada ativa e editor sem alterações pendentes. O relatório contém três eventos `EditedOutsideStop`, às 09:57:00 e 09:57:18, mas nenhum `StopStarted`, `StopCompleted` ou `WindowClosed`. Isso não permite confirmar o cenário solicitado. Foi perguntado ao usuário se a contagem de 15 segundos chegou a aparecer; não se presumiu qual janela ou diálogo foi operado.

A inspeção identificou também uma limitação do registrador inicial: ele observava apenas a primeira instância da linha, substituída por **Recarregar**. Essa limitação afeta a evidência de edições posteriores à recarga; não explica, por si só, a ausência de `StopStarted`. O host foi corrigido para reassinar as linhas atuais, desligar as antigas e registrar apenas a mudança de **Ativa**, evitando confundir notificações de atualização de rota com edição manual.

O registrador versão 2 foi compilado separadamente em `artifacts/ui-close-review-v2-20260910`, com **0 erros e 0 avisos**, sem substituir o processo aberto. O `--check` terminou com saída 0, `RecorderReloadCheckPassed`, arquivo sintético preservado e janela de verificação encerrada. Foram conferidas duas recargas e uma notificação por edição da linha atual, sem notificações das linhas descartadas. Evidência: `review-runs/20260910-140638-566c421b4a184a3c9de5087b68432977/review.json`. Esse relatório tem `Mode: "check"`, não representa cliques do usuário nem comprova os diálogos. Até essa conferência, a versão 2 não havia sido aberta para revisão manual.

Após o usuário pedir **“faz denovo”**, às **10:14:01** o processo antigo foi encerrado exclusivamente para substituir o host de teste. Antes disso foram conferidos caminho, identidade, sessão simulada, ausência de edição pendente e de parada em andamento. O encerramento foi pelo gerenciamento de processos, não pelo fluxo nativo da janela; portanto, não conta como aprovação do fechamento. Seu diretório e relatório foram preservados. O NetLane normal não foi encerrado.

A versão 2 foi então aberta, PID histórico `39208`, com novo relatório em `artifacts/ui-close-review-v2-20260910/bin/NetLane.CloseReview/release/review-runs/20260910-141401-7a80546d590e43df90dce7f74bff29c0/review.json`. A leitura às **10:14:11** confirmou `RecorderVersion: 2`, `Mode: "review"`, `WindowLoaded`, `AwaitingManualClose`, regra fictícia habilitada, editor limpo e janela respondendo. O serviço real continuava ausente e o hash das regras reais permaneceu idêntico. O controle de desktop voltou a falhar com o mesmo erro de pipe; os cliques permanecem manuais. O usuário deve responder **Não** à confirmação após editar durante a espera e deixar a janela aberta para conferência antes da limpeza final.

Após o usuário informar **“fechei”**, esse relatório passou a `WindowClosedAwaitingUserReview`. A sequência registrada foi `StopStarted` às **10:21:36.807**, solicitação inicial de fechamento adiada, nova solicitação adiada às **10:21:44.639**, `StopCompleted` às **10:21:51.923**, `CloseAllowed` e `WindowClosed`. O processo do ensaio já não estava em execução. Isso confirma o fechamento normal após a espera de aproximadamente 15 segundos, incluindo a proteção contra nova solicitação de fechamento durante a parada. Não houve evento de edição: `EditedWhileStopping: false`, `HasChanges: false` e regra fictícia ainda habilitada. Portanto, essa execução não exercitou a nova confirmação de descarte nem sua recusa. Foi solicitada confirmação do usuário sobre ter tentado desativar a regra durante a contagem; não se presumiu falha de binding nem escolha de diálogo.

O arquivo sintético permaneceu inalterado, não houve `error.txt` nesse diretório e o hash das regras reais continuou idêntico. O NetLane normal permaneceu respondendo e não havia processo do serviço real. Nenhum ensaio novo foi aberto automaticamente após esse resultado.

O usuário respondeu **“não”** à pergunta sobre ter desativado a regra durante a contagem. Isso é coerente com o registro: o fechamento sem um novo aviso de descarte era o resultado esperado, e não uma falha identificada da UI. Para concluir o cenário restante, o mesmo host corrigido foi reaberto às **11:04:39**, sem recompilação, após confirmar que não havia outro host em execução. Novo PID histórico: `14584`; relatório: `artifacts/ui-close-review-v2-20260910/bin/NetLane.CloseReview/release/review-runs/20260910-150439-2fdc606743ca48eca4b9a1e397a4eeda/review.json`.

A leitura às **11:04:50** confirmou janela respondendo, `Mode: "review"`, `RecorderVersion: 2`, evento `WindowLoaded`, estado `AwaitingManualClose`, regra fictícia habilitada e editor sem alterações. A DLL revisada mantém o mesmo hash, as regras reais permanecem intactas e o serviço real continua ausente. O roteiro foi reforçado: após X e **Sim**, desligar **Ativa** durante a contagem, sem salvar; no novo aviso, responder **Não** e deixar a janela aberta para conferência. Essa nova execução ainda não é evidência do cenário concluído.

### Resultado do terceiro ensaio

A captura seguinte enviada pelo usuário mostra **Serviço parado**, a regra fictícia desativada, **Não salva**, **Alterações pendentes**, o botão de salvar disponível e a janela aberta, sem diálogo visível. A consulta independente às **11:07:06** encontrou `StopStarted` às **11:06:22.122**, `EditedDuringStop` às **11:06:23.812** e `StopCompleted` às **11:06:37.272**. O relatório confirma `EditedWhileStopping: true`, `HasChanges: true`, `RuleEnabled: false`, `WindowClosed: false` e arquivo sintético inalterado. A edição ocorreu dentro da parada e permaneceu após ela, conforme o resultado esperado do roteiro de recusa do descarte.

O host não registra a resposta retornada pelo `MessageBox`, por isso seu campo `State` continua `AwaitingNativeDiscardResponse` mesmo após a resposta. Esse rótulo não prova que um diálogo ainda esteja aberto. A captura posterior complementa os eventos; o clique em **Não** não foi capturado isoladamente. Não se confunde essa comprovação da janela e do estado do editor com uma nova prova de roteamento ou limpeza WFP real.

O serviço real estava ausente e as regras reais mantiveram o hash de referência. O usuário foi orientado a concluir a limpeza por **Recarregar → Sim** e fechar apenas o host fictício, sem salvar. Às **11:20:17**, esse host ainda permanecia aberto com a edição de teste; não foi encerrado à força. A implementação da bandeja foi iniciada em artefatos separados, sem reutilizar essa janela nem substituir as DLLs do ensaio.

A conferência seguinte, após as capturas da bandeja, confirmou a limpeza final: `CloseAllowed` e `WindowClosed` às **11:25:43**, `HasChanges: false`, `RuleEnabled: true`, `SyntheticPolicyUnchanged: true` e sessão simulada encerrada. Não havia mais processo do host. A edição fictícia foi descartada sem alterar o arquivo, concluindo também a limpeza desse ensaio.

## Preservação e próximo passo

O hash das regras reais permaneceu `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F` antes/depois da suíte e da abertura da UI. Não havia processo `NetLane.Service` nessas conferências. As opções `routepolicies` IPv4/IPv6 estavam desativadas na preparação.

Nova conferência às **06:56:35** confirmou a UI do mesmo build respondendo, ausência do serviço, `routepolicies` IPv4/IPv6 desativados e hash das regras idêntico. OneDrive continuava habilitado para Wi-Fi no arquivo, com Steam desativado. O descarte com serviço parado e a preservação da edição durante a parada simulada foram posteriormente confirmados, conforme registrado acima.

Os comportamentos básicos desta etapa foram revisados, mantendo explícitos os limites de DPI, percurso completo pelo teclado e uso de serviço simulado no último cenário. A limpeza do host também foi confirmada. A entrega seguinte, [bandeja](bandeja-windows.md), já recebeu conferência básica do usuário; segue-se a preparação dos ensaios adicionais de rede. Steam e Microsoft Store permanecem fora do ciclo atual.
