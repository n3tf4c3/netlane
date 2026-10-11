# QA de diagnóstico de rede — prévia Rust 0.4.0

Este kit prepara a prévia para ensaios privados em outro PC Windows x64. O aplicativo, o agente e o helper offline ficam em `application/`. `qa-package.json` identifica versão, commit base, alterações locais, hashes das fontes efetivamente usadas e dos arquivos entregues. Confira `SourceIsClean`: um build com alterações locais não equivale ao commit base. Hashes verificam integridade; o kit ainda não tem assinatura ou instalador atualizado.

## Preparar e repetir o ensaio offline

Na raiz do projeto, execute:

```powershell
.\scripts\prepare-diagnostics-qa.ps1
```

O script executa formatação, testes, Clippy e build Release com dependências travadas. Cria uma pasta nova em `artifacts/`, preservando candidatos anteriores. Não instala o aplicativo ou Npcap, não consulta tráfego, não inicia sondas e não faz login ou inferência.

Copie a pasta inteira para um disco local no PC de QA. Dentro dela:

```powershell
.\scripts\test-diagnostics-qa.ps1
.\scripts\open-diagnostics-qa.ps1 -Mode Demo
```

Os atalhos `Executar QA offline.cmd` e `Abrir QA demonstracao.cmd` também executam esses scripts, respeitando a política de execução do PC. O manifesto recusa arquivos extras fora de `results/`, inclusive DLLs adicionadas junto aos executáveis.

O script de abertura cria um perfil exclusivo por sessão. Windows x64 e os componentes do Visual C++ usados pelo GPUI são pré-requisitos; uma máquina limpa precisa validá-los no QA de instalação. O helper offline não depende de Npcap, Codex ou serviços NetLane.

O ensaio automatizado valida hashes, análise integral PCAP/PCAPNG, totais conhecidos, amostras de até 512 pacotes/2048 bytes cada, baseline, desvios de RST/destino/volume, rejeição de arquivos inválidos, cancelamento, exportação derivada, recusa de sobrescrita, preservação dos originais e coleta rotativa com fonte fictícia. O resultado fica em `results/offline-*/checks/verification.json`; `run.json` registra o veredito e `resources.csv` amostra RAM privada, working set e CPU do helper a cada segundo. Os picos são amostrados e não medem o consumo da GUI nem comprovam desempenho em outro hardware.

Para estender **somente o ensaio sintético**, por exemplo:

```powershell
.\scripts\test-diagnostics-qa.ps1 -StressSeconds 3600 -StressMiB 1024 -StressPacketsPerSecond 1000
```

A coleta termina ao alcançar duração, disco ou limite de segmentos. Consulte o motivo no recibo. Uma duração pedida não prova que ela foi cumprida; orçamento pequeno de disco pode encerrar antes. Quedas nas filas e prévias omitidas são registradas e precisam ser avaliadas, mesmo quando o ensaio passa. O teste exige conciliação dos contadores, fechamento da fonte, limites respeitados e segmentos íntegros; não exige perda zero. Ele não substitui a coleta real longa.

`buffered` conta pacotes aceitos pelo gravador; `written` conta somente pacotes em segmentos publicados após flush/sincronização/fechamento. O recibo final concilia `written` e `bytes` com os segmentos; pacotes num `.part` que não finalizou ficam em `unwritten`, sem serem anunciados como gravados.

Na árvore de desenvolvimento, `scripts/verify-rust-packets.ps1` valida 25 cenas GPUI identificadas por nome, imagens distintas, os recibos interativo/de recuperação/de retomada/do pacote completo e os hashes de três cadernos. Inclui reconexão/logout fictícios, troca/remoção de seleção saturada, bloqueio de envio enquanto o modelo aguarda confirmação, reabertura manual sem envio automático, resumo revisável e histórico de estados. As cenas 24/25 salvam e reabrem pelo worker de produção um pacote movido, com os originais indisponíveis, dumps integrais, baseline, relatórios, anotação, próxima pergunta e pacote fora da amostra. Edição não salva e coleta ativa simulada bloqueiam substituição. Essa prova usa `--demo` e não preenche os casos de conta/captura reais nem exercita o seletor nativo de pastas.

## Capturas fictícias e resultados esperados

`fixtures/expected.json` contém contagens e SHA-256. São endereços de documentação, MACs vazios, conteúdo repetido e checksums zero, sem dados de um dispositivo real. Não use essas capturas para avaliar validade de checksum ou concluir comportamento de rede.

| Arquivo | Resultado esperado no kit padrão |
| --- | --- |
| `baseline.pcap` | 1000 pacotes, 500 TCP, nenhum RST; destino 198.51.100.20 |
| `deviation.pcap` | 70000 pacotes de 2112 bytes; arquivo de 148960024 bytes, acima do antigo limite de 128 MiB; 35000 TCP, 17500 RST; novo destino a partir do pacote 35001 |
| `pcapng/selected.pcapng` | 3 pacotes exportados da baseline, 2 TCP e 1 UDP |
| `truncated.pcap` | Recusa por registro incompleto |
| `invalid.pcap` | Recusa por formato inválido |

Se o gerador receber outros tamanhos/contagens, prevalece `expected.json`. O arquivo inteiro determina os totais; as amostras apenas permitem visualizar/exportar um subconjunto. Exportações podem conter somente os primeiros 2048 bytes de cada pacote, preservando no arquivo e recibo o tamanho original e o recorte. A referência de um desvio pode estar fora das amostras; recarregar esse pacote por âncora ainda é uma evolução pendente.

## Matriz de ensaios manuais

Abra a prévia com perfil exclusivo no PC **autorizado para o ensaio**:

```powershell
.\scripts\open-diagnostics-qa.ps1 -Mode Lab -TargetLabel 'PC-LAB-01'
```

O modo Lab consulta as informações locais normais do aplicativo. Captura, sondas, rotas, autorun e envio para IA não fazem parte da abertura do kit; execute apenas os casos autorizados para o alvo. O agente de roteamento não precisa ser habilitado para validar diagnóstico. Use o `manual-cases.json` copiado para a sessão para registrar `passed`, `failed`, `blocked` ou `pending`, horário, alvo, observação e caminho das evidências. Não altere o modelo da raiz do kit, pois ele faz parte do manifesto de integridade.

| Caso | Execução e critério de aprovação |
| --- | --- |
| QA-01 | Verificar integridade do kit no alvo. Guardar `run.json`, versão e hash do manifesto; nenhum binário ou fixture divergente. |
| QA-02 | Importar as três capturas válidas na GUI. Totais coincidem com `expected.json`; rolagem, detalhes, RAW, seleção e retorno entre telas funcionam; número de amostras aparece claramente. |
| QA-03 | Importar as duas capturas inválidas e cancelar importação grande. A interface apresenta erro/cancelamento e continua utilizável; não exibe conclusão parcial como completa. |
| QA-04 | Selecionar baseline e desvio. Confirmar RST, novo destino e volume no kit padrão, os números das referências e a distinção entre hipótese e diagnóstico. |
| QA-05 | Exportar PCAP e PCAPNG de amostras. Reabrir no NetLane e em Wireshark/tshark: horários, caplen/original_len, bytes e número de pacotes coincidem; recorte identificado; pasta existente recusada; originais intactos. Uso de tcpdump para reabrir PCAP é um ensaio adicional em ambiente compatível. Não instalar ferramentas automaticamente. |
| QA-06 | Gerar relatórios locais Markdown/HTML, abrir e revisar. Contexto/hash e seleção correspondem ao dump, payload literal não vira HTML ativo; novo destino recusado se a pasta existe. |
| QA-07 | No alvo autorizado com Npcap funcional, selecionar uma interface/BPF. Iniciar e parar uma coleta curta; confirmar interface, duração, fechamento da fonte, manifests, contadores e reabertura de todos os segmentos. Um DLL presente não comprova esse caso. |
| QA-08 | Coleta real longa com duração/taxa/budget combinados. Registrar RAM/CPU da GUI e do processo, espaço, rotatividade, responsividade e perdas. Exercitar limite de disco, parada, interface indisponível e fechamento; conciliar recibo e hashes. Registrar duração efetiva e hardware. |
| QA-09 | Codex real: login em perfil exclusivo, catálogo/modelo, revisão, envio, resposta e cancelamento. Fazer perguntas sucessivas sobre a mesma captura: o chat conserva turnos e contextos. Trocar modelo e tentar enviar imediatamente: aguardar confirmação e conferir modelo/esforço no relatório. Logout/login ou desconexão/reconexão iniciam outra conversa identificada; histórico e notas locais permanecem, sem presumir que o provedor recebeu o histórico. Trocar captura também inicia outra conversa. Atingir 16 anexos, remover/substituir seleção e continuar preservando os contextos antigos. Sem envio ao importar/trocar tela; confirmar os dados de cada envio. |
| QA-10 | Anotar uma resposta, editar a hipótese, marcar estado e voltar a discuti-la: preservar contexto de origem e exigir revisão antes do envio. Salvar caderno Markdown/JSON e conferir hashes/turnos/notas/horários/histórico de estados. Abrir a pasta salva pelo seletor nativo, preparar resumo, editar e revisar: abertura e resumo não enviam nada; próximo envio usa nova conversa do provedor. PCAP da tabela permanece separado. Reabrir exportação antiga sem datas inventadas. Alterar um arquivo ou tentar abrir com anotação pendente: recusar e preservar o trabalho atual. Relatório da resposta conserva o contexto original e recusa referências inválidas; o caderno pode registrar uma resposta inválida sinalizada e a omite do resumo. Sair/Nova investigação protegem o caderno pendente. |
| QA-11 | Windows/DPI/escala: pacotes e chat lado a lado em janela larga, chat inteiro em janela menor, rolagens, histórico, campo de pergunta, notas, diálogos, contraste, bandeja e minimizar/restaurar. Nenhum bloqueio visível; encerramento durante análise/coleta autorizado fica registrado. |
| QA-12 | Instalador Rust atualizado em Sandbox/VM autorizada: pré-requisitos, usuário comum, instalação, abertura, atualização e remoção sem perda dos dados preservados. Seguir os scripts/procedimentos específicos; este kit portátil não aprova instalação, assinatura, atualização automática ou migração do perfil .NET. |
| QA-13 | Salvar investigação com capturas pelo seletor nativo: incluir todos os dumps referenciados e baselines, conferir bytes integrais/hashes, turnos/notas/relatórios e pergunta em edição. Mover a pasta e deixar os originais indisponíveis; reabrir e conferir tabela, baseline e pacote selecionado fora da amostra. Discutir anotação de outro dump e usar Reabrir dump da seleção. Cancelamento/arquivo ausente ou alterado não anuncia pacote completo; falha de abertura preserva o trabalho atual. Coleta ativa e edição não salva bloqueiam substituição. Nenhum envio ou login automático; revisão obrigatória para a próxima conversa. Verificar aviso de texto aberto e revisar dados antes de compartilhar. |

Antes da coleta longa, registre duração desejada, memória/CPU máxima aceitável para o hardware, limite de disco e tolerância de perdas. Enquanto esses critérios e a coleta real não estiverem registrados, não declarar “sem travamentos” ou longa duração aprovada. Para QA-09/10, registre modelo e resultado, sem tokens, credenciais ou logs da conta.

Capturas e relatórios reais podem conter dados sensíveis. Guarde os resultados fora do Git e revise antes de compartilhar. A pasta `artifacts/` é local e ignorada. O envio para Codex é uma ação explícita do operador; usar perfil próprio não torna o conteúdo enviado anônimo.

## Ensaio nativo automatizado no desenvolvimento

No PC autorizado, este comando usa Npcap já instalado, perfil novo e tráfego UDP fictício somente no loopback:

```powershell
.\scripts\test-native-diagnostics-qa.ps1 -TargetLabel $env:COMPUTERNAME -Seconds 1200
```

Esse ensaio é optativo e existe somente no build `gui-test`. Não faz parte da abertura normal do kit ou de `--demo`. O alvo precisa coincidir com o PC atual; a pasta deve ser nova sob `artifacts/`, sem links, e o perfil fica dentro dela. A porta UDP é reservada pelo próprio gerador antes da captura; o BPF registrado usa essa porta e `127.0.0.1`, evitando portas excluídas pelo Hyper-V. O ensaio não instala drivers, inicia roteamento ou envia dados para IA.

O mesmo painel, gravador e analisador da aplicação executam coleta curta, parada, limite de disco, interface inexistente e coleta longa. `hardware-and-criteria.json` registra critérios antes da abertura: até 100 datagramas/s, 64 MiB de disco, 512 MiB privados, CPU média normalizada de até 25% e intervalo máximo de 3 s entre atualizações do GPUI. Não são limites universais do produto. O painel é desenhado a cada atualização de 1 s, inclusive com a janela oculta. `resources.csv` usa formato numérico independente da região do Windows e permanece aberto com leitura/escrita compartilhadas para permitir acompanhamento. Imagens e `heartbeat.jsonl` registram a tabela recebendo segmentos; esse exercício automatizado não cobre operação manual, DPI, seletor nativo ou encerramento interativo.

Exige zero descartes na fila/driver e zero falhas/omissões de análise. No corte por parada, duração ou disco, o critério deste ensaio de até 100 datagramas/s admite até 129 registros em `unwritten`. Isso não significa ausência de perda: registre o valor efetivo. A implementação agora usa lotes; o limite geral de registros em trânsito é 8704, e não aumenta a tolerância deste ensaio de baixa taxa. Um arquivo `.part` não publicado não é contabilizado como segmento válido. Confirme adicionalmente a reabertura, os hashes e a conciliação de todos os segmentos.

O carregador consulta somente `System32/Npcap/wpcap.dll` e `System32/wpcap.dll`, usando caminhos absolutos e dependências restritas ao sistema. Confirma a identidade via `pcap_lib_version`, recusa WinPcap e seleciona a versão Npcap mais recente encontrada. Isso atende instalações em modo compatível que conservam DLLs antigas na outra pasta. O runtime selecionado fica fixo durante o processo; após uma atualização de Npcap, reabra o aplicativo. [Referência do Npcap](https://npcap.com/guide/npcap-devguide.html).

## Interoperabilidade em Docker

`scripts/qa-network-tools.Dockerfile` permite criar um ambiente descartável com tshark, editcap, tcpdump e Python, sem instalar ferramentas no host. Registre o hash da imagem e as versões efetivamente usadas. Os pacotes do Alpine são resolvidos no build; o digest da base sozinho não trava as versões dessas ferramentas.

`scripts/diagnostics-interop.py` roda nesse ambiente com fixtures montadas em `/fixtures`, exportações NetLane em `/exports`, ambos somente leitura, e uma pasta nova de saída em `/out`. Use `--network none`, `--cap-drop ALL` e limites de recursos para a leitura offline. Todas as invocações de tshark/tcpdump usam arquivo de entrada explícito. O script compara contagens, horários e comprimentos de todos os registros; verifica RAW dos três primeiros e compara o digest de horários/comprimentos/bytes de todos os pacotes reescritos pelos três tools. Confirma a preservação dos originais. `verification.json` distingue esse ensaio de uma captura de rede.

Para confirmar que os arquivos reescritos também entram no analisador integral do NetLane, use o exemplo offline `inspect_dump`, com saída JSON nova e caminhos absolutos. Captura dentro de um contêiner Linux é evidência adicional de interoperabilidade; não aprova Npcap, GUI ou instalador no Windows.

## Critérios para avançar

### Preparação para interfaces de 10 Gb/s

O painel oferece os perfis **diagnóstico** (64 MiB por segmento, até 2048 bytes por pacote) e **alto volume** (128 MiB, até 65535 bytes, incluindo quadros jumbo). O operador escolhe o perfil antes de iniciar; trocar o perfil não inicia captura nem aumenta o orçamento de disco escolhido. O snaplen fica no recibo e no cabeçalho PCAP. Captura em uma interface de 10G não equivale a sustentar 10 Gb/s contínuos sem perdas.

A fonte entrega até 256 registros ou aproximadamente 256 KiB por lote. São 32 lotes na fila e 34 buffers no pool, com cerca de 10,7 MiB de capacidade reservada. O Npcap usa [`pcap_dispatch`](https://npcap.com/guide/wpcap/pcap_loop.html), com contagem positiva limitada pelo espaço restante e pelo snaplen, e copia o pacote emprestado direto para o lote. A API anterior `Source::poll` continua disponível. O leitor nunca espera um buffer liberado pelo disco: saturação aumenta `queue_dropped`. O driver usa um buffer de 16 MiB configurado antes da ativação, buffer de leitura de 1 MiB e espera pelo evento nativo. Essas opções pertencem somente ao handle da captura, seguindo as [APIs do Npcap](https://npcap.com/guide/npcap-api.html) e a [configuração antes da ativação](https://npcap.com/guide/wpcap/pcap_set_buffer_size.html).

Gravação e SHA-256 usam etapas separadas. A finalização tem fila de dois segmentos e dois trabalhadores; a análise recebe segmentos na ordem da captura, mesmo quando a sincronização termina fora de ordem. `written` só conta arquivos publicados depois de flush, sincronização, conclusão do hash e renomeação. Os buffers adicionais de escrita/hash são limitados a aproximadamente 10 MiB. A análise integral continua separada e limitada; segmentos que não entram na fila são preservados no disco e contabilizados em `analysis_skipped`, para análise posterior. Nenhuma amostragem reduz silenciosamente o PCAP gravado.

`driver_stats` mantém os campos nativos de 32 bits por compatibilidade. `driver_totals` acrescenta totais de 64 bits, acumulados por amostras de aproximadamente um segundo e no fechamento, incluindo a volta dos contadores. Pressupõe contador monotônico por handle e no máximo uma volta entre amostras; driver/host bloqueado por tempo suficiente pode invalidar essa condição. `performance` registra tempos de leitura, espera, escrita, hash e sincronização; etapas paralelas se sobrepõem, portanto esses tempos não devem ser somados como duração total.

Benchmark reproduzível, somente offline, com arquivos sintéticos novos:

```powershell
.\scripts\measure-capture-throughput.ps1 -OutputDirectory artifacts/throughput-1500 -Seconds 5 -PacketsPerSecond 833333 -PacketBytes 1500
.\scripts\measure-capture-throughput.ps1 -OutputDirectory artifacts/throughput-jumbo -Seconds 5 -PacketsPerSecond 138889 -PacketBytes 9000 -SkipBuild
```

Essas cargas pedem aproximadamente 10 Gb/s de bytes capturados, sem modelar preâmbulo, intervalo entre quadros ou FCS. A fila, a gravação PCAP, SHA-256, `sync_all`, rotação e análise são as de produção. O benchmark verifica hashes e concilia os contadores; esse resultado não significa aprovação de vazão. `requested_rate_sustained` também exige taxa observada próxima à solicitada e zero descartes na fila. `source_backlog_not_polled` denuncia quando o próprio gerador não acompanha a chegada simulada. `unwritten` registra o corte; o total publicado não inclui esses registros.

O script registra carga/hardware, hash do executável, memória/CPU e recibo, mantém pelo menos 24 GiB livres além do orçamento calculado e não captura adaptadores, configura roteamento ou envia dados à IA. Cada ensaio pode gravar até 8 GiB. O teste de quadros pequenos precisa medir pacotes/s separadamente: aprovação com quadros grandes não cobre essa carga.

No ensaio local de 11/10/2026, 20 mil pacotes/s deixou de descartar na fila. Cargas solicitadas de aproximadamente 10 Gb/s ainda tiveram perdas, com cerca de 5,1 Gb/s publicados para quadros de 1500 bytes e 6,0 Gb/s para 9000 bytes em duas medições de cinco segundos. Houve variação expressiva entre ensaios; não são uma garantia de vazão sustentada. A placa física do host opera a 1 Gb/s. O próximo gate exige tráfego conhecido em uma placa física de 10G, armazenamento compatível, duração maior e contagem externa de enviados/recebidos, além de fila/driver, integridade e responsividade. Os adaptadores Hyper-V exibindo 10G não aprovam esse gate.

O orçamento também precisa acompanhar a coleta: 10 Gb/s de dados equivalem a 1,25 GB/s, cerca de 75 GB por minuto antes do cabeçalho PCAP. O limite atual de 64 GiB representa aproximadamente 55 segundos nesse volume. Capturas longas exigem orçamento/ponto de coleta/filtro adequados ou uma política futura de retenção/amostragem escolhida explicitamente; o NetLane para ao atingir o limite e preserva as evidências existentes.

O QA inicial de diagnóstico exige QA-01 a QA-11 e QA-13 aprovados no alvo, ou uma exclusão de escopo registrada pelo responsável. QA-12 é o gate separado de entrega pelo instalador. Resultados automatizados não preenchem os casos manuais. Falha de integridade, perda não contabilizada, arquivo incompleto anunciado como válido, interface bloqueada ou resposta/report ligado ao contexto errado bloqueiam o candidato.

Armazenamento protegido de investigações, pseudonimização, PDF, Claude e Gemini são evoluções futuras. Além do caderno Markdown/JSON, a opção investigação com capturas copia os dumps integrais referenciados, baselines e relatórios; ao reabrir, confere todos os arquivos e restaura o dump atual e primeiro pacote selecionado. Outros contextos podem reabrir seu dump pelo botão Reabrir dump da seleção. A retomada prepara trechos editáveis da mesma captura para revisão e envio explícito em uma nova conversa do provedor. Os arquivos permanecem em texto aberto; hashes não autenticam o autor nem confirmam hipóteses. A integração principal deste ciclo é Codex.

### Continuidade da implementação e recuperação de pacotes

Em 11/10/2026, o usuário deixou a homologação de velocidade para um teste real posterior. A implementação continua considerando desktops com interfaces de 2,5G e a necessidade eventual de observar portas de servidores de 10G. Essas capacidades continuam metas de QA; os ensaios sintéticos não homologam nenhuma dessas taxas.

A tabela permite **Abrir por número**, e cada achado oferece **Abrir #N** para suas referências no arquivo atual. Um pacote fora das amostras é recuperado do dump fechado em segundo plano, com progresso/cancelamento, leitura integral e SHA-256 conferido antes da entrega. As 512 amostras continuam intactas; no máximo uma prévia adicional de 2 KiB fica nos detalhes. A busca e a exportação sem seleção continuam percorrendo as amostras distribuídas. O pacote recuperado selecionado pode ser exportado ou preparado para Codex, sem envio automático ou payload no contexto. Troca de segmento cancela a leitura anterior; números pertencem a cada arquivo, não à coleta inteira.

O QA automatizado cobre PCAP em ambos endians/precisões, PCAPNG com interfaces e seções distintas, pacote sem horário, referência fora das amostras, modificação depois do pacote solicitado, truncamento, cancelamento após encontrar a referência, arquivo ausente e substituição limitada da prévia adicional. A 23ª cena GPUI usa o worker real sobre uma fixture local. O QA manual ainda precisa exercitar os botões dos achados, a digitação do número, a recuperação durante coleta real e a revisão/envio com uma conta Codex. Abertura de caderno continua sem dumps; a opção investigação com capturas restaura o dump e primeiro pacote selecionado, conforme QA-13.
