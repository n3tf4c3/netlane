# Fase 3 — Interface de regras

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

Bandeja, instalador, caminho compartilhado protegido e distribuição ficam para depois desses critérios.

## Histórico do limite de viabilidade

O registro anterior de filtros `2/2` prova instalação de filtros, não redirecionamento de novas conexões. Esse motor foi substituído por `FwpmConnectionPolicyAdd0`, API nativa presente neste Windows. Ela permite selecionar a interface sem callout/driver próprio. As políticas dependem da opção global `routepolicies`, que esta implementação não habilita automaticamente.

`CheckPublicIpPerMappedApplication` na PoC executa `curl --interface` com o IP local de cada placa. Os resultados são uma referência das conexões disponíveis, não medições de tráfego originado em `explorer.exe` ou `steam.exe`.

A pendência original de executar a prova elevada `--verify-routing` foi resolvida para IPv4 TCP/UDP, conforme [evidências do motor nativo](roteamento-nativo.md). Isso não comprova IPv6 nem sincronização completa do OneDrive. Steam deixou de ser o alvo ativo por decisão do usuário. A UI diferencia escolha salva, política aceita e tráfego ainda não verificado; os 58 testes acima e a contagem posterior de 95 pertencem às entregas anteriores.

Referências de implementação: a API [File.Replace](https://learn.microsoft.com/en-us/dotnet/api/system.io.file.replace) permite substituir o arquivo criando backup; o [roteamento nativo](roteamento-nativo.md) documenta a API e os pré-requisitos adotados no motor atual.
