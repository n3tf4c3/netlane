# Revisão manual de fechamento

Host opt-in para conferir os diálogos nativos da janela WPF já compilada, com uma sessão simulada que demora 15 segundos para parar. A janela começa em Regras de apps com uma única regra fictícia habilitada. O binário de produção não é modificado; o construtor interno existente recebe arquivo, adaptadores e serviço de teste. A confirmação de descarte permanece a implementação nativa da UI, sem callback substituto.

O projeto não faz parte da solução ou da suíte automática. Exige o caminho de uma compilação existente e copia suas três DLLs para o diretório do ensaio:

```powershell
dotnet build tests\NetLane.CloseReview -c Release --artifacts-path artifacts/ui-close-review-20260910 -p:NetLaneReviewBuild=C:\Codes\netlane\artifacts\ui-validation-20260910\bin\NetLane.UI\release
```

O executável resultante aceita somente `--check` (instancia e confere a janela em memória, sem exibi-la) ou `--review` (abre a janela para operação manual). Cada execução cria uma pasta própria em `review-runs` junto ao executável, com regras sintéticas, estado sintético e `review.json`. Nenhum arquivo de execução anterior é apagado.

Para a revisão manual:

1. Na janela **ENSAIO DE FECHAMENTO**, clicar no X e confirmar **Sim** para a parada da sessão simulada.
2. Durante os 15 segundos de espera, desativar a única regra fictícia, sem salvar.
3. Quando surgir a confirmação de descarte de alterações não salvas, responder **Não**. A janela deve permanecer aberta com a regra desativada e a alteração pendente.
4. Recarregar e confirmar o descarte da edição de teste. A regra deve voltar a ficar habilitada. Fechar pelo X para encerrar o host.

O relatório registra a ordem e os horários de início da parada, edição durante a espera, conclusão e encerramento. Ele também registra o hash da DLL revisada, a revisão do arquivo sintético e o estado do editor. A escolha no diálogo nativo precisa de confirmação/captura do usuário; o relatório não interpreta pixels nem automatiza entradas.

Na versão 2 do registrador (`RecorderVersion: 2`), as assinaturas de edição acompanham as linhas substituídas por **Recarregar**. O modo `--check` confere duas recargas, uma notificação por edição da linha atual, nenhuma da linha descartada e a preservação do arquivo sintético. Esses eventos são programáticos e ficam identificados com `Mode: "check"`; não comprovam a operação manual dos diálogos. O roteiro registra a mudança de **Ativa**, não notificações de rota que também podem ocorrer na atualização dos adaptadores.

O host não cria `WindowsServiceSession`, motor WFP, comandos de rede, processo de serviço ou pedido de elevação. `StartAsync` é recusado e os botões de início/reinício ficam indisponíveis. Esse ensaio comprova a interface e o fluxo assíncrono com serviço simulado; não comprova restauração de rede real, tratada nos ensaios específicos do projeto.
