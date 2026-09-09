# Ensaios de recuperação

Etapa acordada após o controle de sessão, o teste do OneDrive e a implementação de Conexões. O usuário autorizou o ciclo real iniciar → reiniciar → parar com opções temporárias e UAC manual. **Ciclo concluído com restauração final verificada em 2026-09-07.** Em 2026-09-09, o ensaio autorizado de perda/retorno da Wi-Fi confirmou a retirada e a reaplicação da política, além de uma nova conexão TCP/IPv4 do OneDrive na Wi-Fi. **Parada normal e limpeza final confirmadas, com 18 verificações aprovadas e a diferença anterior da métrica automática registrada separadamente.** O ensaio de suspensão/retomada autorizado em seguida também foi concluído: mesma sessão recuperada, Conexões conferida antes/depois, parada normal e limpeza final verificadas, com **28 verificações aprovadas** (16 de recuperação e 12 de conferência/encerramento). Steam permanece fora do teste.

## Retomada em 2026-09-09

O usuário pediu para continuar o projeto. A preparação local foi repetida, preservando as alterações anteriores ainda sem commit/push:

- HEAD continua em `07cc3eec15349e8ac11fce4ba9ca86da2efc27c2`. O arquivo real de regras mantém o SHA-256 registrado abaixo, com somente `OneDrive.exe` habilitado na Wi-Fi e Steam desabilitado.
- Nenhum processo `NetLane.UI` ou `NetLane.Service` estava ativo. `routepolicies` permanece desativado em IPv4 e IPv6.
- Ethernet conectada, IPv4 `192.168.15.4`, gateway `192.168.15.1`, métrica de interface `25` e de rota `0`. Wi-Fi desconectada, com endereço automático `169.254.115.131` e métrica de interface `25`; não há rota padrão por ela. A referência do próximo ensaio precisa ser coletada novamente após a reconexão, sem reutilizar IP, métrica ou PID históricos.
- Novo build Release isolado em `artifacts/resume-20260909`, com **0 avisos e 0 erros**. A suíte completa passou: **190 aprovados, 0 falhas e 0 ignorados**. Evidência: `artifacts/resume-20260909/test-results/resume.trx`. Esses testes não ativam o motor de roteamento nem alteram a conectividade.
- A conferência interativa de Conexões continua pendente. O controle de desktop falhou já ao listar aplicativos, com `Computer Use native pipe is unavailable: failed to connect native pipe: O sistema não pode encontrar o arquivo especificado. (os error 2)`. Reinicializar a sessão JavaScript e repetir a consulta produziu o mesmo erro; nenhuma ação foi enviada à UI.

Próximo passo: reconectar a Wi-Fi e obter a confirmação específica para o ensaio de perda/retorno, incluindo a ativação temporária da sessão pelo painel com UAC manual. O pedido geral de retomada não foi tratado como autorização para interromper a rede. Nenhum ensaio disruptivo foi iniciado ou agendado.

### Wi-Fi reconectada e ensaio autorizado

Em resposta à solicitação específica acima, o usuário confirmou: **“conectei a wifi e vamos seguir o teste”**. A autorização cobre este ensaio de perda/retorno, a sessão temporária e o UAC manual; não precisa ser solicitada novamente para esses passos.

A coleta das 05:08 (America/Cuiaba) confirmou Wi-Fi conectada em `192.168.0.102`, com gateway `192.168.0.1` e métrica de interface `55`; Ethernet permanece em `192.168.15.4`, gateway `192.168.15.1` e métrica `25`. Ambas as rotas padrão têm métrica `0`. Nenhum serviço NetLane estava ativo, ambas as opções `routepolicies` estavam desativadas e o hash das regras permanecia idêntico. O heartbeat antigo `Stopped` foi classificado como histórico, sem associação a processo atual.

Referência inicial: `artifacts/wifi-recovery/20260909-050719/baseline-20260909-050856-571.json`. O coletor local `capture.ps1`, no mesmo diretório, registra processos, estado das placas, rotas/métricas, opções globais, atualidade do heartbeat e endpoints TCP/IPv4 do executável principal do OneDrive. Ele não altera a rede nem controla processos; destinos remotos são representados somente por uma impressão digital da conexão. A coleta inicial terminou sem erros.

O controle de desktop voltou a falhar com `Computer Use native pipe is unavailable`. Por isso, o próximo passo depende apenas da operação manual do painel: abrir `artifacts/resume-20260909/bin/NetLane.UI/release/NetLane.UI.exe`, autorizar as opções temporárias em Diagnóstico e iniciar o serviço, aprovando o UAC. A Wi-Fi deve permanecer conectada até confirmar o retorno recente da sessão e a política esperada. Nenhuma desconexão foi executada nesta preparação.

### Perda e retorno da Wi-Fi — concluído com limpeza final

Após o usuário informar que iniciou a sessão, a consulta confirmou a UI PID `30844`, do build `artifacts/resume-20260909`, e seu serviço filho PID `28356`, criado às 05:11:10. O retorno recente estava em `Ready`, com apenas a política IPv4 do OneDrive aceita e ambas as opções temporárias ativas. O caminho do processo elevado não foi exposto pela consulta sem elevação; foram conferidos o caminho da UI, seu build, a relação de pai/filho e a correspondência do retorno com a instância atual.

O ensaio usou `netsh wlan disconnect` e reconectou ao mesmo perfil existente. A reconexão ficou no bloco de limpeza do script para também ocorrer se a observação da retirada falhasse. Não houve outro controlador de roteamento ou reinício do serviço, nem comandos para editar regras, métricas, gateway ou DNS.

- **05:14:26:** pedido de desconexão da Wi-Fi. A primeira amostra seguinte confirmou a placa desconectada e a Ethernet conectada.
- **05:14:36:** retorno `Attention`, com a regra do OneDrive não aplicada e a explicação de interface ausente/desconectada. A retirada foi reportada **9,6 segundos** após o pedido de desconexão.
- **05:14:39:** pedido de reconexão ao mesmo perfil. A primeira amostra com Wi-Fi conectada foi às 05:14:42.
- **05:14:46:** retorno `Ready`, com a mesma política novamente aceita, **6,6 segundos** após o pedido de reconexão.
- As **12 amostras** do ciclo conservaram o mesmo serviço, a Ethernet conectada e o hash das regras. Não houve erro de execução ou de coleta. Os estados são amostras e retornos informativos do serviço, não uma captura contínua de pacotes ou enumeração independente das políticas WFP.
- Na observação das **05:16:21**, uma nova conexão TCP/IPv4 do `OneDrive.exe`, criada às **05:14:55**, tinha origem `192.168.0.102` na Wi-Fi. Uma conexão antiga de 04:39 continuava na Ethernet. Isso comprova a saída observada dessa nova conexão após a recuperação; não amplia o ensaio para auxiliares, UDP/QUIC, IPv6, volume transferido ou integridade de arquivos.

Os endereços e as rotas padrão retornaram à referência. A métrica automática da Wi-Fi exige uma ressalva: era `55` na referência das 05:08 e já estava em `35` na amostra das 05:12, **antes da interrupção**. Permaneceu `35` durante e após o ciclo; Ethernet permaneceu `25`, e o modo de métrica automática continuou habilitado. O instante e a causa exata da mudança anterior não foram rastreados; não declarar igualdade numérica de todas as métricas com a primeira referência nem alterar a configuração para forçar essa igualdade.

Evidências em `artifacts/wifi-recovery/20260909-050719/`: `started-20260909-051229-016.json`, `wifi-disconnected-20260909-051439-846.json`, `wifi-reconnected-20260909-051448-945.json`, `observation-20260909-051621-362.json` e `cycle-result.json`. Este último preserva a linha do tempo da recuperação até a etapa anterior à parada; a conclusão completa está em **`result.json`**.

O usuário fez a parada normal pelo painel e enviou uma captura com **“Sessão encerrada. Opções temporárias restauradas.”**, **Iniciar serviço** disponível e **Parar/Reiniciar** desabilitados. A imagem também confirmou o executável irmão em `artifacts/resume-20260909/bin/NetLane.Service/release/NetLane.Service.exe`. O retorno `Stopped` foi publicado às **05:21:26**, com lista de regras vazia e sem erro.

A comparação final das **05:22:52** confirmou ausência de qualquer `NetLane.Service`, saída da instância `28356`, ambas as opções `routepolicies` novamente desativadas, hash das regras idêntico e endereços/rotas padrão iguais à referência. As duas placas permaneceram conectadas; os processos do OneDrive foram preservados. As métricas finais são `35` na Wi-Fi e `25` na Ethernet, iguais à amostra anterior à interrupção, com a diferença em relação às 05:08 explicitada acima. A UI `30844` permaneceu aberta.

**18 verificações aprovadas, nenhuma falha.** O resultado é `CompletedWithRecordedMetricDifference`, sem declarar igualdade de todas as métricas com a referência inicial. O recibo visual tem como fonte a captura fornecida pelo usuário; processo, opções globais, arquivo e estado de rede foram verificados independentemente. Evidência final: `stopped-20260909-052252-079.json` e `result.json`, gerado por `finalize.ps1`. Esses 18 itens são verificações do ensaio real, distintos dos 190 testes automatizados aprovados na preparação; a suíte não foi repetida nesta conclusão, que não alterou o código do motor.

Ao concluir esse ciclo, ficaram pendentes a conferência interativa da página **Conexões** e a autorização separada para suspensão/retomada. A captura de Diagnóstico confirma a parada, mas não conclui a conferência de Conexões. A autorização para os próximos passos foi dada na resposta seguinte, registrada abaixo.

## Suspensão/retomada e Conexões — preparação autorizada em 2026-09-09

Em resposta aos dois itens pendentes, o usuário disse **“vamos fazer”**. Isso autoriza a conferência da tela e o ensaio manual de suspensão/retomada, com a mesma sessão temporária, UAC manual e regra do OneDrive. Não solicitar novamente essa autorização para os mesmos passos. O ensaio não reinicia o Windows, não altera o plano de energia e não amplia as regras para outros executáveis.

A preparação das 05:28–05:32 confirmou:

- A UI `30844`, do build `resume-20260909`, continua aberta; nenhum serviço NetLane está ativo. O retorno anterior `Stopped` é histórico, e ambas as opções `routepolicies` estão desativadas. O arquivo de regras mantém o hash original.
- Ethernet e Wi-Fi conectadas, com os mesmos endereços `192.168.15.4` e `192.168.0.102`; métricas automáticas atuais `25` e `35`, respectivamente.
- `powercfg /a` informa suporte à suspensão **S3**. O último boot registrado foi às 04:38:32. Nenhuma configuração de energia foi alterada.
- Os manifestos locais confirmaram os eventos de entrada/saída de suspensão do provedor `Microsoft-Windows-Kernel-Power` (42/107) e o retorno informado por `Microsoft-Windows-Power-Troubleshooter` (1). A referência inicial não contém eventos novos de suspensão; falhas de leitura são registradas separadamente da ausência de eventos.
- Uma leitura nativa pelo mesmo coletor usado pela UI encontrou 16 processos às 05:32:29. O processo principal do OneDrive e seu auxiliar separado tinham uma conexão TCP/IPv4 cada na Ethernet, com o serviço parado. O smoke test de leitura passou; isso não substitui a conferência da janela. Evidências: `artifacts/sleep-resume/20260909-053110/connections-before-ui.json` e `test-results/connections-before-ui.trx`.

O controle de desktop continua retornando `Computer Use native pipe is unavailable`. A próxima interação precisa ser manual: abrir **Conexões**, pesquisar OneDrive e fornecer uma captura; depois iniciar a sessão em **Diagnóstico**, com UAC manual. Antes de suspender, é necessário registrar o novo PID, a política aceita e o retorno recente, sem reaproveitar a sessão anterior.

O coletor somente leitura está preparado em `artifacts/sleep-resume/20260909-053110/capture.ps1`. Ele reutiliza a coleta de rede, grava em diretório separado e acrescenta os eventos de energia e o horário do último boot. A referência inicial `baseline-20260909-053233-937.power.json`, junto ao JSON de rede correspondente, foi gravada sem erros. O coletor não controla o serviço nem suspende o computador.

Sequência preparada: capturar a sessão ativa antes da suspensão; o usuário suspende manualmente por aproximadamente 60 segundos e retoma; verificar eventos reais de suspensão, sessão atual, rede e nova leitura de Conexões. Se a sessão continuar ativa, exigir retorno recente; se encerrar por perda de keepalive, verificar a limpeza sem reiniciar automaticamente. Ao fim, confirmar serviço parado, recibo de limpeza, opções globais restauradas e configuração preservada. A suspensão ainda não havia sido executada nesse ponto.

### Conexões conferida e nova sessão pronta para suspensão

A captura enviada pelo usuário mostra **Leitura às 05:35:10**, pesquisa `onedrive` e filtro **Todos os processos**. As duas linhas estão legíveis: `OneDrive.exe` (PID `19836`) tem regra **Wi-Fi · conectada**, serviço parado e saída observada **Ethernet · 192.168.15.4**, com uma conexão TCP/IPv4; `OneDrive.Sync.Service.exe` (PID `21976`) aparece separado, **sem regra direta**, também com uma conexão na Ethernet. Os três grupos de informação e os caminhos estão visíveis no recorte. A observação coincide com os campos da referência nativa anterior, coletada em outro instante.

**Conferência visual básica e pesquisa concluídas nesse recorte.** Uma imagem estática não confirma avanço do temporizador, tooltips, filtro de regra direta, navegação por teclado ou todos os tamanhos de janela. O registro estruturado está em `artifacts/sleep-resume/20260909-053110/connections-ui-review.json`.

O usuário informou **“Sessão ativa”** após enviar a captura. A verificação confirmou a nova instância PID `32696`, criada às **05:35:34**, filha da mesma UI `30844`. O retorno recente estava em `Ready`, com apenas a política IPv4 do OneDrive aceita. O aviso de serviço parado da imagem é anterior a esse início e não foi tratado como estado atual.

O observador somente leitura `observe.ps1` foi iniciado e confirmou **`ArmedAwaitingManualSleep`**, com processo coletor PID `18656`. A referência anterior à suspensão está em `before-sleep-20260909-053858-201.power.json` e no JSON de rede correspondente: mesma configuração, duas placas conectadas, opções temporárias ativas e sessão recente. Ele observa por até 15 minutos, usa uma lacuna de leitura apenas como gatilho e exige eventos reais de energia para confirmar a suspensão. Após detectar o retorno, coleta o estado imediato e outra amostra após aproximadamente 35 segundos. Não controla o serviço, a energia ou a rede e não cria tarefa agendada.

A ação manual solicitada nessa preparação foi manter o NetLane aberto, suspender pelo menu do Windows por aproximadamente 60 segundos, retomar/desbloquear e enviar uma nova captura de **Conexões**, com a pesquisa OneDrive. O retorno real foi confirmado na etapa seguinte.

### Retomada confirmada — antes da parada final

Após o usuário informar **“feito”**, os eventos reais do Windows confirmaram entrada e retorno da suspensão, sem reiniciar o computador. O evento `Microsoft-Windows-Power-Troubleshooter/1`, registro `80865`, informa `SleepTime` às **05:44:23,492** e `WakeTime` às **05:45:39,805** (America/Cuiaba), intervalo de **76,3 segundos**. Também foram registrados `Kernel-Power/42` e `107`, registros `80848` e `80854`. A duração acima usa os campos do evento 1, não o horário de publicação do evento 107 nem a lacuna de 71 segundos do observador. O último boot permaneceu às 04:38:32.

- As **175 amostras** mantiveram a mesma instância de serviço `32696`. A UI `30844`, o OneDrive `19836` e seu auxiliar `21976` também conservaram suas identidades nas capturas anterior e posterior. Não houve reinício automático nem intervenção no serviço.
- Durante a entrada em suspensão, a Wi-Fi ficou `Dormant` e o serviço reportou `Attention`, com a política não aplicada. No retorno imediato, esse dado ainda estava vencido e não foi tratado como confirmação atual. O novo retorno `Ready`, com somente a política IPv4 do OneDrive aceita, foi publicado às **05:45:44,949**, **5,14 segundos** após o `WakeTime`; a primeira amostra recente correspondente foi às **05:45:46,186**. Houve quatro amostras `Attention` no conjunto, incluindo as leituras vencidas na retomada.
- A amostra estabilizada das **05:46:16** confirmou retorno recente, as duas placas conectadas e os mesmos endereços, rotas padrão, métricas automáticas (`25` Ethernet / `35` Wi-Fi) e hash de regras da referência anterior à suspensão. As opções temporárias IPv4/IPv6 continuavam ativas, como esperado enquanto a sessão permanece aberta; isso **não é limpeza final**.
- Essa amostra encontrou três conexões TCP/IPv4 do OneDrive na Wi-Fi `192.168.0.102`. Duas tinham horário de criação **05:45:39**, no segundo da retomada, e não existiam na referência; a terceira era de 05:44:16. Os horários das conexões têm precisão de um segundo, portanto não estabelecem ordem de milissegundos em relação ao `WakeTime`.
- Uma nova leitura pelo coletor de produção às **05:49:34** encontrou **quatro conexões TCP/IPv4 do OneDrive na Wi-Fi**. O smoke test `WindowsConnections` passou (1 aprovado), com leitura em 49 ms. Isso comprova uma nova leitura nativa após retomar, mas não o avanço visual da página; o auxiliar continuava vivo, sem linha de conexão estabelecida nessa leitura.

**16 verificações de recuperação aprovadas, nenhuma falha**, registradas em `artifacts/sleep-resume/20260909-053110/recovery-review.json`, gerado por `review.ps1`. Estado intermediário: `RecoveryVerifiedAwaitingUiAndNormalStop`, preservado como registro anterior ao encerramento; a conclusão está em `result.json`. Fontes: `observation-result.json`, as capturas `after-resume-20260909-054541-588` e `after-resume-20260909-054616-541` (rede e energia), `connections-after-resume.json` e `test-results/connections-after-resume.trx`. O observador encerrou normalmente às 05:46:16; não há nova suspensão nem coleta recorrente agendada.

Nessa etapa ainda faltavam a captura de **Conexões** com leitura posterior à retomada e a parada normal pela UI proprietária, seguida do recibo de restauração e da conferência independente de processo/opções/configuração. Como o controle de desktop estava indisponível, essas duas interações foram solicitadas ao usuário, dentro da autorização já concedida e sem iniciar outro ciclo. O serviço não foi encerrado à força.

### Conclusão da suspensão/retomada e limpeza final

O usuário enviou as duas capturas e informou **“feito”**. Em **Conexões**, a leitura às **05:58:36** mostra **Serviço ativo**, pesquisa `onedrive`, filtro **Todos os processos** e `OneDrive.exe` PID `19836`, com o caminho completo esperado. A regra Wi-Fi aparece conectada e aceita; a saída observada é **Wi-Fi · 192.168.0.102**, com **uma conexão TCP/IPv4**. O horário avançou em relação à captura anterior à suspensão (05:35:10), e a linha está legível após a retomada. A contagem de uma conexão às 05:58 não contradiz as quatro às 05:49: são amostras de instantes diferentes, não um valor fixo esperado.

A captura seguinte, de **Diagnóstico**, mostra **Serviço parado** e o recibo **“Sessão encerrada. Opções temporárias restauradas.”**, com **Iniciar serviço** disponível e **Parar/Reiniciar** desabilitados. O caminho do executável é o serviço irmão do build `resume-20260909`; o último retorno aparece às **05:58:48**. O estado sem confirmação ativa após a parada é esperado, não uma falha de recuperação.

A consulta independente das **05:59:21**, repetida na finalização às **06:01:57**, confirmou:

- Nenhum `NetLane.Service` ativo; a instância `32696` saiu. O retorno correspondente está em `Stopped`, publicado às **05:58:48,002**, com regras vazias, revisão correta e sem erro.
- `routepolicies` **IPv4 e IPv6 desativados**, iguais à referência anterior ao início da sessão.
- Hash das regras idêntico ao original. Endereços IPv4, rotas padrão e métricas iguais à referência deste ciclo: Ethernet `192.168.15.4` / métrica `25`, Wi-Fi `192.168.0.102` / métrica `35`, ambas em modo automático e conectadas.
- A mesma UI `30844` permaneceu aberta; os processos do OneDrive `19836` e `21976` conservaram identidade, caminho e horário de criação. Último boot inalterado, sem erro de coleta. Nenhum arquivo sincronizado foi manipulado.

**Ciclo concluído: 28 verificações aprovadas, nenhuma falha.** São as 16 verificações anteriores de recuperação mais 12 de conferência visual/encerramento; não são 28 testes automatizados adicionais. O relatório final `artifacts/sleep-resume/20260909-053110/result.json`, gerado por `finalize.ps1`, está em **`Completed`**, sem etapa pendente deste ciclo. Fontes finais: `ui-final-review.json` (transcrição das capturas do usuário), `stopped-20260909-060157-883.json` e seu arquivo `.power.json`. A primeira conferência `stopped-20260909-055921-435.json` também foi preservada.

A diferença histórica da métrica Wi-Fi `55 → 35` pertence à referência do ensaio anterior de desconexão; neste ciclo de suspensão, a referência e o estado final são `35`. Não houve alteração de código do motor, nova compilação, instalação, commit/push ou novo ensaio disruptivo nesta conclusão. A árvore de trabalho anterior foi preservada.

Limites: revisão visual básica e atualização da leitura após retomar confirmadas; as imagens não distinguem atualização pelo temporizador, navegação ou botão, nem provam operação contínua do temporizador, tooltips, filtro de regra direta, teclado ou todos os tamanhos de janela. A prova de rede permanece limitada às conexões TCP/IPv4 estabelecidas do `OneDrive.exe`, sem afirmar roteamento dos auxiliares, UDP/QUIC, IPv6, bytes transferidos, integridade de arquivos ou enumeração independente das políticas WFP. O recibo visual, o processo, as opções globais e a configuração foram conferidos por fontes separadas.

## Preparação em 2026-09-07, 22:15 (America/Cuiaba)

- UI nova aberta sem elevação: PID `37916`, build `artifacts/process-connections/bin/NetLane.UI/release/NetLane.UI.exe`.
- Nenhum `NetLane.Service` ativo; consulta `--check-routing` confirmou `routepolicies` IPv4 e IPv6 desativados.
- Arquivo de regras intacto, SHA-256 `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`: somente OneDrive habilitado na Wi-Fi; Steam desabilitado.
- Rotas padrão preservadas: Ethernet, gateway `192.168.15.1`, métrica de interface `25`; Wi-Fi, gateway `192.168.0.1`, métrica `35`; métrica de rota `0` nas duas.
- **43 testes selecionados aprovados**, sem falhas ou ignorados: sessões, controle de serviço, restauração temporária e worker. Resultado: `artifacts/process-connections/test-results/recovery-preflight.trx`. São testes sintéticos de controle/roteamento, não ensaios elevados reais.
- A árvore de acessibilidade da janela nova respondeu e mostrou as quatro seções, mas a ativação falhou duas vezes com `failed to activate captured window`. Havia um jogo em primeiro plano. Nenhum comando de início/parada/reinício, regra ou ação no jogo foi acionado; a conferência interativa de Conexões continua pendente.

## Ordem e critérios

1. **Iniciar → reiniciar → parar o serviço, mantendo ambas as placas conectadas.** O usuário autoriza a ativação temporária de `routepolicies` e os pedidos de UAC. Verificar o serviço do mesmo build, retorno recente da sessão e apenas a regra esperada. No reinício, confirmar saída do PID anterior, novo PID e novo retorno; não aceitar duas instâncias simultâneas. Na parada final, exigir recibo de limpeza, processo ausente, opções restauradas e regras/rotas/métricas iguais à referência. Se faltar recibo, não repetir o início nem forçar encerramento.
2. **Perda e retorno da Wi-Fi, em outro momento autorizado.** Antes, confirmar que a interrupção não prejudicará trabalho, jogo ou acesso remoto. Observar retirada da política quando a placa ficar indisponível e reaplicação após seu retorno, preservando a configuração. Ausência da política libera a escolha do Windows: não representa bloqueio do app. Novas conexões precisam ser diferenciadas das já abertas.
3. **Suspensão e retomada, com autorização separada.** O usuário suspende/retoma manualmente. Registrar se a sessão permaneceu saudável ou se encerrou com limpeza por perda de keepalive; não presumir persistência automática. Exigir nova amostra de conexões e heartbeat recente, sem tratar dados anteriores à suspensão como atuais.

Os ensaios não reiniciam o Windows, não fecham OneDrive/Steam, não escolhem pastas nem alteram arquivos sincronizados. Se for necessário renovar conexões do OneDrive para medir a saída, esse passo será combinado separadamente. Sucesso do controle do serviço não certifica tráfego de todos os auxiliares, UDP/QUIC, IPv6 ou integridade dos arquivos.

## Ciclo real iniciar → reiniciar → parar — concluído

Após a autorização do usuário, a janela PID `37916` foi restaurada e conferida: nenhuma edição pendente, Steam desabilitado, OneDrive habilitado na Wi-Fi e serviço irmão do build `process-connections`. A opção temporária foi marcada e os botões Iniciar/Reiniciar foram acionados no painel. As confirmações já haviam desaparecido quando a automação tentou seus índices; nenhum comando foi enviado ao UAC.

- **Início:** serviço PID `23264`, criado às 23:45:54; pai `37916`, retorno recente `Ready` e apenas a política IPv4 do OneDrive aceita. Ambas as opções `routepolicies` estavam ativas. A UI mostrou “Sessão ativa, controlada por esta janela.”
- **Reinício:** serviço PID `35764`, criado às 23:48:07, com o mesmo pai e novo retorno `Ready`. Na consulta das 23:48:55, o PID anterior já não existia e havia somente uma instância do serviço. Regras mantiveram o hash original. As verificações são amostras: não capturaram o estado transitório das opções entre as sessões.
- **Parada manual:** o clique automatizado foi recusado porque a janela estava minimizada. A tentativa de recuperá-la detectou interação manual e a captura seguinte ainda informou janela minimizada. Às 23:51:06 a sessão permanecia ativa, e foi solicitado ao usuário usar **Diagnóstico → Parar**, sem encerramento forçado. Após sua confirmação, o registro passou a `Stopped` às **23:52:28**, com lista de políticas vazia e sem erro.
- **Limpeza final:** a leitura nativa das 23:53:04 e a comparação automatizada das **23:54:55** confirmaram ausência de qualquer `NetLane.Service`, saída dos PIDs `23264`/`35764`, `routepolicies` desativado nas duas famílias e regras/rotas padrão/métricas idênticas à referência inicial. A árvore de acessibilidade da janela controladora mostrou **“Sessão encerrada. Opções temporárias restauradas.”**, com Parar/Reiniciar desabilitados. As oito verificações finais passaram. A UI permaneceu aberta; não foi iniciada outra sessão.

Evidências locais em `artifacts/service-restart/20260907-234341/`: `baseline.json`, `started.json`, `restarted.json`, `progress.json` e **`result.json`**. Nenhuma placa ou processo do OneDrive/Steam foi alterado pelo ensaio, que não refez a prova de tráfego. A consulta nativa sem elevação não expôs o caminho do serviço elevado; o build foi conferido no campo da UI, junto ao PID autenticado e ao pai observado pelo Windows. Os 43 testes de preparação são distintos destas oito verificações reais; não houve nova alteração de código ou repetição da suíte nesta conclusão.
