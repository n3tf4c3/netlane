# Continuidade de roteamento com o painel na bandeja

**Concluído em 2026-09-12 às 07:56:33 (America/Cuiaba):** seis fases aprovadas na rodada com limite de 30 minutos, seguidas da verificação após o fechamento normal. Novas conexões do probe saíram pela Wi-Fi com a janela visível, oculta e restaurada, e voltaram à Ethernet após a parada. Mesma janela/serviço durante as fases ativas, rede/regras preservadas e nenhum processo do ensaio restante. O resultado se limita aos handshakes QUIC/IPv4 do executável isolado; não comprova transferência sustentada, aplicativos reais ou uma sessão inteira de 30 minutos.

A primeira rodada, com limite de cinco minutos, permanece **parcial**: encerrou antes da restauração. Seus recibos não foram usados para completar a segunda rodada. A preparação de 30 minutos recusada antes de abrir a janela também permanece preservada, sem aprovação.

> Nova rodada autorizada em 2026-09-12: o usuário respondeu **“ok, vamos fazer”** à proposta explícita de **30 minutos para os cliques**. O host da primeira rodada foi fechado normalmente às **07:31:48**, sem serviço e sem edições pendentes. A nova preparação usa build/pedido/probe separados; os recibos e binários anteriores não são substituídos. A ampliação não inicia roteamento por si só e não dispensa início/UAC manuais.

> **Rodada concluída:** build `artifacts/tray-routing-30m-v2-20260912`, diretório `artifacts/tray-routing/20260912T114202306Z-7128045c/`. Host histórico **2340** fechado às **07:56:04**; serviço histórico **38544** parado normalmente às **07:54:48**. Não há sessão a retomar. Próxima etapa na ordem combinada: preparação do instalador local, ainda não iniciada; instalação, assinatura e publicação continuam separadas.

## Resultado completo da rodada de 30 minutos

O usuário iniciou a sessão manualmente e confirmou “ativa”. O serviço **38544** começou às **07:45:18.178**, dentro da validade da referência. O limite registrado era **08:15:18.178**, mas a parada manual ocorreu após cerca de **9 min 31 s**, sem atingir o prazo. Os cliques de minimizar, restaurar, parar e fechar foram feitos e confirmados pelo usuário; não houve automação de entrada na UI.

Todas as fases abaixo pertencem ao mesmo diretório novo. Os horários são locais de **2026-09-12**, e cada fase usa um PID de probe distinto, com o mesmo destino **104.16.124.96:443**, ALPN `h3`, sem bind manual ou fallback TCP.

| Fase aprovada | Horário da conexão | PID histórico do probe | Saída observada |
| --- | --- | --- | --- |
| `baseline` | 07:42:21–07:42:22 | 1484 | Ethernet · 192.168.15.3 |
| `visible` | 07:48:52–07:48:53 | 18224 | Wi-Fi · 192.168.0.102 |
| `hidden-1` | 07:50:45–07:50:46 | 3896 | Wi-Fi · 192.168.0.102 |
| `hidden-2` | 07:51:16 | 2152 | Wi-Fi · 192.168.0.102 |
| `restored` | 07:53:55 | 10320 | Wi-Fi · 192.168.0.102 |
| `final` | 07:55:19 | 18232 | Ethernet · 192.168.15.3 |

As duas conexões ocultas começaram após **39,241 s** e **70,302 s** no mesmo período de ocultação. Os recibos mantêm `WindowVisible=false`, `WindowState=Minimized`, `TrayHidden=true`, versão de visibilidade **2**, mesma janela **2340** e mesmo serviço **38544**, com heartbeat avançando e histórico sem lacunas acima do limite. A restauração voltou ao estado **Normal**, versão **4**, e uma nova conexão continuou pela Wi-Fi na mesma sessão `Ready`.

A parada pelo painel registrou `Stopped` às **07:54:48.952**, `OwnsService=false`, `CleanupConfirmed=true` e nenhum erro. O controle `final` confirmou retorno à Ethernet. Após “fechado”, o registrador mostrou `WindowClosed=true` às **07:56:04.593** e o processo proprietário já não existia.

`verification-20260912T115633715Z.json` aprovou o roteiro às **07:56:33.715**: seis probes, mesma janela/serviço, fechamento e limpeza confirmados, regras reais/rede preservadas. O verificador releu os seis recibos brutos, fontes, destino, PIDs, sequência e intervalos. A última coleta registrou `ProcessesMatch=true`, `NetworkMatches=true`, nenhum serviço/probe/host restante e `routepolicies` IPv4/IPv6 desabilitados. Endereços, gateways, DNS, métricas e bindings das duas placas permaneceram iguais à referência; IPv6 continua desligado. SHA-256 das regras reais: `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.

Não houve alteração de código ou recompilação durante as fases ativas e a conferência final. Os resultados de build/testes abaixo permanecem os da preparação; não foram reexecutados nem substituídos por esta validação de runtime. O encerramento deste ciclo não autoriza repetições, instalador, alterações em aplicativos reais, commit ou push automaticamente.

## Escopo e proteção

`tests/NetLane.TrayRoutingReview` carrega a janela, o ícone nativo e o controlador de bandeja da UI de produção. Usa `WindowsServiceSession` e o `NetLane.Service.exe` do mesmo build, com uma única regra: uma cópia exclusiva de `NetLane.QuicProbe.exe` pela Wi-Fi. Regras, seleção de interfaces e recibos ficam em um diretório exclusivo de `artifacts/tray-routing/`. O arquivo real de regras é lido somente para hash.

O serviço não é simulado. O host substitui apenas a composição inicial de `App` por dependências explícitas; portanto não valida a localização do arquivo de regras no lançamento comum. Não altera OneDrive, Steam, auxiliares, bindings IPv6, DNS, gateways, métricas ou VPN. Não desconecta placas, suspende/reinicia o Windows, captura pacotes ou instala serviço. Sem commit/push.

Abrir a janela **não inicia o serviço**. O usuário marca a autorização temporária de `routepolicies` no diagnóstico, clica **Iniciar serviço**, confirma o diálogo e responde ao UAC manualmente. As opções globais são restauradas pela parada normal; bindings IPv6 continuam desligados. O host permite uma tentativa de início, revalida hashes/rede antes dela e mantém o arquivo sintético aberto em leitura, impedindo substituição ou ampliação acidental das regras.

## Medição

O host não minimiza/restaura a janela nem responde a diálogos. Registra eventos reais de visibilidade/estado e amostras a cada segundo, PID/início do proprietário e serviço, estado do controlador de bandeja, revisão, alterações pendentes e heartbeat do IPC autenticado.

`Hide` preserva a instância WPF sem emitir `Closed`; isso é comportamento esperado, não prova de roteamento. Referência: [Window.Hide — Microsoft](https://learn.microsoft.com/en-us/dotnet/api/system.windows.window.hide?view=windowsdesktop-10.0).

Cada fase abre um processo novo do probe: QUIC/IPv4, validação normal de certificado/SNI, ALPN `h3`, mesmo IPv4 remoto do controle, sem bind de origem, proxy ou fallback TCP. A conexão deve ser descartada e o processo encerrado antes de aprovação. Não comprova download HTTP/3 nem transmissão contínua.

| Ordem | Fase | Condição | Saída esperada |
| --- | --- | --- | --- |
| 1 | `baseline` | Sem serviço/políticas, antes da janela | Ethernet |
| 2 | `visible` | Janela visível, sessão própria `Ready`, regra aceita | Wi-Fi |
| 3 | `hidden-1` | Mesma janela/sessão, oculta por pelo menos 25 s | Wi-Fi |
| 4 | `hidden-2` | Mesmo período oculto, pelo menos 35 s e novo heartbeat | Wi-Fi |
| 5 | `restored` | Mesma janela/sessão, estado normal/maximizado original | Wi-Fi |
| 6 | `final` | Parada normal confirmada e opções restauradas | Ethernet |

O período oculto ultrapassa o limite de silêncio de 20 segundos do monitor do serviço. Exige histórico com heartbeats distintos, identidade estável, sem edição/erro e sem lacunas maiores que três segundos. O relógio monotônico mede o período oculto; UTC associa amostras ao início/fim do probe. Restaurar e ocultar de novo durante uma conexão invalida esse intervalo.

A rede/regras/flags são consultadas antes/depois de cada conexão. Na fase ativa, somente o PID exato do serviço próprio é permitido. Não se corrige qualquer divergência automaticamente. O verificador final relê os probes brutos, ordem, fontes/destino/PIDs, intervalos, revisões e identidades, além de conferir rede/flags/processos atuais. Exige fechamento normal da janela.

## Build e roteiro

```powershell
dotnet build NetLane.sln -c Release --artifacts-path artifacts/tray-routing-20260912
dotnet test tests/NetLane.TrayRoutingReview.Tests -c Release --artifacts-path artifacts/tray-routing-20260912
dotnet test tests/NetLane.QuicProbe.Tests -c Release --artifacts-path artifacts/tray-routing-20260912
& .\scripts\test-tray-routing.ps1 -BuildDirectory .\artifacts\tray-routing-20260912 -OpenReview
```

O último comando cria referência nova, executa o controle sem políticas e abre a janela não elevada. O pedido de início vence em dez minutos; não reutilizar referências vencidas ou PIDs/IPs históricos. Não abrir o painel comum em paralelo, nem editar/adicionar/remover regras no ensaio.

Após o início manual/UAC, manter a janela visível para `visible`. Minimizar pelo botão **—**, aguardar as medições `hidden-1` e `hidden-2`, restaurar pelo ícone/menu para `restored` e parar pelo painel para `final`. Cada medição é explícita; abrir/minimizar a janela não dispara tráfego automaticamente:

```powershell
& .\scripts\measure-tray-routing.ps1 -RunDirectory <diretorio-do-ensaio> -Stage visible
```

Trocar `visible` pela fase correspondente, na ordem. Após `final`, fechar normalmente e usar `-Stage verify`. Os scripts de medição não controlam a UI, iniciam serviço ou elevam processos. A primeira janela usou o título **ENSAIO DE BANDEJA — somente probe isolado; início e UAC manuais**; a nova composição informa também o limite escolhido no título.

### Rodada com limite de 30 minutos

O parâmetro `-ActiveSessionLimitMinutes 30` é explícito e fica gravado no pedido identificado por hash, no recibo de abertura e no estado da janela. O padrão continua **5 minutos**, e só **5 ou 30** são aceitos; não existe duração ilimitada. O prazo conta a partir do início do processo da sessão, não da abertura do painel. O horário UTC de término previsto aparece em `ActiveDeadlineUtc`; a parada usa tempo monotônico. Pedido de parada e mudança da regra isolada continuam causando interrupção imediata, com motivos distintos nos recibos.

O histórico da rodada de 30 minutos comporta **5.400 amostras/eventos**, cobrindo a sessão e a preparação; o limite anterior de 1.200 poderia descartar o início de uma ocultação longa. A validade de dez minutos da referência **antes de iniciar** foi preservada, assim como a regra exclusiva bloqueada para gravação, o UAC manual e a limpeza normal.

```powershell
dotnet build NetLane.sln -c Release --artifacts-path artifacts/tray-routing-30m-20260912
dotnet test tests/NetLane.TrayRoutingReview.Tests -c Release --artifacts-path artifacts/tray-routing-30m-20260912
dotnet test tests/NetLane.QuicProbe.Tests -c Release --artifacts-path artifacts/tray-routing-30m-20260912
& .\scripts\test-tray-routing.ps1 -BuildDirectory .\artifacts\tray-routing-30m-20260912 -ActiveSessionLimitMinutes 30 -OpenReview
```

As seis fases permanecem obrigatórias. Não reutilizar os resultados da primeira rodada para completar artificialmente a nova. A janela nova informa **limite de 30 min** e permanece com o serviço parado até o início manual.

Validação do novo build `artifacts/tray-routing-30m-20260912`: compilação da solução com **0 avisos/erros** e **328 testes aprovados**, sem falhas/ignorados:

- `test-results/tray-review-30m.trx`: **37/37**, incluindo os 26 anteriores e 11 casos de limite explícito, fronteira exata, parada antecipada por pedido/regra, retenção do histórico e compatibilidade com recibos antigos. O caso de 30 minutos não expira ao passar de cinco minutos. São testes de duração simulada; não equivalem a uma sessão real de 30 minutos.
- `test-results/quic-regressions-30m.trx`: **75/75**.
- `test-results/production-regressions-30m.trx`: **216/216**, em 46 segundos.

As alterações desta preparação limitam-se ao host/testes/lançador de revisão e à documentação. Não houve alteração no código de produção da UI/serviço/motor, nas regras reais ou nos binários da primeira rodada.

#### Preparação recusada e nova referência

A primeira preparação de 30 minutos, `artifacts/tray-routing/20260912T113742283Z-14bc52a1/`, foi recusada às **07:38:08**, na comparação de rede/regras após o controle. O handshake do probe **28216** foi observado pela Ethernet, mas não houve `measured-baseline.json` aprovado nem abertura de janela/UAC/serviço. Não tratar o handshake sozinho como aprovação dessa preparação.

Às **07:39:32**, uma nova comparação voltou a ser igual à referência, com flags desabilitados. O código antigo não havia preservado a amostra exata que divergiu; **não foi possível identificar o campo ou atribuir a causa da diferença anterior**. Não se alterou a rede nem se relaxou a comparação. O host passou a registrar `network-flags-*.json` e `network-snapshot-*.json`, inclusive quando recusa uma amostra. Esses recibos contêm a referência e o valor observado, além dos resultados separados para rede e PID do serviço.

Foi criado outro build, `artifacts/tray-routing-30m-v2-20260912`, preservando também os binários da preparação recusada. A solução compilou com **0 avisos/erros**; foram reexecutados **37/37** testes do host e **75/75** do probe/controlador. A suíte de **216/216** permanece a execução do build de 30 minutos anterior neste mesmo ciclo; não foi reexecutada depois da inclusão dos recibos de diagnóstico. A adição não mudou código de produção ou condições de aprovação.

Nova preparação: `artifacts/tray-routing/20260912T114202306Z-7128045c/`. Controle aprovado às **07:42:21–07:42:22**, probe **1484**, Ethernet **192.168.15.3**, remoto **104.16.124.96:443**, ALPN `h3`. As amostras anteriores/posteriores registram `ProcessesMatch=true` e `NetworkMatches=true`.

A janela **ENSAIO DE BANDEJA — limite de 30 min; somente probe isolado; UAC manual** foi aberta no PID histórico **2340**. Às **07:43:21**, respondeu e registrou `WindowVisible=true`, `WindowState=Normal`, `ActiveSessionLimitMinutes=30`, `StartAttempted=false`, `OwnsService=false`, sem alterações/erro e `ActiveDeadlineUtc=null`: os 30 minutos só começam após o início da sessão. Rede igual à referência, flags IPv4/IPv6 desativados e regras reais com hash `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.

Hashes do build atual: DLL do host `D8B691E9193FB1D459167D3DDB1348099279B539FF72FAB9B5E22FCB22955A71`; DLL de UI carregada `92A9B35D9C7CD3E6A9358AC61FF189202E2A3CC0172BAE5F8410EA87BB34781C`. Revisão da regra sintética `451303D47714400C2BC2D0863532B5E874666034616C4EE9B4EFFF3352080AAB`.

O início usou a referência criada às **07:42:08**, então válida até aproximadamente **07:52:08**, e ocorreu às **07:45:18**. A validade era somente para iniciar; as medições posteriores pertenceram à mesma sessão. Esta referência agora é histórica: não editar sua data, reabrir o ensaio ou reutilizar a sessão encerrada/diretório recusado. Os quatro recibos de tráfego da primeira rodada não completam a segunda.

## Validação da preparação

Build da solução: **0 avisos/erros**. Últimas execuções em `artifacts/tray-routing-20260912/test-results/`:

- `tray-review-locked-rules.trx`: **26/26**, estados e histórico oculto, identidade, heartbeat, início condicionado, cancelamento e confirmação de limpeza.
- `quic-regressions.trx`: **75/75** do probe/controlador.
- `production-regressions.trx`: **216/216** da UI/serviço/motor, em 45 segundos.

A primeira compilação do host teve avisos de captura redundante do parâmetro e tokens de cancelamento nos testes; foram corrigidos. Os 26 casos passaram antes e depois. Nenhuma dessas suítes ativa roteamento real para comprovar esta etapa.

### Controle real sem políticas e janela aberta

Preparação: `artifacts/tray-routing/20260912T103257291Z-08790a04/`. O controle `baseline` passou em **2026-09-12 às 06:33:15–06:33:16 (America/Cuiaba)**: probe PID histórico **24292**, Ethernet **192.168.15.3**, remoto **104.16.124.96:443**, QUIC/ALPN `h3`. A Ethernet mudou de `.5` no dia anterior para `.3` antes deste ensaio; foi usada a referência nova, sem reconfigurar a placa. A Wi-Fi continua em **192.168.0.102**.

A janela foi solicitada às **06:33:23**, PID histórico **37140**, no build `artifacts/tray-routing-20260912`. O registrador confirmou criação da janela de produção e posteriormente estado **Minimized**, `WindowVisible=false`, `TrayHidden=true`, sem alterações pendentes. **StartAttempted=false**, `OwnsService=false`: não houve UAC ou roteamento iniciado por esta preparação. Isso observa a ocultação interna, não substitui captura/relato dos cliques no shell.

Conferência somente leitura às **06:35:29**: host respondendo, nenhum `NetLane.Service`/`NetLane.QuicProbe`, rede igual à referência, flags IPv4/IPv6 `routepolicies=disabled`, bindings IPv6 preservados e hash real `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`. DLL do host: `5663246FD62D6B408A0DFF5BB558246A43735CD2206498F9BC1970EE81F4A832`; DLL de UI carregada: `2BEEE54D4E054E03AFA96D8D4A4482EF6E0DEC4C9990A57AF334894483B72DB4`.

Naquele ponto, a próxima ação era abrir a janela pelo ícone e iniciar a sessão pelo diagnóstico/UAC, mantendo-a visível para `visible`. O pedido de início vencia aproximadamente às **06:43:02**; não editar essa data nem reutilizar a referência para outra tentativa. O progresso efetivo está registrado abaixo.

## Histórico da primeira rodada: execução parcial e encerramento

### Progresso após início manual

O usuário informou **“ativa”**. A conferência às **06:38:26** encontrou a mesma janela, visível em estado normal, controlando o serviço PID histórico **2680**, iniciado às **06:38:05**. O IPC confirmou `Ready`, `WfpRoutingEngine`, uma única regra aceita, caminho/revisão sintéticos corretos e ausência de edição pendente.

`measured-visible.json` aprovou a fase visível às **06:38:49–06:38:50**: novo probe **34656**, QUIC/IPv4 com saída **Wi-Fi / 192.168.0.102**, mesmo remoto **104.16.124.96** do controle. Janela visível antes/depois, mesmo proprietário/serviço e versão de visibilidade **4**, sem mudança durante a conexão. As verificações de rede/regras/flags antes/depois passaram; nenhum erro do registrador. Isso comprova a fase visível, não a continuidade oculta ou a limpeza final.

A próxima ação solicitada foi minimizar manualmente pelo botão **—**, preservando a sessão para `hidden-1` e `hidden-2`. O usuário informou “minimizado”.

### Duas conexões com a janela oculta

- `hidden-1`: **06:41:18**, probe PID histórico **20132**, após **54,626 s** de ocultação.
- `hidden-2`: **06:41:50–06:41:51**, probe **14936**, após **87,689 s** de ocultação; observação final com **89,689 s** no mesmo intervalo.

Ambas saíram pela **Wi-Fi / 192.168.0.102**, com ALPN `h3` e remoto **104.16.124.96:443**. Mesmo proprietário **37140**, mesmo serviço **2680**, janela não visível, `TrayHidden=true` e versão de visibilidade **8** antes/depois das conexões. O heartbeat avançou e as verificações de histórico/rede/regras/flags passaram. Isso comprova novas conexões QUIC/IPv4 do probe durante a ocultação, não transferência contínua nem tráfego de aplicativos reais.

### Limite automático antes da restauração

O registro mostra `Ready` e janela oculta até **06:43:06**, seguido de `Stopped`/`OwnsService=false` às **06:43:06.867**, com `CleanupConfirmed=true`. O limite de cinco minutos do host encerrou a sessão pelo IPC normal. Não houve pedido `stop.request`, e a regra sintética manteve a revisão original. O erro de interrupção é preservado; não foi apagado ou convertido em sucesso.

Após “restaurado”, a conferência encontrou a mesma janela visível em estado **Normal**, versão de visibilidade **10**, mas com serviço já parado. O primeiro registro dessa restauração é **06:59:42.772**. Portanto não foi executada a fase `restored`, nem `final` ou `verify`: o roteiro não pode ser aprovado sem elas. Nenhuma sessão foi reiniciada automaticamente.

Conferência independente somente leitura às **07:01:03**: rede/regras iguais à referência pré-ensaio, flags IPv4/IPv6 `routepolicies=disabled`, bindings IPv6 preservados e nenhum processo de serviço/probe. SHA-256 real permanece `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`; regra isolada `980997124DAD0F99E2B05A92296015D00358FF49139F4A3A28AF04C39B56D4B7`. Naquela conferência, o host continuava aberto, respondendo e sem edições pendentes; foi fechado depois, às **07:31:48**.

Também foram relidos os quatro probes brutos: PIDs/caminho corretos, saída 0, stderr vazio, handshake `h3` e descarte confirmados, sem bind manual ou fallback TCP. **53 hashes** do host/UI/serviço e **4 do probe** continuam iguais ao pedido. O histórico foi preservado separadamente em `review-at-restoration-20260912.json`, SHA-256 `1E5A4261C9345A7656D3238D434615D65B699A0C524DD84DF51FE7F7051F8DC5`, antes de o buffer móvel perder eventos antigos.

Naquele ponto, o próximo passo proposto era fechar normalmente o host parado e, **somente após nova decisão do usuário**, preparar outra referência/rodada com prazo adequado para os cliques, UAC manual e as mesmas regras isoladas. A decisão posterior e o resultado da rodada nova estão acima. O limite e os binários da primeira rodada não foram alterados.

## Interrupção e limites

O host solicita parada normal da sessão própria após o limite gravado no pedido (**5 minutos por padrão; 30 na nova rodada autorizada**), mudança do arquivo sintético ou presença de `stop.request` no diretório do ensaio. Registra interrupção, não sucesso. Não mata o serviço nem reinicia automaticamente. Falha de parada permanece indicada para diagnóstico e nova tentativa pelo painel. Timeout do probe recolhe somente o processo que a medição criou.

Uma fase recusada não autoriza repetição elevada presumida. Preservar recibos `error-*.json`, parar normalmente e conferir flags/rede. Não repetir ou agendar este ensaio automaticamente.

A skill **Computer Use** foi consultada/inicializada, mas `sky.list_apps()` retornou `Computer Use native pipe is unavailable` (`os error 2`). A conferência visual fica com o usuário; não houve captura atual do desktop. A skill orientou manter cliques/UAC manuais, sem automação direta de UI por PowerShell. Os registros internos não substituem a confirmação do ícone/menu e dos cliques no shell.

Não comprova vazão, tráfego sustentado, IPv6, aplicativos reais, Explorer reiniciado ou outras escalas/monitores. A limpeza refere-se à sessão própria e às opções globais observadas; não há enumeração global de todas as políticas WFP.

## Revalidação para commit/push

Após o encerramento do ensaio, o usuário solicitou commit/push antes do instalador. Em **2026-09-12**, a validação do código atual usou outro diretório, `artifacts/prepublish-20260912T1216/`, sem substituir os binários/recibos das rodadas:

- Build Release da solução: **0 avisos/erros**.
- `test-results/production-prepublish.trx`: **216/216**, em 43 segundos.
- `test-results/quic-prepublish.trx`: **75/75**.
- `test-results/tray-review-prepublish.trx`: **37/37**.
- Sete scripts PowerShell analisados sem erros de sintaxe.
- Host de fechamento compilado sem avisos/erros com `NetLaneReviewBuild` apontando para a UI desse build; `--check` terminou com código **0**, sem janela visível ou serviço real.

A primeira tentativa de compilar o host de fechamento omitiu a propriedade obrigatória `NetLaneReviewBuild` e foi recusada como previsto pelo projeto. A compilação com o parâmetro documentado passou; não foi necessária correção de código. Os **328 testes** foram reexecutados nesta validação posterior, distinta da preparação v2 e da prova de runtime. Nenhum ensaio elevado ou conexão de probe real foi repetido. O repositório não possui workflow de CI configurado; testes locais não são apresentados como CI ou instalação validada.
