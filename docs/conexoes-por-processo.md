# Conexões observadas por processo

Entrega local de 2026-09-07. A seção **Conexões** mostra os processos que têm conexões TCP/IPv4 estabelecidas no instante da coleta, sem iniciar o serviço, pedir elevação ou mudar regras, rotas e adaptadores.

Atualização em 2026-09-09: revisão visual básica, pesquisa OneDrive e nova leitura visível após suspensão/retomada conferidas por capturas do usuário, com leitura nativa independente. O ciclo foi encerrado normalmente e a restauração final verificada. Os detalhes abaixo distinguem a prova obtida dos limites ainda não exercitados.

## O que a tela informa

- **Aplicativo / processo:** nome, PID e caminho do executável. PIDs diferentes continuam separados, inclusive para o mesmo aplicativo. Se o acesso estiver restrito ou a identidade tiver mudado durante a coleta, aparece o PID com uma explicação, sem associação por nome.
- **Regra configurada:** associação pelo caminho completo, sem distinção de maiúsculas, com a escolha e o estado que o editor já apresenta. Alterações não salvas e arquivo indisponível são explicitados. Auxiliares não herdam uma associação visual: “Sem regra direta” não afirma ausência de toda política do Windows ou de uma regra indireta.
- **Saída observada · IP local:** correspondência entre o endereço local de cada conexão e os endereços das interfaces conectadas. Considera endereços secundários e interfaces fora da seleção da Visão geral. Mostra todas as saídas encontradas para um PID; IP sem correspondência ou presente em placas diferentes resulta em “Não identificada”. O tooltip detalha a contagem por interface/IP.

A pesquisa aceita nome, caminho, PID ou nome de interface; o filtro **Com regra direta** inclui regras desativadas. A tabela não edita nem salva regras. As linhas são atualizadas preservando seleção enquanto a mesma instância do processo continuar presente.

## Coleta e limites

A UI agenda a leitura a cada 2 segundos em segundo plano, sem sobrepor consultas e sem depender do monitor de bytes ou da sessão elevada. Falhas limpam as linhas anteriores; amostras com mais de 10 segundos são retiradas, inclusive quando outra consulta estiver em andamento. Uma leitura concluída depois de fechar a janela não atualiza a UI.

O coletor usa [GetExtendedTcpTable](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedtcptable), IPv4 com PID, respeitando dimensionamento, crescimento da tabela e [alinhamento das linhas](https://learn.microsoft.com/en-us/windows/win32/api/tcpmib/ns-tcpmib-mib_tcptable_owner_pid). Erros nativos não viram uma lista vazia aparentemente bem-sucedida. A identidade é consultada com acesso limitado, usando [QueryFullProcessImageNameW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew) e [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes) no mesmo handle, sem cache persistente de PIDs.

Isso é observação de endpoints, não captura de pacotes nem verificação de um arquivo transferido:

- Inclui conexões TCP estabelecidas de Internet e rede local; exclui escuta, loopback, conexões em abertura/fechamento, UDP/QUIC e IPv6.
- Não mede bytes ou velocidade por app, não mantém histórico e pode perder conexões curtas entre amostras.
- IP local associado a uma interface não certifica todo o caminho externo, especialmente com VPN, proxy ou tunelamento.
- Não prova que uma política causou a escolha nem que todo o tráfego do aplicativo usa aquela interface. Uma conexão antiga pode continuar na Wi-Fi depois de parar o serviço.
- Um processo pode encerrar entre a leitura da tabela e a consulta de identidade. Nesse caso, a próxima amostra corrige a lista; não há atribuição por coincidência de nome.

## Validação

Build Release sem avisos/erros; suíte de **190 testes** aprovada. Novos casos cobrem endereços secundários, múltiplas interfaces/PIDs, nomes iguais em caminhos diferentes, identidade restrita ou reutilizada, erros de leitura nativa, tabela vazia/crescente/malformada, dados vencidos, recuperação da leitura, filtros e edições não salvas. Os testes WPF verificam bindings, árvore de acessibilidade, seleção, consulta fora da thread da UI, ausência de sobreposição/publicação após fechar e layout em 1000×650, 1220×810 e 1440×920.

Execução isolada usada neste ciclo:

```powershell
dotnet build NetLane.sln -c Release --artifacts-path artifacts/process-connections
$env:NETLANE_TEST_RENDER_PATH = "$PWD\artifacts\process-connections\renders\all.png"
$env:NETLANE_PROCESS_SNAPSHOT_PATH = "$PWD\artifacts\process-connections\native-snapshot.json"
dotnet test tests/NetLane.Tests -c Release --artifacts-path artifacts/process-connections --no-build --logger "trx;LogFileName=process-connections.trx" --results-directory artifacts/process-connections/test-results
```

O smoke test `WindowsConnections` leu o Windows real sem elevação. O snapshot opcional acima grava apenas identidade resumida, PID, IP/interface locais e contagens: não exporta destinos remotos nem conteúdo transferido. Continua sendo informação local que deve ser revisada antes de compartilhar. Sem essa variável, a lista não é persistida.

As renderizações foram conferidas, mas a troca da janela antiga pela nova não foi concluída nesta execução: o controle de desktop retornou `coordinate input geometry is unavailable` e, na recuperação, `failed to activate captured window`. A janela antiga foi preservada; a versão nova compilada está em `artifacts/process-connections/bin/NetLane.UI/release/NetLane.UI.exe`. A validação interativa dessa versão permanece pendente; os testes em memória não substituem essa conferência.

A leitura real encontrou o OneDrive principal ainda com conexão Wi-Fi e o executável separado `OneDrive.Sync.Service.exe` na Ethernet, com o serviço de roteamento parado. Esse auxiliar não estava coberto pelo ensaio anterior, limitado ao `OneDrive.exe`; nenhuma regra adicional foi criada. Não se deve usar essa amostra posterior como prova da saída de todos os componentes durante a sincronização anterior.

Próxima etapa: combinar testes de recuperação e reinício. Esta entrega não desliga Wi-Fi, suspende o computador, reabre OneDrive/Steam nem publica ou instala a aplicação.

Na retomada das 22:13, a janela anterior já estava fechada e a versão nova foi aberta sem elevação, PID `37916`. A árvore de acessibilidade respondeu com as quatro seções. A ativação da janela ainda falhou, portanto a conferência interativa de Conexões não foi declarada concluída. O serviço permaneceu parado; veja a [preparação dos ensaios de recuperação](testes-recuperacao.md).

Em 2026-09-09, o [ensaio de perda/retorno da Wi-Fi](testes-recuperacao.md#perda-e-retorno-da-wi-fi--concluído-com-limpeza-final) foi concluído com limpeza final e uma nova conexão TCP/IPv4 do OneDrive observada na Wi-Fi após a recuperação. A captura enviada pelo usuário confirmou o recibo de parada em **Diagnóstico**; a conferência interativa específica de **Conexões** continua pendente. A preparação usou um novo build isolado, com os mesmos 190 testes aprovados.

Após o usuário autorizar os passos seguintes, a leitura nativa de 05:32:29 voltou a passar e registrou 16 processos, incluindo `OneDrive.exe` (PID `19836`) e `OneDrive.Sync.Service.exe` (PID `21976`), com uma conexão TCP/IPv4 na Ethernet cada. O serviço estava parado. Esse snapshot é uma referência temporal para a revisão manual, não uma expectativa fixa para contagens futuras. A automação da janela segue indisponível; foi solicitada a conferência da página com a pesquisa OneDrive. Veja a [preparação de suspensão/retomada](testes-recuperacao.md#suspensãoretomada-e-conexões--preparação-autorizada-em-2026-09-09).

### Conferência visual básica concluída — 2026-09-09

A captura fornecida pelo usuário mostra a busca `onedrive` funcionando no filtro **Todos os processos**, com leitura às **05:35:10**. O processo principal apresenta regra Wi-Fi conectada e saída observada Ethernet; o auxiliar aparece em outra linha, sem regra direta, também na Ethernet. Os PIDs, caminhos e a contagem de uma conexão TCP/IPv4 por linha estão legíveis. A tela preserva a distinção entre configuração e observação, com o serviço parado naquele instante.

Essa conferência cobre o recorte e a pesquisa operada pelo usuário. O avanço do temporizador, tooltips, filtro de regra direta e navegação por teclado não são demonstrados por uma única imagem. A nova leitura após suspensão/retomada foi conferida separadamente, abaixo. Registro anterior à suspensão: `artifacts/sleep-resume/20260909-053110/connections-ui-review.json`.

### Leitura nativa após a retomada — 2026-09-09

Após a suspensão real confirmada pelos eventos do Windows, o smoke test `WindowsConnections` passou novamente (1 aprovado). A leitura das **05:49:34**, em 49 ms, encontrou o mesmo `OneDrive.exe` PID `19836`, com quatro conexões TCP/IPv4 na Wi-Fi `192.168.0.102`. O auxiliar `21976` continuava vivo, mas não apareceu com conexão estabelecida nessa amostra. Nenhuma regra adicional foi criada.

Evidências: `artifacts/sleep-resume/20260909-053110/connections-after-resume.json` e `test-results/connections-after-resume.trx`. Essa leitura nativa, sozinha, não prova que a janela avançou seu horário ou redesenhou as linhas; a confirmação visual veio depois.

### Conferência visual após a retomada e encerramento concluídos

A captura enviada pelo usuário mostra **Leitura às 05:58:36**, **Serviço ativo**, pesquisa `onedrive` e filtro **Todos os processos**. O mesmo `OneDrive.exe` PID `19836` aparece com caminho legível, regra Wi-Fi conectada/aceita e saída observada **Wi-Fi · 192.168.0.102**, com uma conexão TCP/IPv4. O horário e a linha avançaram em relação à captura anterior à suspensão. Uma conexão às 05:58 e quatro às 05:49 são contagens de instantes diferentes; não há expectativa de contagem fixa nem inferência de erro pela ausência do auxiliar sem conexão estabelecida.

A captura seguinte confirmou o recibo de restauração após a parada normal. A consulta independente confirmou serviço ausente, `routepolicies` IPv4/IPv6 desativados e regras/endereço/rotas/métricas preservados. Registro visual: `artifacts/sleep-resume/20260909-053110/ui-final-review.json`. O [ensaio completo](testes-recuperacao.md#conclusão-da-suspensãoretomada-e-limpeza-final) passou em 28 verificações e foi encerrado.

Isso conclui a conferência visual básica e da nova leitura após retomar, não toda a matriz interativa: imagens estáticas não distinguem disparo pelo temporizador, navegação ou botão **Atualizar**, nem validam atualização contínua, tooltips, filtro de regra direta, teclado ou outros tamanhos de janela.
