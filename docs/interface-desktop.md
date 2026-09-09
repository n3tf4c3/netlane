# Interface desktop do NetLane

Renovação de 2026-09-07, mantendo WPF/.NET 8 e as regras existentes. Não adiciona bibliotecas de UI, telemetria ou alterações no motor WFP.

## Organização

- **Visão geral:** cartões selecionáveis das conexões, taxas atuais, gráfico de 60 segundos e totais da sessão. Download em azul contínuo; upload em laranja tracejado. Áreas preenchidas não conectam intervalos sem amostras.
- **Regras de apps:** pesquisa, filtro, seleção da conexão, chave de habilitação e ações de adicionar, remover, recarregar e salvar. As regras ocultas pelo filtro continuam incluídas no salvamento.
- **Conexões:** [saídas observadas por processo](conexoes-por-processo.md), com PID, caminho, regra configurada, interface, IP local e número de conexões TCP/IPv4. Pesquisa e filtro por regra direta; leitura independente do serviço.
- **Diagnóstico:** controle da sessão (iniciar/parar/reiniciar), estado do serviço, último retorno, quantidade confirmada de políticas, orientação contextual, caminho da configuração e cópia de um resumo local. A cópia só ocorre ao clicar e inclui caminhos que devem ser revisados antes de compartilhar.

O tema claro usa recursos centralizados em `src/NetLane.UI/Styles/DesignSystem.xaml`, tipografia Segoe UI, foco por teclado e estilos consistentes de controles. Na inicialização, utiliza as cores de sistema quando o alto contraste estiver ativo. A barra de título e os comandos de janela continuam nativos do Windows. Não há seletor de tema nesta entrega.

## Configuração não é confirmação

- Um PID encerrado tem prioridade sobre a idade do último retorno: o painel mostra **Serviço parado**, mesmo se o arquivo antigo ainda disser Ready.
- Retorno vencido, versão divergente, erro de leitura ou retorno incompleto não produzem confirmação verde.
- Alterações locais aparecem como **Não salva**. Salvar não significa que o serviço já aplicou a nova configuração.
- **Política aceita** significa que o Windows aceitou a política. Não significa tráfego comprovado na interface escolhida.

O painel agora oferece [controle de sessão com UAC e IPC autenticado](controle-servico.md), sem instalação permanente. `routepolicies` temporário exige autorização explícita, desmarcada por padrão, e as opções alteradas são restauradas na parada normal. A UI não reabre aplicativos. O gráfico mede todos os apps na interface, incluindo rede local.

## Verificação reproduzível

```powershell
dotnet build .\NetLane.sln -c Release
```

```powershell
dotnet test .\NetLane.sln -c Release --no-build
```

Renderização WPF opcional com dados e arquivos temporários, sem serviço real:

```powershell
$env:NETLANE_TEST_RENDER_PATH = "$PWD\artifacts\ui-refresh\policy-editor.png"
```

```powershell
dotnet test .\tests\NetLane.Tests -c Release --filter FullyQualifiedName~MainWindowTests
```

As capturas incluem as quatro áreas em 1000×650, 1220×810 e 1440×920 unidades de layout. Em janelas menores o painel de conteúdo rola; navegação e ações de salvar permanecem acessíveis. Os testes também verificam bindings, seleção persistida, estados vazios, filtro, chave de ativação e salvamento completo. São renderizações em memória, não prova de compatibilidade com todos os monitores, leitores de tela ou escalas de DPI.

Na validação da renovação visual, o build Release terminou sem avisos/erros e os **115 testes passaram**. A conferência da janela real revelou que o template sem cabeçalhos ocultava o conteúdo da árvore de acessibilidade. O `WorkspaceHost` agora expõe a página como painel, com regressão automatizada e nova conferência da árvore do Windows. A revisão visual não iniciou o serviço nem modificou as regras reais de OneDrive e Steam. O ciclo de [controle do serviço](controle-servico.md) teve depois início elevado, teste do OneDrive e parada/restauração reais validados. A entrega de Conexões ampliou a suíte para **190 testes**. Essas contagens registram etapas distintas.

## Distribuição futura — fora do escopo atual

WPF pode ser distribuído pela Microsoft Store; não é necessário migrar a UI apenas por esse objetivo. A Microsoft documenta as opções [MSIX e instalador EXE/MSI](https://learn.microsoft.com/en-us/windows/apps/distribute-through-store/how-to-distribute-your-win32-app-through-microsoft-store). A organização visual usa como referência as recomendações de [espaçamento e hierarquia](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics).

Antes de escolher o formato de publicação, ainda precisamos validar:

1. Instalação e atualização do serviço privilegiado, compatibilidade do roteamento e reversão segura.
2. Caminho de configuração fora do checkout, permissões e comunicação segura entre UI e serviço.
3. Empacotamento, assinatura, desinstalação e preservação das preferências.
4. Acessibilidade, escalas de DPI, documentação de privacidade, identidade visual final e requisitos de certificação vigentes.

Esta entrega não cria pacote de distribuição nem confirma aprovação na Store.
