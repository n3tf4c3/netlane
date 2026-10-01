# Qualidade da conexão

Recurso da prévia local **0.3.3**, autorizado em 2026-09-30 após a proposta de marcador por placa. Na Visão geral, cada cartão recebe um **balão com a latência da última tentativa sobre o ícone da conexão**, ao lado de Conectada, conforme a referência visual enviada pelo usuário. O ícone fica verde, âmbar ou vermelho conforme a classificação; sem amostras suficientes, cinza. Os Mbps continuam sendo o tráfego em uso.

## Uso

1. Em **Qualidade**, escolha **Ping (ICMP)** e um IPv4, inicialmente `1.1.1.1`, ou **Site (HTTPS)** e um endereço de site.
2. Clique em **Iniciar**. O teste é feito separadamente pelas interfaces selecionadas no painel, a cada cinco segundos, com limite de três segundos por tentativa.
3. Passe o mouse no balão/ícone para ver **ping médio, último ping, perda de sondagens**, jitter, destino/IP resolvido, classificação, número de amostras e horário. Em HTTPS, os rótulos indicam tempo de resposta e falhas de conexão. O tooltip usa fundo escuro e linhas curtas, como a referência; interfaces indisponíveis ou última tentativa sem resposta mostram `— ms` no balão.
4. Clique em **Parar** para encerrar. O destino pode ser editado quando parado; durante a medição os campos ficam recolhidos, mantendo o painel compacto. Ocultar na bandeja mantém o monitor ativo; Sair cancela o monitor. Uma tentativa ICMP já em andamento pode terminar em até três segundos, com resultado descartado.

Somente tipo/destino são gravados em `%LOCALAPPDATA%\NetLane\quality-settings.json`. Resultados não são persistidos e o monitor inicia **desativado** a cada abertura. A escolha não altera rotas, prioridades, DNS, bindings IPv6 ou regras dos aplicativos; não precisa de UAC e funciona sem iniciar a sessão de roteamento. Um teste de velocidade máxima não faz parte deste monitor.

## Medidas e interpretação

- **Ping:** tempo de ida e volta ICMP; percentual de tentativas sem resposta válida. Ausência de ICMP pode significar filtro ou despriorização; **Sem resposta** não afirma queda de Internet.
- **HTTPS:** tempo da abertura TCP, negociação TLS e chegada dos cabeçalhos da resposta a um `HEAD`. Qualquer código HTTP recebido conta como resposta da rede, inclusive 3xx/4xx/5xx. Falha de certificado/protocolo também pode produzir falha HTTPS; isso não mede perda de pacotes.
- **Jitter:** média das diferenças absolutas entre tempos de amostras consecutivas bem-sucedidas. Não conecta intervalos separados por falha.
- **Janela:** no máximo 12 tentativas concluídas nos últimos 60 segundos. Erros locais de configuração e interfaces indisponíveis não contam como tentativas perdidas. Alteração de IP, remoção, desmarcação, parada ou nova sessão descartam resultados antigos/em voo. Amostras vencidas deixam de produzir verde.
- **Destino:** mede o caminho até o alvo configurado, não toda a Internet. O mesmo IPv4 resolvido é usado pelas duas placas durante a sessão, para comparar um mesmo alvo. Sites podem resolver vários endereços; reinicie a medição para resolver novamente. A resolução DNS usa a configuração normal do Windows, fora do tempo HTTPS e sem avaliar DNS por placa.

Classificação **indicativa**, após cinco tentativas; até lá, **Medindo**. Estes limites são heurísticas locais, não um SLA, e variam por destino e aplicação:

| Condição | Ping | HTTPS |
| --- | --- | --- |
| Boa | zero falhas, média < 80 ms, jitter < 15 ms | zero falhas, média < 500 ms, jitter < 150 ms |
| Atenção | alguma falha, média ≥ 80 ms ou jitter ≥ 15 ms | alguma falha, média ≥ 500 ms ou jitter ≥ 150 ms |
| Ruim | falhas ≥ 20%, média ≥ 150 ms ou jitter ≥ 40 ms | falhas ≥ 20%, média ≥ 1500 ms ou jitter ≥ 400 ms |
| Sem resposta | nenhuma resposta válida na janela | nenhuma resposta válida na janela |

Condições mais graves têm prioridade. Jitter sem pares suficientes é exibido como `—`. Cor é acompanhada de texto e o selo expõe descrição à acessibilidade; alto contraste usa cores do sistema.

## Seleção da interface

Cada tentativa valida GUID, índice atual, conexão e endereço de origem. HTTPS fixa origem e `IP_UNICAST_IF` no socket e confere a opção e o endereço após conectar, com TLS validado para o nome do site. Proxy, cookies e redirecionamentos automáticos ficam desativados. Não baixa o corpo da página e não segue outro destino.

Ping usa `IcmpSendEcho2Ex` com origem explícita. Para evitar atribuir saída ambígua, recusa modo weak-host send em qualquer interface ativa e IPs duplicados entre interfaces ativas; nesses casos, orienta usar HTTPS. Não altera o modo do Windows. Depois de uma tentativa, GUID/IP/índice são novamente conferidos.

Referências primárias: [ICMP com origem explícita](https://learn.microsoft.com/en-us/windows/win32/api/icmpapi/nf-icmpapi-icmpsendecho2ex), [seleção IPv4 por socket](https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-ip-socket-options), [estado da interface](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/ns-netioapi-mib_ipinterface_row), [métricas de qualidade](https://developers.cloudflare.com/speed/aim/).

## Validação desta rodada

Evidências da preparação em `artifacts/connection-quality-20260930/`: testes focados, regressão, renderizações WPF em 1000×650 e 1440×920, leituras de preservação e verificador de tráfego leve. Capturas são sintéticas, separadas do ensaio real das APIs. Instalação posterior da 0.3.3 registrada abaixo; QA pela UI instalada, commit/push e distribuição continuam etapas distintas.

- **25 testes focados** da apresentação final e **270 testes da regressão completa**, nenhum erro/ignorado. Recibos `quality-bubble-focused-v3.trx` e `quality-bubble-regression.trx`. Duas tentativas anteriores do teste visual identificaram o contexto do tooltip e o espaço vertical no tamanho mínimo; corrigidas, com recibos anteriores preservados.
- **Visual:** `renders-v2/quality-1000.png`, `quality-1440.png` e `quality-tooltip-1000.png`, conferidos após a referência do usuário. Balão mostra o último ping; tooltip mantém média separada, último valor e taxa de sondagens perdidas. Dados ilustrativos dessas capturas não são a leitura real.
- **APIs reais:** cinco pings para `1.1.1.1` e cinco `HEAD` HTTPS para `https://www.microsoft.com/` por placa. Ethernet `192.168.15.3`, índice 21; Wi-Fi `192.168.0.102`, índice 18, revalidados durante o ensaio. Todas as 20 tentativas responderam. Ping médio **29,2 / 52,6 ms**, jitter **0,75 / 3,25 ms**, nenhuma sondagem perdida nessas cinco amostras. Não extrapolar para um benchmark geral. HTTPS retornou 200 nas duas placas, com maior variação na Wi-Fi; ver cada amostra em `runtime-probe-result.json`.
- **Preservação:** `runtime-preservation.json` registra `ExitCode=0`, `Preserved=true`; quatro arquivos do perfil, regras reais, rotas/métricas IPv4, bindings IPv6, registro instalado 0.3.2 e Defender iguais antes/depois. Wi-Fi continua na prioridade 5, Ethernet 55. Nenhuma escrita no perfil real pelo verificador, nova instalação ou mudança de rede.

## Pacote final

Usar `artifacts/installer/preview-20260930-connection-quality-v2/NetLane-0.3.3-preview-win-x64-setup.exe`, **69.479.231 bytes**, SHA-256 `32EBFF173870A026EE3D079C7AE3A52961038129FD801659E68A7CD8F15FB560`. Compilação Release/self-contained x64 concluída; validação estática e consolidação registradas em `artifacts/connection-quality-20260930/implementation-review.json`. A pasta anterior sem `-v2` contém o visual anterior à referência: preservada como histórico, não escolher para atualizar.

Na preparação acima, o instalador **ainda não havia sido executado** e a **0.3.2 estava instalada**. Esses recibos históricos permanecem intactos; execução posterior está registrada abaixo. Nenhum commit/push.

## Instalação autorizada — 2026-09-30

Após **“vamos remover a 0.3.2 e instalar a 0.3.3”**, remoção **22:26:10–22:26:40** e instalação **22:26:41–22:27:20 (America/Cuiaba)** concluídas com UAC/assistentes manuais. `StageVerified=true` nas duas etapas e `SequenceVerified=true`, com 758 hashes do manifesto iguais, 760 arquivos totais esperados, registro **0.3.3**, atalho/ícone corretos e zero pixels diferentes no logo extraído da UI instalada.

Perfil de quatro arquivos preservado por hash e copiado em `artifacts/installer-qa-20260930-033/profile-backup/`; rede/DNS/IPv6, autostart, política de scripts e 26 arquivos protegidos iguais antes/depois de cada etapa. Defender ativo, Wi-Fi continua prioritária (**5 / Ethernet 55**) e nenhum processo NetLane restante. Nenhuma sessão de roteamento ou medição foi iniciada pelo instalador/supervisor.

[Recibos e limites da instalação](instalador-local.md#remoção-da-032-e-instalação-da-033--2026-09-30). Próximo: abrir pelo menu Iniciar, ir à Visão geral, escolher o alvo em Qualidade e clicar em Iniciar para conferir os balões/tooltips na UI instalada. Esse QA real da UI ainda não acompanhado; os 270 testes, capturas e ensaio real das APIs acima continuam evidências separadas. Não autoriza commit/push, distribuição ou novos testes de roteamento.

## Confirmação do usuário — 2026-09-30

Após a orientação **NetLane → Visão geral → Qualidade → Iniciar**, o usuário informou **“Testado e validado”**. Confirmação da UI instalada/recurso aceita e registrada como **relato do usuário**, sem novos cliques automatizados ou captura direta de valores/amostras. Rodada da 0.3.3 concluída.

Leitura às **22:41:29 (America/Cuiaba)** confirmou versão instalada **0.3.3**, processo da UI no caminho `C:\Program Files\NetLane\NetLane.UI.exe` (PID histórico 28160), nenhum serviço, quatro arquivos anteriores do perfil preservados e `quality-settings.json` presente. A presença da preferência confirma sua gravação, sem afirmar qual teste permanece ativo. Interfaces iguais à instalação, **Wi-Fi 5 / Ethernet 55**. O aplicativo não foi encerrado pelo agente e nenhum novo teste/mudança de roteamento foi emitido.

Recibo `artifacts/installer-qa-20260930-033/user-ui-confirmation.json`, SHA-256 `9AD1263641F84AAFCED349303B669EBC6059780280C26B544F00B155CB7FC5A8`. Resultados anteriores de testes, preparação e instalação permanecem intactos. Confirmação não amplia a cobertura para todos os destinos/protocolos, DPI ou demais cenários do instalador. Alterações continuam locais, sem commit/push; suporte adiado.
