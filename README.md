# NetLane

Cada app, sua rota. Projeto experimental Windows para configurar a conexão de rede por executável.

## Estado atual

Ensaio mais recente concluído em **2026-09-12 às 07:56:33**: [continuidade com painel na bandeja](docs/ensaio-bandeja-roteamento.md#resultado-completo-da-rodada-de-30-minutos), com regra isolada. Seis conexões novas confirmaram Ethernet → Wi-Fi com janela visível/oculta/restaurada → Ethernet após parada. Mesma janela/serviço, fechamento normal, opções temporárias restauradas, rede/regras preservadas e nenhum processo do ensaio restante. Limite da prova: handshakes QUIC/IPv4 do probe, não transferência sustentada nem aplicativos reais.

Rodada com limite de **30 minutos**, encerrada manualmente após cerca de **9 min 31 s** de serviço. Build v2 sem avisos/erros, **37 + 75 testes** reexecutados; os **216 testes** de produção passaram no build anterior deste ciclo, antes da adição de recibos de diagnóstico. A primeira rodada parcial e a preparação recusada foram preservadas. Próxima etapa na ordem combinada: preparação do instalador local, ainda não iniciada; instalação, assinatura, distribuição e publicação continuam separadas.

Revalidação para o commit/push solicitado em **2026-09-12**, antes do instalador: **328/328 testes locais** (216 de produção + 75 de QUIC + 37 do host de bandeja), build Release da solução sem avisos/erros e host de fechamento compilado/conferido com `--check`. Sete scripts PowerShell sem erros de sintaxe. Evidências separadas em `artifacts/prepublish-20260912T1216/`; nenhum ensaio elevado foi repetido. O repositório não possui workflow de CI configurado nesta conferência.

- Monitor de adaptadores, processos conhecidos e conexões TCP/UDP IPv4.
- Serviço com políticas em JSON e roteamento por AppId/LUID usando a API nativa `FwpmConnectionPolicyAdd0` (IPv4/IPv6), sem driver próprio.
- Fase 3 em andamento: [interface WPF renovada](docs/interface-desktop.md), com navegação lateral, cartões de conexão, editor de regras e diagnóstico do serviço. Inclui pesquisa, habilitação e persistência com backup.
- [Controle de sessão pelo painel](docs/controle-servico.md): iniciar, parar e reiniciar com UAC e canal local autenticado. Sem instalação permanente; a sessão termina ao fechar a janela. O [ciclo real iniciar → reiniciar → parar](docs/testes-recuperacao.md) foi validado com restauração das opções temporárias e preservação de regras, rotas e métricas.
- [Bandeja do Windows](docs/bandeja-windows.md): minimizar esconde o painel sem encerrar a sessão; o ícone permite abrir, consultar diagnóstico e acessar os mesmos controles do serviço. **X** e **Sair** mantêm o fechamento seguro. Implementação local com 216 testes aprovados; ícone, menu e uso básico de minimizar/restaurar confirmados pelo usuário. Os limites da revisão estão registrados.
- [Recuperação após perda da Wi-Fi](docs/testes-recuperacao.md#perda-e-retorno-da-wi-fi--concluído-com-limpeza-final): retirada e reaplicação da política observadas, com nova conexão TCP/IPv4 do OneDrive na Wi-Fi e limpeza final confirmada. As 18 verificações passaram; uma variação da métrica automática anterior à interrupção está registrada no relatório.
- [Suspensão/retomada](docs/testes-recuperacao.md#conclusão-da-suspensãoretomada-e-limpeza-final): ciclo real concluído na mesma sessão, com novas conexões TCP/IPv4 do OneDrive na Wi-Fi, Conexões conferida antes/depois e parada normal com restauração verificada. **28 verificações aprovadas**, sem alteração das regras, endereços, rotas padrão ou métricas em relação à referência deste ciclo.
- Painel de consumo por interface: download/upload em Mbps, gráfico de 60 segundos e volume recebido/enviado/total durante a medição.
- [Conexões de apps](docs/conexoes-por-processo.md): aplicativo/PID, regra configurada e interface/IP local observados nas conexões TCP/IPv4 estabelecidas. Coleta somente leitura, sem elevação e independente do serviço.
- [Ensaio QUIC/IPv4 isolado](docs/ensaio-quic-ipv4.md): suporte, baseline e sequência elevada Ethernet → Wi-Fi → controle Ethernet verificados com processos novos, ALPN `h3` e IP local observado. Autorização/UAC manual, regras reais preservadas e limpeza confirmada em 33 verificações independentes. Limite: handshake do executável de prova, não download HTTP/3 ou QUIC de aplicativos reais. Sem ativar IPv6 ou iniciar o serviço comum.
- [Concorrência QUIC/IPv4](docs/ensaio-quic-concorrente.md): dois executáveis isolados com conexões sobrepostas, regras Ethernet/Wi-Fi independentes e inversão das rotas, seguidas de retorno ao controle e limpeza. 75 testes e 61 verificações independentes aprovados. Não comprova transferência contínua de dados; a continuidade com o painel minimizado foi validada no ensaio separado acima.
- Motor de seleção de saída implementado; o [teste real do OneDrive](docs/teste-onedrive-wifi.md#conclusão-e-parada-normal) teve download em progresso, conclusão confirmada pelo cliente/usuário e 31 conexões TCP/IPv4 distintas observadas na Wi-Fi. A parada pelo painel encerrou o serviço e restaurou as opções temporárias, sem mudar rotas, métricas ou regras salvas. A coleta foi por amostras; UDP/QUIC e IPv6 continuam pendentes. Consulte [ativação, prova isolada e reversão](docs/roteamento-nativo.md). Requer API disponível, Administrador e `routepolicies` habilitado no Windows.

O status “conectada” da interface e o status “salvo” da regra não confirmam roteamento. Na sessão iniciada pelo painel, a UI recebe o resultado por named pipe autenticado; o heartbeat em arquivo continua informativo, com revisão e expiração. “Política aceita” confirma o registro pelo Windows, não tráfego medido. Steam inclui seu auxiliar conhecido quando a opção estiver marcada; jogos precisam de regras próprias.

## Executar no Windows

Requer .NET SDK 8. Compile toda a solução para disponibilizar também o executável do serviço:

```powershell
dotnet build "C:\Codes\netlane\NetLane.sln" -c Release
```

Abra a interface sem elevação:

```powershell
& "C:\Codes\netlane\src\NetLane.UI\bin\Release\net8.0-windows\NetLane.UI.exe"
```

Em **Diagnóstico → Controle do serviço**, use **Iniciar serviço** e aprove o UAC. Se necessário, marque antes a autorização explícita de `routepolicies` temporário. Ela começa desmarcada; sem autorização e com as opções desativadas, o início é recusado. **Parar** aguarda a limpeza; **Reiniciar** só prossegue após confirmá-la. Fechar a janela encerra a sessão que ela iniciou. Consulte os [limites e a recuperação](docs/controle-servico.md).

Alternativamente, o serviço pode rodar em outro PowerShell, como administrador. Nesse modo externo ele **não habilita** opções globais e não é controlado pelos botões do painel; consulte os [pré-requisitos do Windows](docs/roteamento-nativo.md#pré-requisito-global-do-windows):

```powershell
dotnet run --project "C:\Codes\netlane\src\NetLane.Service"
```

Reinicie o serviço após atualizar o código. Alterações de regras salvas pela UI são lidas no próximo ciclo do serviço, de 10 segundos por padrão.

O alvo local de teste agora é **OneDrive → Wi-Fi**, com a regra do Steam desabilitada. Para [validar a sincronização](docs/teste-onedrive-wifi.md), mantenha a sessão do painel ativa até concluir o ensaio. O controlador de teste `-UntilStopped` continua como alternativa com coleta de evidências. **Não execute os dois controladores ao mesmo tempo.**

No checkout, a UI usa `src/NetLane.Service/netlane-rules.json`, inclusive quando aberta de outra pasta. O caminho aparece em **Diagnóstico → Configuração local**, onde também é possível copiar um resumo do estado do serviço. Fora do checkout, procura o arquivo junto ao executável da UI; instalação e caminho compartilhado de produção ainda estão pendentes.

Em **Visão geral**, abra **Gerenciar interfaces** e marque as placas desejadas, como Ethernet e Wi-Fi. A preferência é salva automaticamente em `%LOCALAPPDATA%\NetLane\interface-selection.json`; o botão de salvar permite repetir uma gravação que tenha falhado. A seleção controla o painel e as opções oferecidas nas regras do NetLane, sem ligar/desligar placas no Windows e sem remover regras existentes. A seleção inicial prioriza interfaces conectadas com gateway; adaptadores virtuais podem ser selecionados manualmente.

Clique no cartão de uma conexão para ver sua medição. As leituras são atualizadas a cada 2 segundos. **Reiniciar medição** zera somente os totais e o gráfico daquela interface. Os volumes começam quando a UI abre e não são um histórico diário/mensal; incluem tráfego local e Internet de todos os aplicativos. Intervalos com falha de leitura são desconsiderados. As preferências permanecem ao reabrir a UI, mas os totais da medição não são persistidos.

Em **Conexões**, pesquise por aplicativo, PID ou interface e compare **Regra configurada** com **Saída observada · IP local**. Cada PID tem sua própria linha; auxiliares são executáveis separados. A lista inclui interfaces fora da seleção do painel, pode indicar mais de uma saída e retira dados vencidos ou com falha. Não mede bytes por app, não inclui UDP/QUIC ou IPv6 e não prova que a política causou a saída observada. Conexões antigas podem permanecer na mesma interface após parar o serviço.

- “Adicionar aplicativo” seleciona um arquivo `.exe`; a regra começa em Automático.
- Escolher uma placa grava o modo e o GUID correspondente. Uma escolha explícita indisponível não é trocada silenciosamente por outra placa.
- “Automático”, desabilitar ou remover a regra libera o aplicativo no próximo ciclo do serviço.
- “Salvar alterações” grava todas as regras, inclusive as ocultas pela pesquisa.
- A versão anterior fica em `netlane-rules.json.bak`. JSON inválido e conflito com alteração externa são informados sem substituir o arquivo por exemplos.

## Verificar

```powershell
dotnet build "C:\Codes\netlane\NetLane.sln" --configuration Release
dotnet test "C:\Codes\netlane\NetLane.sln" --configuration Release --no-build
```

Os testes automatizados usam arquivos temporários e sessões WFP simuladas, incluindo rollback/ABI/heartbeat. O processo auxiliar `NetLane.ControlProbe` exercita o canal de controle sem motor WFP, `netsh` ou elevação. `WindowsCounters` lê os contadores reais; `WindowsConnections` consulta conexões e identidades de processos em modo somente leitura. Nenhum teste de `dotnet test` instala políticas ou altera a conectividade. A prova opt-in `--verify-routing` aplica políticas temporárias somente ao próprio executável da PoC e exige elevação; não se confunde com a antiga consulta `--check-public-ip`, que usa bind manual no curl.

## Planejamento

- [Visão do produto](docs/Planejamento%20do%20Projeto%20NetLane.md)
- [Fase 0 — evidências e limites da PoC](docs/plano-de-execucao-fase0.md)
- [Fase 1 — monitor](docs/plano-de-execucao-fase1.md)
- [Fase 2 — políticas no serviço](docs/plano-de-execucao-fase2.md)
- [Fase 3 — interface e próximas entregas](docs/plano-de-execucao-fase3.md)
- [Evolução do motor WFP](docs/wfp-engine-roadmap.md)
- [Roteamento nativo — ativar, verificar e reverter](docs/roteamento-nativo.md)
