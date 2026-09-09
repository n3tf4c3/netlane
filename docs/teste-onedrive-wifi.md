# Validação do OneDrive na Wi-Fi

O alvo deste ensaio é `C:\Program Files\Microsoft OneDrive\OneDrive.exe`, na Wi-Fi selecionada em `src/NetLane.Service/netlane-rules.json`. A regra anterior do Steam está desabilitada. O teste exige que somente o OneDrive tenha uma regra ativa e não reinicia nenhum aplicativo.

## Preparação

Compile o serviço sem elevação:

```powershell
dotnet build "C:\Codes\netlane\src\NetLane.Service\NetLane.Service.csproj" --configuration Release
```

Confira o alvo, a interface, as conexões atuais e as opções do Windows sem alterar a rede:

```powershell
& "C:\Codes\netlane\scripts\test-onedrive-wifi.ps1" -CheckOnly
```

## Manter ativo durante a sincronização

O serviço precisa permanecer ativo durante toda a validação das pastas. Abrir a UI e salvar uma regra não iniciam o motor por conta própria. A versão atual permite [iniciar a sessão em Diagnóstico](controle-servico.md); mantenha essa janela aberta e pare pela mesma janela ao concluir.

O procedimento a seguir é uma alternativa pelo terminal, com coleta de evidências. **Não o execute junto da sessão iniciada pelo painel.**

Em um PowerShell **como Administrador**:

```powershell
& "C:\Codes\netlane\scripts\test-onedrive-wifi.ps1" -UntilStopped
```

Esse modo não tem horário de encerramento. O serviço continua até a solicitação de parada ou uma falha detectada. Para encerrar e restaurar as opções globais, execute em outro PowerShell; este comando não exige elevação:

```powershell
& "C:\Codes\netlane\scripts\test-onedrive-wifi.ps1" -Stop
```

O comando apenas cria uma solicitação de parada na pasta da sessão. O controlador administrativo encerra o serviço que iniciou, restaura as opções que habilitou e publica o resultado. Aguarde `State: Finished` em `artifacts/onedrive-wifi/active-session.json`. Isso não instala um serviço com inicialização automática nem persiste opções globais após reiniciar o Windows.

## Executar uma prova com tempo limitado

Em um PowerShell **como Administrador**, execute uma única linha:

```powershell
& "C:\Codes\netlane\scripts\test-onedrive-wifi.ps1"
```

O ensaio dura 180 segundos. Use `-DurationSeconds 300` para cinco minutos; o limite é 900 segundos. O script recusa a execução se outro `NetLane.Service` estiver ativo.

O script registra o estado inicial de `routepolicies` em IPv4/IPv6, habilita somente o necessário com `store=active` e inicia sua própria instância do serviço em segundo plano. A API atual requer ambas as opções habilitadas, mesmo quando a interface só oferece IPv4. Essas opções são globais do Windows; os filtros deste teste têm somente o AppId do OneDrive como alvo.

Ao terminar ou encontrar uma falha, encerra a instância do serviço criada pelo teste e restaura as opções que alterou. A sessão dinâmica WFP é removida ao encerrar esse processo. Não muda gateways, métricas, DNS, firewall, adaptadores nem arquivos sincronizados.

Não encerre à força o PowerShell que conduz o teste: isso impediria seu bloco de restauração de terminar. Use `-Stop` para acionar a limpeza. No modo com tempo limitado, o prazo também aciona a limpeza enquanto esse processo permanece vivo. A configuração global usa somente o armazenamento ativo, sem persistência após reiniciar o Windows.

## Medir

O diretório `artifacts/onedrive-wifi/<execução>` recebe:

- `progress.json`: processo do serviço, confirmação da política e conexões TCP observadas durante o ensaio.
- `service.log` e `service-error.log`: mensagens do serviço.
- `result.json`: estado inicial, confirmação recebida, todas as amostras, falhas e resultado da limpeza.

O histórico em memória retém as últimas 1.800 amostras; `TotalSamples` e `SamplesTruncated` informam se amostras mais antigas foram descartadas em sessões longas. `EndsAtUtc: null` e `Mode: UntilStopped` identificam a sessão sem prazo.

Políticas só afetam conexões novas. Uma conexão aberta antes do ensaio pode continuar na Ethernet. Para gerar atividade, pause e retome a sincronização pelo ícone do OneDrive, conforme o [suporte da Microsoft](https://support.microsoft.com/en-us/onedrive/how-to-pause-and-resume-onedrive-sync). Se o cliente mantiver a conexão antiga, encerre-o normalmente pelo menu e abra-o novamente.

A evidência necessária é uma conexão externa do PID do OneDrive, criada após a aplicação da política, com `LocalAddress` igual ao IPv4 da Wi-Fi. A mensagem de política aceita, isoladamente, não prova roteamento. Observe também se a sincronização progride; tráfego TCP estabelecido não certifica a conclusão de uploads/downloads.

Este script observa TCP. Não certifica UDP/QUIC, IPv6, todos os componentes da suíte Microsoft ou sincronização prolongada. Depois da limpeza, conexões já abertas na Wi-Fi podem continuar nela; conexões novas voltam à seleção normal do Windows.

A regra OneDrive → Wi-Fi continua salva após o teste, mas não há roteamento ativo quando o serviço encerra. O status `Ready` deixado pelo serviço também precisa ser interpretado com a existência do PID e a validade do heartbeat.

## Ensaio de 2026-09-07

- Serviço Release compilado com zero erros/avisos; somente `OneDrive.exe` recebeu política, aceita para IPv4 na Wi-Fi.
- Entre 08:16 e 08:19 (America/Cuiaba), foram coletadas 77 amostras. A única conexão TCP observada usava `192.168.15.5` e já existia desde 06:58, antes da política. Nenhuma conexão nova foi observada; a saída real pela Wi-Fi continua sem comprovação.
- O serviço do ensaio foi encerrado, `routepolicies` voltou a desativado nas duas famílias e as rotas padrão/métricas permaneceram iguais. Nenhum processo do Steam ou do OneDrive foi reiniciado.
- Relatório local: `artifacts/onedrive-wifi/20260907-081608-1352f736/result.json`. A configuração anterior foi preservada em `artifacts/onedrive-wifi/netlane-rules-before-20260907-081425.json`.

## Segundo ensaio de 2026-09-07: novas conexões na Wi-Fi

- O usuário encerrou o OneDrive normalmente antes do teste. O serviço confirmou a política às 08:40:03, e o OneDrive foi reaberto às 08:40:34, sem elevação, com PID `27196`.
- Entre 08:40 e 08:43 (America/Cuiaba), foram coletadas 77 amostras. Havia zero conexões do OneDrive no início; foram observadas **17 conexões TCP externas distintas**, todas na porta remota `443` e todas com origem `192.168.0.102`, IPv4 da Wi-Fi. Nenhuma conexão externa do OneDrive foi observada pela Ethernet durante o ensaio.
- As conexões foram criadas entre 08:40:34 e 08:40:55, depois da confirmação da política. Isso comprova a seleção da Wi-Fi para o TCP/IPv4 observado nesse processo. Não comprova UDP/QUIC, IPv6 nem a conclusão de sincronização de arquivos.
- O serviço de teste, PID `34776`, encerrou às 08:43. Não houve erros de execução ou limpeza, e `routepolicies` voltou a desativado nas duas famílias. As rotas padrão e métricas verificadas permaneceram iguais. O OneDrive continuou aberto; o Steam não foi alvo de políticas ou reiniciado.
- Relatório: `artifacts/onedrive-wifi/20260907-084002-e13356ef/result.json`. Neste relatório, `AcceptedReceipt` contém o último heartbeat; a primeira confirmação acima foi conferida no `progress.json` durante o teste. O script passou a guardar também `AcceptedAtUtc` nos próximos relatórios para preservar essa distinção.

## Sincronização iniciada após o fim da janela

O ensaio seguinte encerrou automaticamente às **09:02:26**. A captura apresentada pelo usuário começou a medir às 09:04:49 e mostrava o alerta de serviço sem resposta. A consulta confirmou novas conexões de `OneDrive.exe`, PID `30860`, pela Ethernet às 09:04:24, 09:04:28 e 09:05:20, depois de encerrada a política; `routepolicies` já estava desativado nas duas famílias.

O procedimento com prazo não manteve o motor ativo durante a sincronização solicitada pelo usuário. Para essa validação, o modo adotado passa a ser `-UntilStopped`, com encerramento explícito por `-Stop`. Conexões que já estavam na Ethernet precisam ser renovadas com a política ativa.

## Ensaio pelo painel corrigido — 2026-09-07, concluído

O [controle pela interface](controle-servico.md) iniciou a sessão real após o UAC manual do usuário, com serviço PID `3748` e política do OneDrive aceita. O cliente foi reaberto às 16:37:51 (America/Cuiaba), PID `37672`, somente depois da confirmação.

Até 16:44:03, oito amostras registraram **18 conexões TCP externas distintas**, todas com `LocalAddress: 192.168.0.102` (Wi-Fi), porta remota `443` e criação posterior à confirmação da política. Nenhuma conexão externa desse PID foi observada pelo cabo. O serviço continuava ativo e com heartbeat recente; a regra do Steam continuava desabilitada e o JSON real não foi alterado.

Registro parcial até 16:44:03: `artifacts/onedrive-ui/20260907-163733/progress.json`. Naquele momento a sessão permanecia aberta, aguardando o teste de transferência das pastas pelo usuário; sincronização completa e limpeza real pelo botão Parar ainda estavam pendentes. A conclusão está registrada abaixo. As observações validam somente a saída TCP/IPv4 registrada, não UDP/QUIC ou IPv6; os contadores da interface representam todos os aplicativos.

### Transferência iniciada pelo usuário às 21h

Após o usuário informar que iniciou a sincronização, o painel do OneDrive no Explorador mostrou **Baixando arquivos (21%)** e depois **Baixando arquivos (22%)**. Isso acrescenta evidência de download efetivo em progresso, mas não de conclusão. Apenas o resumo de status foi aberto; não foram escolhidas outras pastas nem alterados arquivos ou opções de sincronização.

Entre 21:08:36 e 21:11:42 (America/Cuiaba), duas rodadas somaram **14 amostras** e **11 conexões TCP externas distintas** do mesmo PID `37672`, todas pela Wi-Fi (`192.168.0.102`). O serviço PID `3748` estava ativo, com estado `Ready`, política do OneDrive aplicada e heartbeat recente nas amostras. A configuração real permaneceu com o mesmo hash. Não houve observação contínua no intervalo entre a coleta das 16h e esta retomada; o relatório registra essa lacuna.

A sessão foi mantida ligada durante o download. Até essa coleta, confirmação final de sincronização e teste real de **Parar/restaurar** permaneciam pendentes. O relatório parcial então agregava 22 amostras e 28 conexões TCP distintas; seus contadores de bytes eram da interface inteira, não do processo.

### Conclusão e parada normal

O usuário confirmou **Todos baixados** e enviou uma captura do próprio OneDrive mostrando **Incluído no backup e sincronizado**, com itens baixados no histórico. Essa evidência encerra a validação funcional de sincronização deste ensaio, após o progresso observado anteriormente. O horário exato da conclusão e o conteúdo/checksum individual de cada arquivo não foram auditados.

Antes de parar, às 21:23:45 (America/Cuiaba), o serviço PID `3748` ainda estava ativo, com heartbeat recente, política `Ready` para o OneDrive e `routepolicies` ativo nas duas famílias. A amostra final acrescentou três conexões distintas; o total do ensaio passou a **23 amostras e 31 conexões TCP externas distintas**, todas com origem na Wi-Fi (`192.168.0.102`) e criação posterior à confirmação da política. A coleta foi por amostras e não certifica os intervalos sem observação, UDP/QUIC ou IPv6.

O botão **Diagnóstico → Parar** da janela controladora foi acionado uma vez. Às **21:24:38**, o recibo passou a `Stopped`, sem regras ativas e sem erro. A interface confirmou **Sessão encerrada. Opções temporárias restauradas.** A consulta posterior verificou a saída do PID `3748`, `routepolicies` desativado em IPv4/IPv6 e rotas padrão, gateways e métricas idênticos ao estado anterior à parada. O hash do JSON permaneceu `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.

O OneDrive permaneceu aberto com o mesmo PID `37672`; nenhum processo foi encerrado à força e o Steam não foi alterado. Duas conexões já existentes continuavam na Wi-Fi após a limpeza; isso não indica política ainda ativa, pois conexões existentes não são migradas pela remoção. Não foi provocada uma nova transferência após a parada.

Relatório final local: `artifacts/onedrive-ui/20260907-163733/result.json`, com estados antes/depois da parada e limites da conclusão. `progress.json` também foi marcado como `Finished`. Resultado: sincronização confirmada pelo cliente/usuário, saída TCP/IPv4 observada na Wi-Fi e parada/restauração normal validadas. Reinício real, queda da Wi-Fi e suspensão/retomada permanecem para os próximos ensaios.
