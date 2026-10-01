# Conexão padrão — cabo ou Wi-Fi

Implementada em 2026-09-30 para a prévia local **0.3.1**, em **Visão geral → Conexão padrão**. A 0.3.0 foi removida e a 0.3.1 instalada às **18:19 (America/Cuiaba)**, após autorização específica, com perfil/backup e campos de rede preservados. [Resultado da instalação](instalador-local.md#remoção-da-030-e-instalação-da-031--2026-09-30).

## Uso

1. Confira **Atual**, que mostra a rota padrão IPv4 observada no Windows.
2. Escolha cabo ou Wi-Fi conectada com uma rota padrão IPv4 disponível.
3. Clique em **Aplicar** e confirme o UAC. Apenas selecionar uma conexão não modifica a rede.
4. Use **Restaurar configuração anterior** para recuperar o modo automático ou a prioridade manual existente antes da primeira troca pelo painel.

A escolha permanece após fechar o NetLane e reiniciar o Windows. Não há reaplicação automática ao abrir o painel. A rota padrão atende apps em Automático e tráfego sem uma regra mais específica. Regras por aplicativo continuam independentes, quando o serviço de roteamento está ativo. Conexões já existentes não são reiniciadas pelo painel.

O recurso cobre **IPv4**; não habilita IPv6 nem altera gateway, DNS, firewall, estado das placas ou `routepolicies`. A escolha muda a prioridade das interfaces, não comprova acesso à Internet nem tráfego de um aplicativo. VPNs e rotas mais específicas podem definir outras saídas; uma rota padrão concorrente que impeça a escolha causa recusa antes da gravação.

## Aplicação e recuperação

- Leitura sem elevação com os módulos do Windows `NetAdapter` e `NetTCPIP`, importados por caminho de sistema. Interfaces identificadas por GUID, sem inserir nomes ou caminhos fornecidos pelo usuário em comandos.
- UAC somente ao aplicar/restaurar. O executável irmão `NetLane.Service` atende um comando limitado, independente do motor WFP, por canal autenticado pelo PID exato nas duas pontas e pela identidade/start time da janela controladora. O marcador `default-connection.protocol` evita abrir componentes antigos sem o comando novo.
- [Prioridade é a soma da métrica da interface com a da rota](https://learn.microsoft.com/en-us/windows-server/networking/technologies/network-subsystem/net-sub-interface-metric). A interface escolhida recebe prioridade manual 5; as alternativas envolvidas recebem um valor maior que o total escolhido. Adaptadores virtuais sem participação na escolha ficam fora da gravação.
- [Set-NetIPInterface](https://learn.microsoft.com/en-us/powershell/module/nettcpip/set-netipinterface) é chamado com `AddressFamily IPv4`, sem `PolicyStore`, para aplicar a configuração ativa e salva. O resultado é relido e precisa confirmar as prioridades e a saída escolhida. A leitura compara também a configuração salva; diferenças entre estado ativo e salvo são preservadas e impedem uma troca que perderia essa distinção.
- Recuperação em `%LOCALAPPDATA%\NetLane\default-connection.json`, escrita atomicamente **pela UI sem elevação antes da alteração**. Guarda valores originais e os valores envolvidos na operação; nenhum caminho de perfil é usado pelo componente elevado para gravar arquivos. Esse JSON e backups são excluídos do pacote.
- Falha parcial tenta restaurar as interfaces tocadas em ordem inversa e registra separadamente se a restauração foi conferida. Canal interrompido ou ausência da janela controladora também provoca tentativa de reversão de uma operação sem confirmação de recebimento. Nenhum sucesso é apresentado sem retorno e encerramento do componente.
- Prioridade alterada externamente, arquivo de recuperação inválido, interface ausente e falha de leitura são tratados sem sobrescrever a configuração externa ou descartar os valores de recuperação.
- Aplicação/restauração e ações de início/parada do serviço não podem se sobrepor na mesma janela. Uma sessão de regras por app já ativa pode continuar durante a troca de prioridade. Fechamento/minimização para a bandeja são bloqueados durante a operação.

## Validação e limites

Build e testes locais isolados em `artifacts/default-connection-20260930/`; testes de transação, persistência do registro, troca Wi-Fi → cabo → restauração, cancelamento, falha após escrita parcial, proteção de alterações externas, validação da saída, canal entre processos e fechamento da janela. O processo de prova usa dados sintéticos, sem UAC ou gravação de configurações reais do Windows.

Resultado final: **242/242 testes do projeto principal aprovados**, sem ignorados; recibo `test-results/regression-final.trx`. Dois testes adicionais conferiram a ligação entre a seleção do ComboBox e a escolha do modelo, também aprovados (`selection-binding.trx`). Build Release sem avisos/erros. Scripts de empacotamento analisados sem erros sintáticos.

Leitura real das interfaces foi exercitada; métricas de desenvolvimento conferidas continuam **Ethernet 25 / Wi-Fi 35**, modo automático. Layout renderizado e inspecionado em **1000×650** e **1440×920**, com seletor, Aplicar, restauração e atualização acessíveis. As capturas usam interfaces sintéticas.

**Troca real exercitada pelo usuário após instalar a 0.3.1**, conforme a seção seguinte. Persistência após reiniciar o Windows, restauração real e convivência com tráfego por app ainda não foram verificadas. A interação com o UAC e o fechamento da UI não foram acompanhados diretamente pelo agente; o estado resultante e o registro de recuperação foram lidos. IPv6 das interfaces físicas continua desabilitado. Sem commit/push ou distribuição.

## Troca real exercitada pelo usuário — 2026-09-30

O usuário informou **“funcionou”** e forneceu captura de dois `tracert -d 8.8.8.8`: ambos chegaram ao destino. Um passou por **192.168.15.1** no primeiro salto, o outro por **192.168.0.1**. A captura demonstra caminhos diferentes; a ordem das operações e os cliques de UAC não foram observados diretamente.

Leitura somente do estado às **18:49 (America/Cuiaba)** confirmou:

- **Ethernet:** gateway 192.168.15.1, métrica de rota 0 e de interface **55**, modo manual.
- **Wi-Fi:** gateway 192.168.0.1, métrica de rota 0 e de interface **5**, modo manual; prioridade padrão atual na Wi-Fi.
- **Nenhum processo NetLane aberto:** a prioridade permanece aplicada com o programa fechado; isso não comprova um ciclo de reboot.
- **Registro de recuperação:** `default-connection.json` contém os valores originais automáticos **Wi-Fi 35 / Ethernet 25** e os valores aplicados **5/55**, sem operação pendente (`Previous=[]`). Registro e backup presentes no perfil.
- **Preservação:** hashes das regras principal/backup e bindings IPv6 iguais aos do recibo de instalação. Defender Normal, antivírus/proteção em tempo real/monitoramento de comportamento ativos.

Observação local: `artifacts/default-connection-20260930/user-route-check-20260930.json`. Nenhuma configuração de interface/rota ou início de serviço foi emitido pelo agente nesta conferência. Os recibos de desenvolvimento e instalação permanecem intactos. O teste cobre a mudança observada de prioridade e o caminho até 8.8.8.8; restauração, reboot e tráfego real por app continuam pendentes.

## Prévia instalada

Instalador final: `artifacts/installer/preview-20260930-default-connection-v2/NetLane-0.3.1-preview-win-x64-setup.exe`, **69.436.801 bytes**, SHA-256 **`73371FB21645A35A1DCC2670BBBAF743D61BCE654F22F7FBDB16F7CC6FFADC41`**. Pacote x64 self-contained, ainda sem assinatura, baseado no worktree local do HEAD `e9dc27c`; não é uma release publicada.

Extração estática aprovada: **759 arquivos de payload** conferidos, incluindo manifesto, **758 entradas inventariadas**, dois executáveis x64, UI `asInvoker`, instalador/desinstalador administrativos, marcador do componente novo presente e ausência de configurações pessoais/regras de exemplo. Recibo `archive-check-*/verification.json`. Esse instalador foi executado com UAC manual: **758/758 hashes** conferidos após instalar, registro 0.3.1 e atalho correto, **760 arquivos** totais incluindo desinstalador. Recibos de remoção/instalação em `artifacts/installer-qa-20260930-031/`, ambos com `StageVerified=true` e sequência aprovada. A primeira preparação sem o sufixo `-v2` foi preservada como anterior ao ajuste final de coordenação com o serviço; somente o v2 foi instalado.

Consolidação do desenvolvimento em `artifacts/default-connection-20260930/implementation-review.json`, às **17:44:44 (America/Cuiaba)**: campos IPv4/IPv6 conferidos iguais, regras do checkout e os dois arquivos do perfil preservados, nenhum `default-connection.json` criado no perfil real, Defender ativo. Naquele instante a instalação ainda era 0.3.0 e o painel antigo estava aberto. Esse recibo histórico foi preservado; a remoção/instalação posteriores terminaram às 18:19 com a 0.3.1, perfil/rede iguais e nenhum processo NetLane restante. A troca real feita depois pelo usuário está registrada separadamente acima.
