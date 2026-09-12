# Controle do serviço pela interface

Entrega local de desenvolvimento de 2026-09-07. Não instala serviço permanente, tarefa agendada, inicialização automática ou pacote de distribuição. A Microsoft Store está fora desta etapa.

Estado atualizado em 2026-09-09: início elevado pelo painel, download do OneDrive com conexões TCP/IPv4 observadas na Wi-Fi, conclusão informada pelo cliente/usuário e parada normal com restauração real das opções temporárias validados. O [ciclo real de iniciar, reiniciar e parar](testes-recuperacao.md), a perda/retorno da Wi-Fi e a suspensão/retomada foram concluídos com limpeza final confirmada. Conexões teve revisão visual básica antes/depois da suspensão. Os registros abaixo preservam a sequência do desenvolvimento e dos ensaios, sem ampliar essa prova a todo o tráfego ou a todos os cenários de energia.

## Uso

1. Compile toda a solução em Release e abra o painel sem elevação.
2. Salve as regras. No teste atual, mantenha somente **OneDrive → Wi-Fi** habilitado; não habilite o Steam.
3. Em **Diagnóstico → Controle do serviço**, marque a autorização de `routepolicies` temporário somente se concordar com essa alteração global do Windows. A opção começa desmarcada.
4. Clique em **Iniciar serviço**, revise a confirmação e aprove o UAC. Se as opções estiverem desativadas e não houver autorização, o serviço recusa o início.
5. Aguarde o retorno da sessão e a política do OneDrive aceita. Uma sessão ativa com regras em atenção não confirma o roteamento.
6. Reabra o OneDrive normalmente para renovar conexões e inicie a sincronização. Mantenha o painel aberto durante todo o ensaio.
7. Use **Parar** e aguarde o resultado da limpeza. **Reiniciar** primeiro para e só abre outra instância se a limpeza for confirmada. Fechar a janela também solicita a parada.

O painel não fecha nem reabre aplicativos, não escolhe pastas e não altera os arquivos sincronizados. Regras salvas continuam no JSON após a parada; sem o motor, novas conexões voltam à escolha normal do Windows. O gráfico continua medindo a interface inteira, não o tráfego individual do OneDrive.

## Fronteira de privilégio

- O processo gráfico continua sem elevação. Somente `NetLane.Service.exe` é solicitado com `runas`, em janela oculta. O executável é localizado no build correspondente ou ao lado da UI; não vem do JSON de regras.
- A UI cria um named pipe de nome aleatório, primeira instância exclusiva, ACL restrita ao usuário e administradores, com acesso de rede negado. Ambos os lados verificam o PID do outro pelo Windows; o serviço também verifica o horário de início da janela controladora, impedindo confusão por reutilização de PID.
- O cliente privilegiado usa nível de impersonação `Anonymous`, impedindo a UI de assumir seu token. `CurrentUserOnly` não é usado: ele também compara elevação e impediria a passagem normal pelo UAC.
- O protocolo permite apenas início com arquivo local existente, keepalive e parada. Não há execução remota de comandos, caminhos executáveis recebidos por IPC ou controle de PIDs arbitrários. Mensagens têm tamanho máximo de 64 KiB e prazos de conexão/leitura/escrita.
- A UI sem elevação detém a trava do arquivo de estado e grava o heartbeat informativo. O worker elevado da sessão lê regras e publica retornos pelo canal, sem bootstrap ou gravações no diretório de configuração do usuário.
- Uma trava de sessão global e a verificação de processos impedem dois motores NetLane desta versão de operar simultaneamente. Nenhum serviço externo é encerrado; pare-o pelo controlador que o iniciou.

Referências Microsoft: [opções de pipe e comparação da elevação](https://learn.microsoft.com/en-us/dotnet/api/system.io.pipes.pipeoptions), [criação com ACL explícita](https://learn.microsoft.com/en-us/dotnet/api/system.io.pipes.namedpipeserverstreamacl.create), [PID do servidor](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid) e [PID do cliente](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid).

O checkout e seus binários continuam sendo um ambiente de desenvolvimento controlado pelo usuário. Isso não substitui instalador assinado, diretórios protegidos ou uma revisão de segurança de distribuição.

## Alterações temporárias e recuperação

O controlador lê primeiro as duas opções `routepolicies` (IPv4/IPv6). Estado desconhecido impede a ativação. Com autorização explícita, habilita somente as opções inicialmente desativadas, usando exclusivamente `store=active`; verifica cada alteração e guarda quais opções precisa restaurar. As opções já habilitadas antes da sessão não são desabilitadas na saída. Não altera gateway, DNS, métricas, regras de firewall ou estado das placas.

O encerramento normal interrompe o motor, libera sua sessão WFP, restaura as opções que habilitou e envia o resultado ao painel. Falhas na ativação parcial também passam pela restauração. Se o painel desaparecer, o serviço detecta a saída do processo, a desconexão do canal ou a ausência de keepalive por 20 segundos e inicia a mesma limpeza.

Limites importantes:

- Não use encerramento forçado do serviço como procedimento normal. Se o processo privilegiado for morto, seu código de restauração não poderá executar. A sessão dinâmica WFP desaparece com o processo, mas as opções globais ativas podem permanecer até recuperação manual ou reinicialização do Windows.
- Saída do processo sem recibo de limpeza não é apresentada como restauração confirmada. A janela recusa outro início nessa condição; confira o estado atual e só depois reabra o painel. **Copiar diagnóstico** inclui o resultado do controle da sessão.
- O estado de `routepolicies` é global. Não execute o script de ensaio, outra versão do NetLane ou ferramentas que alterem essas opções em paralelo. Não é possível atribuir mudanças feitas por terceiros durante a sessão.
- A sessão depende do processo do painel. Na [bandeja do Windows](bandeja-windows.md), minimizar apenas esconde a mesma janela e mantém a sessão; **X** e **Sair** continuam usando a parada e as confirmações existentes. Execução após encerrar o processo e inicialização automática não fazem parte desta entrega. Um ciclo real de suspensão/retomada preservou a sessão em 2026-09-09; isso não garante todos os cenários de energia, e keepalive vencido pode encerrar a sessão por segurança.
- Um serviço externo iniciado pelo terminal permanece externo: o painel só envia comandos ao processo que ele próprio iniciou. O modo CLI comum não habilita `routepolicies` automaticamente.

## Verificação

Build isolado, sem sobrescrever uma UI que já esteja aberta:

```powershell
dotnet build .\NetLane.sln -c Release --artifacts-path .\artifacts\service-control
```

```powershell
dotnet test .\tests\NetLane.Tests -c Release --artifacts-path .\artifacts\service-control --no-build
```

A suíte inclui protocolo e limites, ACL/PIDs reais em pipes locais, processo auxiliar sintético (início/parada/reinício), cancelamento simulado do UAC, disputa de trava, saída abrupta, falha de limpeza, preservação das opções já habilitadas, rollback parcial, ausência de gravações elevadas do worker e controles WPF. O auxiliar `NetLane.ControlProbe` não executa `netsh`, não instancia WFP e não solicita elevação.

Esses testes sintéticos **não** comprovam, isoladamente, a passagem pelo UAC real, restauração real de opções do Windows ou sincronização completa do OneDrive. A validação real separada, registrada abaixo, observou PID/heartbeat, política, conexões TCP do OneDrive na Wi-Fi, progresso/conclusão e limpeza final.

Validação local desta entrega: build Release isolado com **0 avisos/erros**, **146 testes aprovados** e renderizações WPF nos tamanhos 1000×650, 1220×810 e 1440×920. O JSON real de regras permaneceu idêntico, com Steam desabilitado e OneDrive na Wi-Fi. A consulta `--check-routing` confirmou API disponível, processo não elevado e `routepolicies` desativado em ambas as famílias. O serviço real ainda não foi iniciado pelo novo painel.

A janela real do build isolado foi aberta e conferida visualmente e pela árvore de acessibilidade do Windows: **Iniciar serviço** disponível, **Parar/Reiniciar** desabilitados e autorização temporária desmarcada. Foi deixada em Diagnóstico para a validação manual do UAC; nenhum desses comandos privilegiados foi acionado na revisão visual.

## Retomada real e correção de seleção do build — 2026-09-07

Na retomada, a Wi-Fi estava desconectada. Após a reconexão pelo usuário, voltou a `192.168.0.102`; o OneDrive permaneceu fechado. A tentativa de início pelo painel criou o PID `9908`, às 16:00:47 (America/Cuiaba), mas o handshake expirou e a parada pelo novo canal também expirou.

A causa foi localizada no seletor do executável: o caminho real da UI foi recebido como `...\bin\netlane.ui\release`, mas o código comparava o nome da pasta com `NetLane.UI` distinguindo maiúsculas/minúsculas. Isso ignorou o serviço do build isolado e selecionou a versão antiga em `src/NetLane.Service/bin/Release/net8.0-windows`, compilada às 08:14:43. O heartbeat desse diretório identificou o mesmo PID `9908`, estado `Attention`, política do OneDrive **não aplicada** e ausência dos pré-requisitos globais. `routepolicies` permaneceu desativado em IPv4/IPv6. O processo antigo não implementa o protocolo de controle da UI nova.

A correção compara nomes sem distinção de caixa. Uma UI no layout `artifacts` agora exige o serviço irmão do mesmo build; se ele estiver ausente, não recorre a outra versão. O diagnóstico exibe o caminho do executável e deixa de reaproveitar heartbeat de outra sessão enquanto aguarda a resposta da instância controlada. Mensagens de timeout passaram a explicar o problema em português, sem afirmar que o processo está encerrando quando ele apenas permanece aberto.

Os testes de regressão primeiro reproduziram quatro falhas (caixa do caminho e fallback indevido). A validação elevada real do build corrigido e a sincronização do OneDrive ainda estão pendentes. Foi solicitado ao usuário encerrar somente a instância antiga identificada, antes de novo UAC. Nenhuma regra real foi alterada e nenhum processo do Steam/OneDrive foi aberto ou encerrado por esta correção.

O build corrigido foi gerado isoladamente em `artifacts/service-control-fix`, com **0 avisos/erros** e **152 testes aprovados**. A instância antiga e a janela já aberta não foram sobrescritas. A troca da janela e a repetição do ensaio dependem de confirmar a saída do processo antigo.

Após o usuário confirmar o encerramento, a consulta verificou que o PID `9908` não estava mais ativo, a Wi-Fi estava conectada com `192.168.0.102` e `routepolicies` permanecia desativado nas duas famílias. A janela antiga foi fechada normalmente, após conferir que não havia regras pendentes de salvamento. Às 16:26, o painel de `artifacts/service-control-fix` foi aberto e o campo **Executável do serviço desta sessão** confirmou o serviço irmão correto em `artifacts/service-control-fix/bin/NetLane.Service/release/NetLane.Service.exe`, mesmo com o caminho base em minúsculas. A autorização temporária ficou desmarcada e o serviço parado, aguardando novo UAC manual. O JSON real manteve o mesmo hash e o OneDrive continuou fechado.

## Início real confirmado pelo painel corrigido — 2026-09-07

Após a autorização manual do usuário, o serviço PID `3748` iniciou às 16:35:19 (America/Cuiaba). O heartbeat da sessão passou a `Ready`, com somente a política IPv4 do OneDrive aceita na Wi-Fi; a consulta dos pré-requisitos confirmou `routepolicies` ativo nas duas famílias. A janela controladora é o PID `31840`, do build `artifacts/service-control-fix`. Isso confirma o início real da sessão; a mensagem de política aceita continua sem certificar tráfego por si só.

O OneDrive foi aberto sem elevação às 16:37:51, PID `37672`, depois da confirmação da política. Oito amostras, até 16:44:03, registraram **18 conexões TCP externas distintas**, todas com origem `192.168.0.102` e destino na porta `443`. Todas foram criadas após a confirmação; nenhuma conexão externa desse PID foi observada pela Ethernet. O heartbeat permaneceu recente e o processo do serviço estava ativo na última amostra. A configuração real manteve o hash `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`, com Steam desabilitado.

Evidência local: `artifacts/onedrive-ui/20260907-163733/progress.json`. O ensaio permanece **em andamento**: a seleção Wi-Fi para o TCP/IPv4 observado está comprovada, mas a transferência completa das pastas e a restauração real ao usar **Parar** ainda não foram verificadas. A sessão foi mantida ativa para o teste de sincronização do usuário; não foi executado o script de ensaio em paralelo. Contadores de bytes da placa não foram atribuídos ao OneDrive.

Às 21h, após o usuário iniciar a transferência, o resumo do OneDrive no Explorador mostrou **Baixando arquivos (21%)**, avançando para **22%**. Duas novas rodadas, entre 21:08:36 e 21:11:42, registraram 14 amostras e 11 conexões TCP externas distintas do PID `37672`, todas com origem na Wi-Fi. O mesmo serviço PID `3748` estava ativo e com heartbeat recente em cada amostra. Isso valida progresso real de download durante a política, não a conclusão das pastas. A sessão foi mantida ativa; parada/restauração reais continuam pendentes. A lacuna de observação desde a coleta das 16h está explícita no relatório e não é tratada como monitoramento contínuo.

## Sincronização concluída e parada real validada — 2026-09-07

O usuário enviou a confirmação **Todos baixados** e uma captura do OneDrive com **Incluído no backup e sincronizado**. Antes de parar, a consulta às 21:23:45 ainda encontrou a instância PID `3748`, heartbeat recente `Ready`, somente OneDrive aceito e ambas as opções `routepolicies` ativas. A coleta totalizou 23 amostras e 31 conexões TCP externas distintas do OneDrive, todas na Wi-Fi. O teste não inspecionou conteúdo dos arquivos nem capturou todo o tráfego de forma contínua.

A parada foi solicitada uma vez pelo botão da própria janela controladora, sem nova elevação e sem encerramento forçado. O recibo `Stopped` foi publicado às **21:24:38** (America/Cuiaba), sem regras e sem erro. O painel mostrou **Sessão encerrada. Opções temporárias restauradas.** Na leitura posterior, o PID `3748` já não existia e `routepolicies` estava novamente desativado nas duas famílias. Rotas padrão e métricas permaneceram iguais ao estado anterior à parada; o arquivo real de regras manteve o hash original.

A UI e o OneDrive permaneceram abertos, e o Steam não foi alterado. Novas conexões deixam de ter a política do NetLane; conexões já estabelecidas podem continuar na interface anterior. A janela ficou em Diagnóstico, com **Iniciar serviço** disponível e **Parar/Reiniciar** desabilitados. A autorização temporária continua marcada nesta janela, conforme a escolha anterior do usuário; não foi iniciada outra sessão.

Relatório final: `artifacts/onedrive-ui/20260907-163733/result.json`. Este resultado valida início, sincronização no escopo observado e parada normal, mas não substitui os ensaios reais de reinício, desconexão da Wi-Fi ou suspensão/retomada.

Após o ensaio real, a suíte foi repetida sobre o build isolado corrigido, sem recompilar os executáveis abertos: **152 testes aprovados, 0 falhas, 0 ignorados**, em 27 segundos. Resultado: `artifacts/service-control-fix/test-results/service-control-after-real-test.trx`. A conferência `git diff --check` também passou; o registro da validação não alterou o código do motor.

## Recuperação da Wi-Fi e parada confirmadas — 2026-09-09

O [ensaio de perda e retorno da Wi-Fi](testes-recuperacao.md#perda-e-retorno-da-wi-fi--concluído-com-limpeza-final) conservou a mesma sessão, retirou e reaplicou a política e observou uma nova conexão TCP/IPv4 do OneDrive na Wi-Fi. A parada manual publicou `Stopped` às 05:21:26; a captura enviada pelo usuário mostrou o recibo de restauração e o serviço irmão correto do build `resume-20260909`.

As 18 verificações do ensaio passaram, incluindo ausência do serviço e `routepolicies` IPv4/IPv6 desativados, regras intactas e retorno dos endereços/rotas padrão. A métrica automática da Wi-Fi já havia passado de 55 para 35 antes da interrupção e permaneceu 35 até o fim; essa diferença ficou explícita no resultado. A UI permaneceu aberta, com o serviço parado. Nesse ponto, suspensão/retomada e a conferência interativa da página Conexões ainda estavam pendentes; foram realizadas no ciclo seguinte.

## Sessão preservada após suspensão — 2026-09-09

No ensaio seguinte, já autorizado, os eventos do Windows confirmaram a suspensão/retomada sem reboot. A nova sessão `32696`, filha da mesma UI `30844`, permaneceu ativa nas 175 amostras. Houve `Attention` durante a transição e retorno inicialmente vencido; a política IPv4 do OneDrive voltou a `Ready` em retorno recente publicado 5,14 segundos após o horário de retomada. Endereços, rotas, métricas e regras foram preservados. As 16 verificações de recuperação passaram, e uma leitura nativa posterior encontrou quatro conexões TCP/IPv4 do OneDrive na Wi-Fi.

A revisão visual básica de Conexões foi concluída antes e depois da suspensão: a imagem posterior mostra leitura às 05:58:36, serviço ativo e o OneDrive na Wi-Fi. A parada normal seguinte publicou `Stopped` às 05:58:48, com o recibo de restauração visível. A conferência independente final às 06:01:57 confirmou saída do serviço `32696`, `routepolicies` IPv4/IPv6 desativados, regras/endereço/rotas/métricas preservados e a mesma UI/OneDrive abertos. As opções estavam ativas na amostra estabilizada de recuperação e foram restauradas pela parada; são estados de etapas diferentes.

**Ciclo concluído com 28 verificações aprovadas**, sendo 16 de recuperação e 12 de conferência/encerramento. Evidência final: `artifacts/sleep-resume/20260909-053110/result.json`, estado `Completed`. Veja o [registro completo](testes-recuperacao.md#conclusão-da-suspensãoretomada-e-limpeza-final).

## Proteção de edições durante o fechamento — corrigida em 2026-09-09

A revisão final identificou um caso P2: após confirmar parar/fechar a janela, o editor continuava disponível durante a parada assíncrona, mas uma edição feita nessa espera era descartada sem nova confirmação. O usuário autorizou a correção separadamente.

O editor agora mantém uma versão das alterações locais. O fluxo registra a versão já autorizada antes de aguardar a parada; se surgirem novas edições ainda não salvas, pede confirmação de descarte novamente antes de fechar. Atualizações de adaptadores e retornos do serviço não contam como edição.

- Recusar o novo descarte mantém a janela aberta em **Regras de apps**, com as edições preservadas e o serviço já parado.
- Alterações anteriores já autorizadas não geram pergunta duplicada; salvar durante a espera também permite fechar sem novo descarte.
- Uma parada com falha mantém a janela aberta em **Diagnóstico**, com o editor disponível e as alterações preservadas para salvar ou tentar novamente.
- A autorização interna para concluir o fechamento vale somente para aquela chamada de `Close`, sem permanecer ativa para outra tentativa.

O comportamento foi validado com janelas WPF em memória, arquivos descartáveis e serviço simulado, sem abrir a janela real, pedir UAC ou tocar o roteamento. A confirmação de descarte é substituível somente no construtor interno usado pelos testes; a janela normal continua usando o diálogo padrão. O escopo não inclui automação do diálogo nativo.

Os oito novos cenários primeiro reproduziram **4 falhas e 4 aprovações** no comportamento anterior. Após a correção, os **8 passaram**; cobrem editor inicialmente limpo/sujo, aceitar/recusar novas edições, fechar novamente após recusar, fechamento duplicado durante a espera, ausência de nova edição, atualização somente de observação, salvar durante a espera e falha de parada com nova tentativa. A suíte completa passou com **198 aprovados, 0 falhas e 0 ignorados**, em 27 segundos. O build Release da solução teve **0 avisos e 0 erros**.

Evidências: `artifacts/close-edit-fix-20260909/test-results/closing-red.trx`, `closing-green.trx`, `close-edit-full.trx` e `artifacts/close-edit-fix-20260909/result.json`. O build corrigido está em `artifacts/close-edit-fix-20260909/bin/NetLane.UI/release/NetLane.UI.exe`, com serviço irmão do mesmo build. Não foi aberto nesta correção.

Foram alterados somente o fluxo de fechamento, a versão de edição, os testes e este registro. Regras reais, rotas e métricas preservadas; `routepolicies` IPv4/IPv6 desativados e nenhum NetLane em execução na conferência final. Nenhum arquivo sincronizado foi manipulado. Os ensaios reais de rede anteriores não foram repetidos nem apresentados como prova desta compilação. Sem instalação, staging, commit ou push.
