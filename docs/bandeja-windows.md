# Bandeja do Windows

Implementação local iniciada em 2026-09-10, depois da [validação complementar da interface](validacao-interface.md), seguindo a ordem autorizada. Não instala serviço, não adiciona inicialização automática e não aplica roteamento por abrir o painel.

**Ajuste instalado e testado em 2026-09-30:** na **0.3.2**, X oculta na bandeja, preservando a sessão e as edições; **Sair** no menu do ícone solicita encerramento explícito. Upgrade verificado às 21:19 e ícone do executável igual ao logo. Usuário confirmou **“testado, tudo certo”** depois da orientação X → bandeja/reabrir pelo ícone/Sair; confirmação dos cliques registrada como relato do usuário.

## Comportamento

- **X** e **Minimizar** escondem a janela, sem fechá-la. O mesmo editor, filtros, seleção, temporizador e sessão continuam vivos. Não salvam ou descartam edições.
- Clique esquerdo no ícone, ou **Abrir NetLane**, restaura a mesma janela no estado normal/maximizado anterior.
- O menu de contexto mostra o estado do serviço, o número de regras habilitadas e a existência de alterações não salvas. Esses dados não são prova de tráfego roteado.
- **Diagnóstico**, **Iniciar serviço**, **Parar serviço** e **Reiniciar serviço** reutilizam os controles existentes. A janela é restaurada antes da operação ou confirmação; início/reinício continuam exigindo regras salvas, confirmação e UAC manual. A bandeja não concede autorização de `routepolicies` por conta própria.
- **Sair** encerra de verdade: confirmar eventual descarte, parar a sessão própria, aguardar a limpeza e reconfirmar alterações feitas durante essa espera. Recusa ou falha mantém o painel disponível.
- Enquanto uma operação ou confirmação está em andamento, os comandos de sessão/saída e o X são bloqueados. Minimizar nesse intervalo restaura o painel para manter o proprietário dos diálogos disponível.
- Se a criação ou atualização do componente da bandeja falhar, o painel permanece ou volta à barra de tarefas, com aviso discreto. O ícone é removido e liberado no fechamento; não há um segundo processo de bandeja.

Se a bandeja estiver indisponível, **X** mantém o encerramento seguro para permitir sair pela janela. Parar o serviço remove as políticas da sessão sem apagar as regras salvas, conforme o controle já existente. Nenhuma preferência de conexão padrão é restaurada por ocultar ou sair.

## Implementação

`App` cria `WindowsTrayIcon` e `WindowTrayController` antes de exibir a janela. A janela de teste construída diretamente não ganha um ícone automaticamente. O construtor interno de sete parâmetros usado pelo host de revisão foi preservado.

O controlador separa a visibilidade da janela do ciclo de vida da sessão, reavalia permissões dos comandos ao executá-los e bloqueia reentrada durante confirmações. A implementação nativa usa o componente [NotifyIcon do .NET](https://learn.microsoft.com/en-us/dotnet/api/system.windows.forms.notifyicon?view=windowsdesktop-8.0), sem biblioteca de terceiros. O logo vetorial já existente é renderizado em memória nos tamanhos 16, 20, 24, 32, 48 e 64 para o ícone; não foi criado outro desenho ou arquivo de marca.

O modo WPF é `OnMainWindowClose`. Esconder com `Window.Hide` não fecha a janela nem provoca o encerramento por esse modo, conforme a [documentação Microsoft](https://learn.microsoft.com/en-us/dotnet/api/system.windows.window.hide). O X cancela `Closing` e agenda a ocultação no Dispatcher depois desse evento; **Sair** solicita fechamento explícito. O controlador não chama `Application.Shutdown` para contornar uma recusa de fechamento.

## Validação

### Confirmação do usuário — 2026-09-30

Após instalar, foi solicitado abrir pelo menu Iniciar e conferir **X → bandeja**, **ícone → reabrir** e **Sair → encerrar**. O usuário respondeu **“testado, tudo certo”**. Confirmação registrada às **21:25:38 (America/Cuiaba)** em `artifacts/installer-qa-20260930-032/user-ui-confirmation.json`, SHA-256 `C22391E0DCC7DEBA420DC49C289674E59AE062CA063A2D5221838A93D9FE9B4D`.

Leitura posterior confirmou registro 0.3.2 e perfil/interfaces iguais aos do upgrade. A UI instalada estava aberta/respondendo, PID histórico **19252**, sem serviço; não foi encerrada pelo agente. O relato valida o teste solicitado, mas não registra o código de saída nem a sequência de fechar/reabrir; não inferir que a janela já estava encerrada no instante da leitura. Não houve nova captura do menu Iniciar, automação de cliques, configuração de rede ou serviço nesta conferência. Os recibos de instalação/preparação permanecem intactos.

### Instalação da 0.3.2 — 2026-09-30

[Upgrade sobre a 0.3.1 concluído](instalador-local.md#atualização-para-a-032--2026-09-30), após **“vamos instalar a 0.3.2”**, com UAC manual e retorno 0. `StageVerified=true`, registro 0.3.2, 758 hashes do inventário iguais, 760 arquivos totais, atalho e fonte do ícone corretos. Ícone do executável instalado extraído sem iniciar a UI: logo correspondente, zero pixels diferentes em 32×32. Quatro arquivos do perfil e seus backups/cópias, rede/autostart e 19 arquivos protegidos preservados, Defender ativo; nenhum processo NetLane restante. Recibo `artifacts/installer-qa-20260930-032/upgrade-result.json`. A confirmação visual do menu Iniciar/X/bandeja permanece pendente; as seções seguintes preservam a preparação anterior.

### Ajuste do X — 2026-09-30

Build/testes isolados em `artifacts/close-to-tray-20260930/`. Suíte final **245/245 aprovações**, sem falhas/ignorados, recibo `test-results/close-to-tray-regression.trx`; compilação sem avisos/erros. Verificados X/restauração normal e maximizada com sessão ativa/edições não salvas, ausência de descarte/parada ao ocultar, saída explícita, recusa de saída, novas edições durante a parada, cancelamento de uma ocultação agendada ao solicitar Sair, bloqueio durante operações e saída pelo X após falha da bandeja. As dependências de sessão e ícone nesses testes são sintéticas.

Prévia final **0.3.2** em `artifacts/installer/preview-20260930-close-to-tray-v2/NetLane-0.3.2-preview-win-x64-setup.exe`, **69.485.432 bytes**, SHA-256 `FA04847A5CF7E79662288F1F35878FB4C68E99E4E178DA82AF9737E4763EF30C`. Inclui a correção do ícone genérico do aplicativo no menu Iniciar apontada pelo usuário. Validação estática aprovada: **759 arquivos de payload**, 758 entradas do inventário, UI sem elevação, instalador administrativo, ausência de dados pessoais/regras e ícone do executável extraído igual ao logo em todos os pixels de 32×32. Instalador sem assinatura e ainda não executado; a instalação continua **0.3.1**.

Consolidação em `artifacts/close-to-tray-20260930/implementation-review.json`: hashes das fontes/pacote/testes, perfil e interfaces iguais ao início do preparo, regras do checkout preservadas e Defender ativo. Atualização solicitada separadamente ao usuário porque a autorização anterior nomeava a 0.3.1; não iniciar o instalador sem a resposta. Cliques reais no X/menu após instalar a 0.3.2 ainda não verificados. Nenhuma alteração de rede, serviço, commit/push ou distribuição durante este preparo.

Consolidação do v2 em `artifacts/app-icon-20260930/implementation-review.json`: logo existente da janela renderizado em `Assets/netlane.ico`, com 16/20/24/32/48/64/128/256 pixels e transparência, incorporado pelo `ApplicationIcon` do projeto. O atalho instalado já apontava para `NetLane.UI.exe,0`; a ausência do recurso no executável explicava o ícone genérico. A extração do executável instalado confirmou a diferença; a do pacote novo confirmou o logo. Não houve gravação no cache de ícones, no atalho instalado ou na rede. Os 245 testes pertencem à mudança de fechamento anterior à adição do recurso de ícone; a lógica da UI não mudou depois deles. Build/publicação final sem avisos/erros; validação estática com o novo ícone aprovada. A primeira preparação sem `-v2`, hash `1412BCF28E5217325805C588368788C81E3C23CA60A76F86AC7CD148C6B1D065`, permanece como histórico anterior à correção do ícone; usar somente o v2 para atualizar.

Para regenerar o ICO a partir do mesmo vetor: `powershell.exe -NoProfile -STA -File scripts/generate-app-icon.ps1`. `scripts/test-app-icon.ps1` lê o ícone de um executável sem iniciá-lo. A validação estática do instalador usa essa comparação para prévias 0.3.2 em diante. O menu Iniciar real após instalar ainda depende de conferência visual.

### Histórico da implementação anterior — 2026-09-10

Build isolado em `artifacts/tray-20260910`, sem substituir os binários dos ensaios anteriores:

```powershell
dotnet build NetLane.sln -c Release --artifacts-path artifacts/tray-20260910
dotnet test tests\NetLane.Tests -c Release --artifacts-path artifacts/tray-20260910 --no-build
```

- Primeira compilação: **0 erros e 0 avisos**.
- Testes focados finais: **17 aprovados**, resultado `artifacts/tray-20260910/test-results/tray-focused-final.trx`.
- O primeiro teste focado teve 16 aprovações e uma falha na preparação da janela invisível maximizada, por uma restrição de `ShowActivated=false`. A preparação foi corrigida e os 17 cenários passaram; o resultado inicial foi preservado.
- Suíte completa: **216 aprovados, 0 falhas e 0 ignorados**, em 45 segundos; resultado `artifacts/tray-20260910/test-results/tray-full.trx`. Inclui os cenários de fechamento e o temporizador acrescentado na etapa anterior.

Os cenários novos verificam minimizar/restaurar normal e maximizado, preservação de regras/edições/filtros/sessão, confirmação antes de início/reinício, autorização temporária não inferida, rejeição de comandos duplicados ou obsoletos, edição durante a saída, recusa de descarte, falha de parada e retry, falha da bandeja com recuperação do painel, liberação única de recursos e atualização de Conexões enquanto oculto.

Os testes WPF usam janelas transparentes, fora da área visível, sem ativação ou entrada na barra de tarefas; o componente de bandeja é simulado. Um teste separado instancia os controles nativos com `NotifyIcon.Visible=false`, verifica a projeção do menu e a geração do logo, sem registrar um ícone no shell. Esses testes não comprovam, sozinhos, cliques reais na bandeja, o menu no desktop, escalas de monitores, reinício do Explorer nem continuidade de tráfego com serviço elevado.

### Abertura do build para conferência

A UI de `artifacts/tray-20260910/bin/NetLane.UI/release/NetLane.UI.exe` foi aberta sem elevação às **11:23:21**, PID histórico `33240`, depois de verificar que não havia outro processo `NetLane.UI`. Às **11:23:46**, havia um único painel desse build, respondendo com o título **NetLane — Cada app, sua rota**, e nenhum `NetLane.Service`. O hash das regras reais permaneceu `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.

Hash SHA-256 da nova DLL de UI: `2F63255A2CB23EF38D12A9D5FD24921B38B282C5E802858C9115F38A0C4B9EF0`. Essa é uma compilação diferente da DLL da validação anterior. O host fictício de fechamento permaneceu separado, sem substituir seus binários ou encerrar sua edição pendente. A existência de um processo respondendo não comprova o ícone ou menu no desktop; a conferência abaixo continua necessária.

### Resultado da conferência básica

O usuário enviou duas capturas e informou **“Deu certo”** após o roteiro de minimizar/restaurar. A primeira mostra o ícone N do NetLane na área de ícones ocultos do Windows. A segunda mostra o menu nativo com **Serviço parado**, **Regras habilitadas: 1 de 2**, **Abrir NetLane**, **Diagnóstico**, **Iniciar serviço…**, **Parar serviço**, **Reiniciar serviço…** e **Sair**. Parar e Reiniciar aparecem indisponíveis; não há indicação de alterações não salvas. O estado e a contagem são coerentes com as regras preservadas.

A presença do ícone e a apresentação do menu são comprovadas pelas imagens; minimizar/restaurar fica confirmado pelo relato do usuário, não pela imagem estática isolada. Não se presume que ele tenha repetido os dois estados normal/maximizado ou o descarte pelo menu Sair. Esses casos têm cobertura automatizada, mas não foram capturados individualmente no desktop. Reinício do Explorer, outras escalas e roteamento real enquanto minimizado também não foram declarados validados.

A consulta posterior encontrou o mesmo painel respondendo, sem processo `NetLane.Service` e sem host `NetLane.CloseReview`. O relatório do ensaio anterior registra `WindowClosed` às **11:25:43**, editor sem alterações, regra fictícia habilitada, arquivo sintético inalterado e sessão simulada encerrada. As regras reais mantiveram o hash de referência. Nenhum código foi alterado nesta confirmação e os 216 testes não foram repetidos sem necessidade.

A revisão básica da bandeja está concluída nesse escopo. Naquele ponto, a próxima etapa era a [ampliação dos ensaios de rede](ampliacao-ensaios-rede.md), ainda sem ativação do serviço ou alterações de conectividade.

Em **2026-09-12 às 07:56:33**, o [ensaio separado de continuidade com roteamento](ensaio-bandeja-roteamento.md#resultado-completo-da-rodada-de-30-minutos) foi concluído com início/UAC e cliques manuais. Seis novas conexões do probe confirmaram Ethernet → Wi-Fi com janela visível/oculta/restaurada → Ethernet após parada, usando a mesma janela e serviço nas fases ativas. Fechamento, limpeza e preservação da rede/regras verificados. O resultado é limitado a handshakes QUIC/IPv4 isolados; não amplia a revisão visual de escalas/teclado/Explorer nem comprova tráfego sustentado ou de aplicativos reais.

## Roteiro manual de referência, sem iniciar roteamento

1. Abrir a UI desse build e verificar o ícone NetLane na área de notificações ou em seus ícones ocultos.
2. Minimizar e clicar no X: o painel deve desaparecer da barra de tarefas; clicar no ícone deve restaurá-lo. Repetir partindo da janela maximizada.
3. Abrir o menu com o botão direito e conferir estado, contagem, **Abrir NetLane**, **Diagnóstico** e **Sair**. Com o serviço parado, **Parar** e **Reiniciar** ficam indisponíveis.
4. Conferir que uma edição não salva permanece ao minimizar/restaurar e que **Sair → Não** não a descarta. Recarregar sem salvar a edição de teste e encerrar normalmente.

Não usar início/reinício neste roteiro de interface. Testes reais de rede, UAC, protocolos adicionais e instalador permanecem etapas separadas.
