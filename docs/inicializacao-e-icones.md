# Inicialização com o Windows e ícones dos aplicativos

Entrega local da prévia **0.3.4**, autorizada em 2026-09-30 por “vamos seguir o recomendado”, após a proposta de completar estes dois pontos do MVP.

**Atualização concluída às 23:53:** [0.3.4 instalada sobre a 0.3.3](instalador-local.md), retorno 0 e StageVerified=true. Perfil de cinco arquivos, rede/IPv6/autostart preservados, Defender ativo; entrada NetLane no Run continua ausente. Ícones, marcar/desmarcar pela UI instalada e logon real ainda aguardam QA. Evidências da preparação abaixo são históricas.

## Iniciar com o Windows

- A opção **Iniciar com o Windows** fica na parte inferior da barra lateral, em todas as páginas. Começa desativada; abrir o painel, consultar o estado ou instalar o pacote não a ativa.
- Ao marcar, o NetLane instalado registra somente a entrada do usuário atual. Não exige UAC. Ao entrar novamente no Windows, abre na bandeja; clicar no ícone reabre a janela normalmente.
- Serviço de roteamento e monitor de qualidade continuam sendo iniciados pelo painel. O logon não aplica regras, muda prioridades ou inicia sondagens.
- Desmarcar remove somente a entrada que corresponde ao executável desta instalação. Uma entrada NetLane de outro local é preservada, com mensagem; erros de gravação mantêm a caixa coerente com o estado consultado.
- A opção é indisponível em builds de desenvolvimento sem o marcador do pacote. O comando é o caminho completo entre aspas seguido de --startup, limitado a 260 caracteres, em HKCU\Software\Microsoft\Windows\CurrentVersion\Run, valor NetLane.
- A caixa representa o registro feito pelo aplicativo. O Windows também pode controlar a execução em **Configurações → Aplicativos → Inicialização**; não se altera StartupApproved.
- A abertura automática usa a presença das janelas deste executável na sessão para evitar outra janela quando ele já estiver aberto. A abertura manual mantém o comportamento anterior. Se a bandeja falhar, o painel fica visível.
- O desinstalador remove apenas o comando exato deste pacote no HKCU da conta que executa a remoção, depois da remoção bem-sucedida dos arquivos. Comandos de outros locais/contas são preservados; UAC com credenciais de outra conta não permite presumir limpeza do usuário original.

Referência: [inicialização por Run/RunOnce](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys).

## Ícones por executável

**Regras de apps** e **Conexões** exibem o ícone do executável, mantendo as linhas compactas. Quando não há caminho local consultável, arquivo acessível ou recurso de ícone, aparece o marcador padrão.

A leitura usa ExtractIconExW, sem iniciar o aplicativo, e libera o HICON com DestroyIcon. Bitmaps congelados podem ser usados pela UI; até duas leituras trabalham em segundo plano, com cache limitado a 128 caminhos, invalidado quando tamanho/data do arquivo mudam. Caminhos remotos e arquivos marcados Offline usam o marcador padrão. Reciclagem da linha, troca de caminho e descarregamento descartam resultados atrasados. Ícones não são gravados no perfil nem exportados no pacote.

Referência: [extração de ícones e liberação dos handles](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-extracticonexw).

## Validação

Evidências em artifacts/startup-icons-20260930/:

- Gate de versionamento em **2026-10-01**: build da solução Release com zero avisos/erros e **283/283 testes aprovados**, sem falhas/ignorados, em artifacts/versioning-20261001-034/test-results/versioning-regression.trx. As 14 fontes de produto/pacote correspondem por hash à revisão que gerou o instalador. Perfil de cinco arquivos, regras reais, UI instalada, entrada de inicialização, métricas/modos/estado IPv4 e bindings IPv6 preservados na rodada; validation.json registra Preserved=true. Confirmação dos ícones e captura de inicialização são evidências separadas abaixo; logon real não foi exercitado.

- Build Release sem avisos/erros e **283/283 testes de produção aprovados**, sem falhas/ignorados, em test-results/startup-icons-regression.trx. Depois foram conferidos os ajustes do host de renderização e a recusa da caixa de seleção em testes focados, sem mudança de código do produto.
- Registro real exercitado em chaves descartáveis de teste, sem usar o Run real: carregar sem escrever, ativar/desativar, caminho com espaços, preservação de entrada externa e indisponibilidade no desenvolvimento. Preferência inicial, falha/recusa e atualização externa também verificadas.
- Abertura na bandeja e restauração exercitadas com janela/serviço de teste, sem iniciar roteamento. Presença de instâncias e interpretação exata de --startup verificadas separadamente.
- Extração nativa do ícone do executável da UI, cache compartilhado/limitado, atualização do arquivo, fallback e descarte de resultados antigos/descarregados conferidos.
- Renderizações finais em renders-v2/startup-icons-rules-1000.png, startup-icons-rules-1440.png, startup-icons-connections-1000.png e startup-icons-connections-1440.png, revisadas. São dados sintéticos, não captura da UI instalada. As primeiras imagens usavam janela hospedeira menor que o conteúdo; corrigido o tamanho do host, sem mudança do layout do produto.
- preservation-result.json, às **23:22:30 (America/Cuiaba)**: Preserved=true; perfil, regras reais, entrada NetLane no Run, métricas/modo/estado IPv4, bindings IPv6 e executável instalado iguais. **0.3.3 permanece instalada**. Nenhum registro real de inicialização foi criado, serviço/sondagem iniciado ou instalador executado.

## Pacote preparado

artifacts/installer/preview-20260930-startup-icons/NetLane-0.3.4-preview-win-x64-setup.exe, **69.481.679 bytes**, SHA-256 **4468E105CCAEB4EC09FCA23A614E158183476B5ED32AED12642AB8390F88604B**.

Pacote Release/self-contained win-x64, .NET 8.0.30/NSIS 3.12, prévia local sem assinatura. Conferência estática aprovada: 759 arquivos, manifesto/arquitetura corretos, UI sem elevação, sem regras/perfil reais e logo extraído igual ao fonte (zero pixels diferentes). Recibo em archive-check-eb0fd66a/verification.json.

## Confirmações do usuário — 2026-10-01

O usuário mostrou a tela **Aplicativos → Inicialização** do Windows com **NetLane.UI — Ativado** e informou **“Apareceu aqui”**. Depois confirmou **“conferi os ícones”**. Isso confirma, por relato/captura do usuário, os ícones na UI instalada e o reconhecimento da entrada pelo Windows. Leitura local durante a preparação do versionamento confirma a versão instalada 0.3.4.0 e o comando esperado, caminho completo da NetLane.UI.exe entre aspas seguido de --startup, no Run do usuário. O agente não executou cliques nem mudou o registro.

O usuário informou que **não pode reiniciar agora** e retornará com o resultado. **Logon real ainda não verificado**; a captura não prova a abertura na bandeja, e desmarcar/remarcar pela UI instalada não foi confirmado separadamente. A limpeza da entrada pelo novo desinstalador ainda tem apenas validação de fonte/compilação. Não houve reboot/logoff ou novo ensaio de tráfego.

Após esclarecer que versionar é uma operação no Git, o usuário solicitou **“salve no ai-memory, commit e push”**. Continuidade durável no escopo default/netlane, página notes/netlane-0-3-4-instalado-2026-10-01.md. Este marco inclui fontes, testes e documentação; o hash e a sincronização efetivos devem ser conferidos no Git. Binários, recibos, perfis, regras reais e configuração/guia local preexistentes não são parte da entrega.

**Próxima etapa:** receber a confirmação de abertura na bandeja no próximo login. Limpeza da entrada na desinstalação, cenários restantes do instalador, restauração/convivência da conexão padrão e assinatura/distribuição permanecem pendentes. O pedido de versionamento não autoriza novos ensaios de rede, publicação binária ou contato com suporte.
