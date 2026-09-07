# Roteamento nativo por executável

Estado em 2026-09-06: implementação disponível para teste elevado; a troca real de saída do Steam ainda não foi validada.

## O que mudou

O motor anterior instalava PERMIT/BLOCK, o que não selecionava a saída. `WfpRoutingEngine` agora usa [FwpmConnectionPolicyAdd0](https://learn.microsoft.com/en-us/windows/win32/api/fwpmu/nf-fwpmu-fwpmconnectionpolicyadd0), com AppId do caminho completo e `FWP_NETWORK_CONNECTION_POLICY_NEXT_HOP_INTERFACE`. A interface é identificada pelo LUID de 64 bits obtido do GUID; o Windows escolhe endereço de origem e próximo salto nessa interface. Não há driver próprio, injeção em processos, proxy local, mudança de métricas ou edição da tabela de rotas.

- Políticas para IPv4 e IPv6, na mesma transação por executável. A atualização remove a versão anterior e adiciona a nova com rollback em erro.
- Só é registrada a família que a placa escolhida realmente carrega (`NetworkInterface.Supports`). Prender IPv6 numa placa com o protocolo desmarcado apontaria o tráfego para uma pilha que não responde. Uma placa sem IPv4 e sem IPv6 é recusada, em vez de aceitar a regra sem aplicar nada. O modo **Bloqueado** continua cobrindo as duas famílias, para que a família não roteável não vire saída.
- Apenas unicast não-loopback é direcionado. Comunicação local do Steam, multicast e broadcast não são desviados.
- O modo **Bloqueado**, quando já configurado, usa filtros separados de bloqueio IPv4/IPv6. Esses filtros não são usados para escolher Wi-Fi/Ethernet.
- A sessão WFP é dinâmica. O Windows remove seus objetos quando ela termina, inclusive no encerramento do processo. Ver [ciclo de vida e transações WFP](https://learn.microsoft.com/en-us/windows/win32/fwp/object-management).
- A ausência de pré-requisitos deixa a política **não aplicada**. O serviço não usa uma simulação como se fosse sucesso.
- Falha na regra, desabilitação, Automático ou ausência da placa removem a política anterior no ciclo do serviço: comportamento **fail-open**, não um mecanismo de impedir vazamentos por outra interface.

## Pré-requisito global do Windows

Este computador possui as APIs nativas. Em 2026-09-07 a verificação encontrou `routepolicies` **habilitado** em IPv4 e IPv6, ativado manualmente num terminal elevado para o teste. A aplicação nunca liga nem desliga essa opção por conta própria.

Atenção ao ler o resultado: `Ipv4RoutePolicies`/`Ipv6RoutePolicies` refletem apenas essa chave global de cada pilha. Não dizem se a placa tem aquela família habilitada — nesta máquina o IPv6 estava desmarcado (`ms_tcpip6`) nas duas placas com `routepolicies` IPv6 em `enabled`. Quem decide as famílias aplicadas é a checagem por placa descrita acima.

A [fonte oficial da documentação da Microsoft](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/fwpmu/nf-fwpmu-fwpmconnectionpolicyadd0.md) exige habilitar o processamento de políticas de rota. Isso é uma opção global: não muda os gateways/métricas, mas pode ativar a avaliação de políticas de outros produtos instalados. Em máquina corporativa/VPN, revise isso com o responsável pela rede antes do teste.

Diagnóstico somente leitura, sem iniciar o serviço:

```powershell
dotnet run --project "C:\Codes\netlane\src\NetLane.Service" --configuration Release -- --check-routing
```

Retorno `0`: pré-requisitos disponíveis (não comprova roteamento). Retorno `2`: API, privilégio ou opção do Windows ausente/não verificável. A leitura de `netsh` reconhece os rótulos em português/inglês; outro idioma fica não verificável, nunca presumido habilitado.

Para autorizar e habilitar **temporariamente** o teste, execute em PowerShell **como Administrador**:

```powershell
netsh interface ipv4 set global routepolicies=enabled store=active
netsh interface ipv6 set global routepolicies=enabled store=active
```

Verifique o sucesso das duas chamadas. `store=active` não grava essa mudança permanentemente: ela dura até a próxima inicialização. A aplicação não executa esses comandos por conta própria.

## Prova isolada antes de testar o Steam

Encerre instâncias antigas do NetLane antes de recompilar. Com os pré-requisitos disponíveis, no terminal administrativo:

```powershell
dotnet run --project "C:\Codes\netlane\poc\NetLane.NetworkPoC" --configuration Release -- --verify-routing
```

O teste exige exatamente uma Wi-Fi e uma Ethernet conectadas com gateway. Se houver mais de uma candidata, informe os GUIDs com `--wifi-interface "GUID" --ethernet-interface "GUID"`.

O alvo é somente `NetLane.NetworkPoC.exe`, em processos filhos novos. **Não aplica regras ao Steam, Explorer ou dotnet.exe e não altera o JSON de regras.** Faz controle sem política, mede Ethernet, troca para Wi-Fi e mede novamente; remove suas políticas e repete o controle automático. Uma divergência entre a interface pedida e os IPs locais medidos é falha, mesmo que a API tenha aceitado a política.

- TCP: HTTPS em `api.ipify.org`, registrando o endereço local do socket e o IP público retornado.
- UDP conectado: consulta DNS A de `example.com` a `1.1.1.1:53`, verificando a resposta e o endereço local do socket.
- UDP **não conectado** (o padrão da Steam: um socket, vários destinos por `sendto`): requisição STUN Binding a `stun.l.google.com:19302` e depois a `stun.cloudflare.com:3478`, no mesmo socket. Um socket não conectado nunca fixa endereço local — `LocalEndPoint` fica em `0.0.0.0` — então a origem só é observável perguntando ao servidor qual ela foi. O critério é que o IP público visto pelos dois STUN seja igual ao que o TCP obteve; divergência significa identidade do aplicativo repartida entre dois links, que é exatamente o que derruba a sessão da Steam.
- Não faz bind de origem, não usa `IP_UNICAST_IF` e não usa `curl --interface`. Não segue proxy do ambiente.
- O resultado depende também do acesso a esses destinos; timeout não prova, sozinho, defeito no motor.
- Esta prova cobre **IPv4 TCP/UDP**. A implementação registra IPv6, mas a prova de tráfego IPv6, concorrência entre aplicativos, encerramento abrupto e compatibilidade VPN ainda estão pendentes.
- Retornos: `0` passou; `1` falhou/incompleto; `2` pré-requisitos ausentes, sem aplicar políticas.

## DNS: por que não dá para rotear por aplicativo

O NetLane **não roteia o DNS do aplicativo, e não há como fazê-lo** com políticas de conexão. O motivo é a arquitetura do Windows, não uma lacuna da implementação:

1. O aplicativo não resolve nomes. Quem resolve é o serviço **DNS Client** (`Dnscache`), hospedado em `svchost.exe`. A política do NetLane casa por AppId, e o AppId dessa consulta é o `svchost.exe`, não o `steam.exe`.
2. A consulta que sai do `Dnscache` não carrega nenhuma identidade de quem pediu. Mesmo interceptando, não dá para separar a consulta da Steam da consulta de qualquer outro processo.
3. Aplicar a política ao `svchost.exe` prenderia o DNS da máquina inteira num link só — e, como cada link só enxerga o resolvedor do próprio link, quebraria a resolução em vez de corrigi-la.

Medição nesta máquina (2026-09-07), consultando cada resolvedor com o socket amarrado em cada placa:

| origem | resolvedor da Ethernet | resolvedor da Wi-Fi | `1.1.1.1` |
| --- | --- | --- | --- |
| Ethernet | responde | sem resposta | responde |
| Wi-Fi | sem resposta | responde | responde |

Consequência prática: o DNS sai pelo link de menor métrica (a Ethernet, aqui) e devolve endereços escolhidos para o ISP daquele link, enquanto as conexões saem pelo link da política. Isso deixa a escolha de CDN pior, mas **não reparte a identidade da sessão** — o que os servidores da Steam enxergam é o IP de origem da conexão, não o de quem consultou o DNS. Foi verificado que os IPs devolvidos pelo resolvedor da Ethernet são alcançáveis normalmente a partir da Wi-Fi.

Mitigação possível, se a divergência de CDN incomodar: configurar nas duas placas um resolvedor público alcançável pelos dois links (`1.1.1.1`, `8.8.8.8`), o que torna a resposta igual independentemente do caminho. É uma configuração de máquina, reversível, e o NetLane não a aplica sozinho.

## Testar a regra salva do Steam

1. Feche as instâncias antigas da UI/serviço do NetLane para carregar o código novo.
2. Inicie o serviço no terminal administrativo e mantenha-o aberto:

   ```powershell
   dotnet run --project "C:\Codes\netlane\src\NetLane.Service" --configuration Release
   ```

3. Abra a UI em outro terminal (não precisa de elevação):

   ```powershell
   dotnet run --project "C:\Codes\netlane\src\NetLane.UI" --configuration Release
   ```

4. Confirme o Steam na Wi-Fi e salve. A caixa **Incluir steamwebhelper.exe** cobre o auxiliar conhecido encontrado na pasta do Steam. Não inclui jogos ou filhos arbitrários. Uma regra explícita do auxiliar, inclusive Automático/desabilitada, prevalece sobre a herança.
5. Aguarde a coluna **Estado no serviço** informar política aceita; isso confirma o registro, não a saída medida. Se houver erro, não continue supondo aplicação bem-sucedida.
6. Feche e reabra o Steam normalmente para gerar novas conexões (não mate processos durante downloads/gravações). Políticas não migram conexões já estabelecidas. Inicie uma operação de rede e confira os endereços locais:

   ```powershell
   $steamProcessIds = @(Get-Process -Name steam,steamwebhelper -ErrorAction SilentlyContinue).Id
   Get-NetTCPConnection -State Established | Where-Object { $_.OwningProcess -in $steamProcessIds } |
       Select-Object OwningProcess,LocalAddress,RemoteAddress,RemotePort
   Get-NetIPAddress -InterfaceAlias 'Wi-Fi','Ethernet' |
       Select-Object InterfaceAlias,AddressFamily,IPAddress
   ```

   Compare conexões externas, desconsiderando loopback. Confirme também IPv6 se disponível; monitoramento por IP local não substitui captura de pacotes em cenários com VPN/proxy/weak-host. O gráfico da UI soma todos os aplicativos, não é uma prova isolada do Steam.

O serviço publica um heartbeat informativo junto ao JSON (`netlane-rules.json.runtime.json`), com revisão do arquivo, horário, motor e resultados por executável. A UI rejeita confirmação antiga (mais de 25 segundos), revisão diferente e estado inválido; atualiza a leitura a cada 2 segundos.

O encerramento gracioso publica `Stopped`, mas um encerramento abrupto não publica nada: o último retrato continua dizendo `Ready` para sempre. Por isso a UI também confere se o `ProcessId` do heartbeat ainda existe e, se não existir, informa que o serviço foi encerrado e que o Windows já removeu as políticas da sessão. **Ao ler o arquivo direto, fora da UI, faça a mesma conferência** — um `State: Ready` sozinho não prova que há serviço vivo. Há um único escritor por arquivo. Isso não substitui IPC autenticado para comandos privilegiados; o arquivo é apenas leitura de status, não entrada de autorização do motor.

## Parar e reverter

Pare o serviço com Ctrl+C. As políticas desta sessão são removidas; as regras salvas permanecem. Feche/reabra o aplicativo para testar a seleção automática em conexões novas.

Se ambas as opções estavam desativadas antes do teste (como nesta verificação), restaure em PowerShell administrativo:

```powershell
netsh interface ipv4 set global routepolicies=disabled store=active
netsh interface ipv6 set global routepolicies=disabled store=active
```

Não desative opções que já estavam habilitadas por outro produto. Não é necessário desligar nenhuma placa, reinstalar driver ou apagar o JSON.

## Evidência desta implementação

- Build Release: zero erros/avisos.
- 102 testes automatizados: ABI de interoperabilidade, ponteiro de LUID, transações simuladas, rollback, idempotência, falhas, auxiliares do Steam, heartbeat/estado obsoleto, processo do serviço morto, encerramento ordenado e seleção de família por placa, além da suíte anterior. Todos usam sessão WFP simulada: provam registro e transação, **nunca tráfego real**.
- UI renderizada em memória com dados sintéticos e ligações verificadas.
- Prova sem política: TCP HTTPS e UDP DNS responderam, ambos usando o IPv4 da Ethernet.
- `--verify-routing` recusou execução por falta dos pré-requisitos. **Não houve prova de seleção Wi-Fi nem aplicação WFP elevada nesta sessão.** A aceitação do roteamento real continua pendente até esse ensaio.
