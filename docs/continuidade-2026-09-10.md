# Ponto de retomada — NetLane

> Marco de versionamento solicitado em **2026-09-12**, após a conclusão do ensaio e antes do instalador: commit/push direto em `main` para a bandeja, hosts/probes, testes e documentação deste ciclo. Revalidação local: **328/328 testes**, Release sem avisos/erros, host de fechamento `--check` aprovado e sete scripts PowerShell sem erros de sintaxe; artefatos novos em `artifacts/prepublish-20260912T1216/`. Não há workflow de CI configurado. Binários, recibos e regras locais ficam fora do commit; nenhum roteamento, instalação ou teste interativo foi reiniciado. As referências abaixo são anteriores a esse pedido; o hash efetivo do marco deve ser conferido no Git, não inferido dos PIDs ou builds históricos.

> Referência mais recente de **2026-09-12 às 07:56:33**: [rodada de 30 minutos concluída](ensaio-bandeja-roteamento.md#resultado-completo-da-rodada-de-30-minutos) em `artifacts/tray-routing/20260912T114202306Z-7128045c/`, build `artifacts/tray-routing-30m-v2-20260912`. Seis fases aprovadas, mesma janela/serviço durante as conexões ativas e retorno à Ethernet após parada. Host histórico 2340 fechado, serviço histórico 38544 encerrado, nenhum probe restante. Rede/regras preservadas, flags restaurados e IPv6 ainda desligado. Não há sessão a retomar nem referência reutilizável. Próxima etapa na ordem combinada: preparação do instalador local, ainda não iniciada; instalação, assinatura, publicação e novos ensaios exigem escopo próprio. Sem commit/push.

> Histórico da nova decisão em 2026-09-12: usuário autorizou outra rodada de bandeja com **30 minutos** para os cliques. Host anterior fechado às 07:31:48, sem serviço. Build `artifacts/tray-routing-30m-20260912`, com 37 + 75 + 216 testes aprovados; preservados os binários/recibos anteriores. No build v2, após adicionar recibos de diagnóstico sem mudar os critérios, foram reexecutados 37 + 75 testes; a suíte de 216 permanece a execução anterior. A sessão real durou cerca de 9 min 31 s, não 30 minutos completos.

> Histórico da primeira rodada em 2026-09-12: [continuidade com painel na bandeja](ensaio-bandeja-roteamento.md) parcialmente executada, com início/UAC manuais e regra isolada. Conexão visível e duas ocultas aprovadas; serviço terminou às 06:43:06 pelo limite de cinco minutos, antes da restauração às 06:59:42. Limpeza/rede/regras conferidas às 07:01:03; o host ainda estava aberto, sem sessão. Restauração ativa e conclusão ficaram pendentes naquela rodada, que não foi convertida em sucesso. Preparação com 26 + 75 + 216 testes aprovados. A pausa e a ordem de entregas abaixo preservam o estado histórico; não reutilizar IPs/PIDs antigos.

**Pausado a pedido do usuário em 2026-09-10, às 21:32 (America/Cuiaba).** Não continuar nem agendar testes durante a pausa. O pedido de salvar não autoriza ativação de roteamento, mudanças de rede, instalador, commit ou push.

> Atualização em 2026-09-11: [QUIC/IPv4 sequencial](ensaio-quic-ipv4.md#resultado-elevado-real-e-conferência-final) concluído às 11:42:15 e [concorrência de dois executáveis](ensaio-quic-concorrente.md) concluída às 12:25:51, após pedidos de continuação dos respectivos cenários e UAC manual. Última conferência independente: 61/61 às 12:26:43; 75 testes do probe/controlador aprovados. Rede/regras preservadas, IPv6 desabilitado e nenhum controlador/probe/serviço restante. Próximo: continuidade com painel minimizado, ainda não executada. Sem commit/push. O restante deste documento preserva o retrato da pausa, não o estado atualizado do projeto.

## Estado salvo

- Repositório: `C:\Codes\netlane`; HEAD `4b13ca16e21d557f7a26a037515a050337d1890e`.
- Implementação e documentação estão salvas no worktree, **sem commit/push**. Há arquivos modificados e novos; preservar todos ao retomar, sem reset, checkout de descarte, limpeza ou staging amplo.
- Última suíte completa: **216 aprovados, 0 falhas, 0 ignorados**, em 45 segundos. O TRX foi relido nesta pausa; os testes não foram executados novamente.
- Artefato: `artifacts/tray-20260910/test-results/tray-full.trx`. Build correspondente: `artifacts/tray-20260910/bin/NetLane.UI/release/NetLane.UI.exe`.
- A consulta de processos nesta pausa não encontrou `NetLane.UI`, `NetLane.CloseReview` nem `NetLane.Service` em execução.
- Hash atual das regras reais: `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`, preservado. Nenhuma regra ou configuração de rede foi modificada para salvar este ponto.

## Ordem combinada e entregas

1. **Validação complementar da interface:** comportamentos básicos confirmados. Inclui atualização automática, pesquisa/filtros, tooltips, foco, descarte com serviço parado e edição durante parada simulada. O último host registrou edição durante a espera, preservação da alteração, recarga sem salvar e fechamento final às 11:25:43. Os limites de DPI, percurso completo do teclado e serviço simulado permanecem explícitos em [validação da interface](validacao-interface.md).
2. **Bandeja:** implementada localmente e confirmada pelo usuário no uso básico, com capturas do ícone/menu e relato de minimizar/restaurar. Minimizar esconde a mesma janela e preserva a sessão; X e Sair mantêm o fechamento seguro. Menu com abrir, diagnóstico e controles existentes do serviço. Sem autostart, serviço permanente ou perfis. Há 17 novos testes; detalhes e limites em [bandeja do Windows](bandeja-windows.md).
3. **Ampliação dos ensaios de rede:** apenas preparação somente leitura e roteiro. Próximo cenário proposto: **QUIC/IPv4 em executável de prova isolado**. O harness ainda não foi implementado e o teste não foi executado.
4. **Instalador local:** não iniciado. Assinatura, distribuição e publicação continuam separados.

## Próxima ação ao retomar

Ler [ampliação dos ensaios de rede](ampliacao-ensaios-rede.md) e revisar o worktree antes de editar. A última pergunta feita ao usuário foi se autorizava o ensaio QUIC/IPv4 isolado com ativação temporária das políticas de rota e UAC manual, sem alterar as regras do OneDrive ou Steam. **Essa autorização não foi concedida: o usuário pediu para pausar.** Não interpretar uma retomada genérica como aprovação de mudanças de conectividade.

Depois de retomar explicitamente, é possível preparar o harness e verificar suporte QUIC/MsQuic sem ativar roteamento. A fase elevada requer confirmação específica. Usar processo de prova, arquivo/regras isolados, conexões novas, evidência de tráfego separada da política aceita e limpeza normal com conferência final. Não iniciar o painel comum com suas regras reais como substituto desse alvo de teste.

## Limites de rede e segurança

- Última observação de rede, **11:28:30**, não repetida durante a pausa: Ethernet/Wi-Fi conectadas, `ms_tcpip6` desabilitado nas duas, sem endereços/rotas IPv6 nessas placas e `routepolicies` IPv4/IPv6 desativados. É um registro histórico; revalidar antes de executar testes.
- Não habilitar IPv6, desconectar placas, suspender/reiniciar o Windows, reiniciar OneDrive, executar Steam/auxiliares ou testar VPN sem combinar esse escopo separadamente.
- Steam e Microsoft Store continuam fora deste ciclo. O resultado de um probe QUIC não comprovará QUIC do OneDrive.
- Não há retomada automática ou teste agendado por esta conversa.

## Principais alterações locais desta conversa

- `src/NetLane.UI/Tray/`: estado/menu nativo e controlador de visibilidade/comandos.
- `App.xaml`/`App.xaml.cs`: criação da bandeja e encerramento atrelado à janela principal.
- `MainWindow.xaml`/`.cs`: dica compacta, comandos reutilizados e proteção contra confirmações simultâneas; construtor interno original de sete parâmetros preservado.
- `NetLane.UI.csproj`: uso do componente Windows Forms, sem biblioteca de terceiros; remoção dos imports globais conflitantes.
- `tests/NetLane.Tests/TrayTests.cs`: 17 cenários da bandeja; `MainWindowTests.cs`: regressão do temporizador de Conexões.
- `tests/NetLane.CloseReview/`: host opt-in para diálogos nativos com sessão/arquivo fictícios, fora da solução e da suíte automática.
- README, plano da fase 3, controle do serviço, Conexões e documentos de validação/bandeja/rede atualizados. Não sobrescrever alterações adicionais do usuário nesses caminhos.
