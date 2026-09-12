# Concorrência QUIC/IPv4 com dois executáveis isolados

**Concluído em 2026-09-11, às 12:25:51 (America/Cuiaba).** O usuário pediu “vamos seguir” após a proposta específica de dois executáveis simultâneos, cada um em uma conexão. O início elevado foi solicitado via UAC manual. Houve conexões sobrepostas, rotas distintas, inversão das atribuições e retorno ao controle sem políticas. A verificação independente passou **61/61** às **12:26:43**.

O ensaio não iniciou a UI nem `NetLane.Service`, não alterou as regras reais e não habilitou IPv6 nas placas. OneDrive, Steam, auxiliares e arquivos sincronizados não foram alvos. Não houve commit, push, instalador ou agendamento.

## Escopo e método

- Duas cópias de `NetLane.QuicProbe.exe`, com conteúdo idêntico e **caminhos completos distintos**, em `probe-a/` e `probe-b/` dentro do diretório exclusivo do ensaio. O AppId de roteamento é o caminho completo, não apenas o nome do arquivo. Não foi aplicada política ao `dotnet.exe`, ao controlador ou a aplicativos reais.
- O mesmo `WfpRoutingEngine` mantém duas identidades de aplicação separadas (papéis A/B). Primeiro A → Ethernet e B → Wi-Fi; depois A → Wi-Fi e B → Ethernet, sem filhos antigos em execução durante a mudança.
- Antes de conectar, os dois filhos informam PID/caminho e aguardam `GO` em stdin. O controlador só libera a barreira quando **ambos** estiverem prontos. Isso evita confundir duas execuções sequenciais com concorrência.
- Cada filho negocia QUIC/IPv4 com ALPN `h3`, usa a validação normal do certificado/nome e observa a conexão por aproximadamente três segundos. Um monitor espera streams de controle ou fechamento remoto; erro/fechamento antes do fim da observação impede aprovação. O probe confere que endpoints e ALPN não mudaram e fecha/descarta os recursos antes de emitir sucesso. Referência da operação de espera: [AcceptInboundStreamAsync](https://learn.microsoft.com/en-us/dotnet/api/system.net.quic.quicconnection.acceptinboundstreamasync).
- A sobreposição é a interseção dos intervalos após o handshake e antes do fechamento controlado. Usa `Stopwatch`/QPC do mesmo Windows, não o horário de criação do processo nem a simples existência de objetos. Exige pelo menos **1.000 ms** de interseção e pelo menos 2.900 ms em cada observação. QPC é comparável entre processos no mesmo computador; a margem é muito maior que a ambiguidade de um tick. Referência: [temporização de alta resolução no Windows](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps).
- O DNS é resolvido uma vez antes das políticas; todas as conexões usam o mesmo IPv4 remoto e preservam o nome TLS. Não há bind de origem, proxy ou fallback TCP.

O controlador mantém as salvaguardas do [ensaio sequencial](ensaio-quic-ipv4.md#controlador-elevado-e-segurança): pedido recente e identificado por hash, preflight, binários conferidos, exclusão de outro controlador/serviço real, rede/regras revalidadas, UAC manual, sessão WFP dinâmica e restauração em `finally`. Os dois flags `routepolicies` são temporariamente ativados em `store=active`; os bindings IPv6 permanecem desabilitados. O controlador recolhe **ambos os filhos** antes de remover políticas e restaurar flags, mesmo se uma inicialização/barreira/conexão falhar.

## Validação antes da elevação

Build isolado e **75 testes aprovados**, 0 falhas/ignorados: os 54 existentes continuaram passando e foram acrescentados 21 casos de concorrência. Incluem barreira por PID/caminho, requisitos do filho, duração/sobreposição insuficientes, relógios incompatíveis, processo/AppId duplicados, fechamento remoto indicado, troca de rotas, falha de cada par, recusa da segunda política, cancelamento, erro de remoção e recibo ausente. Esses testes são sintéticos e não ativam políticas.

Recibo: `artifacts/quic-concurrent-20260911/test-results/concurrent-full.trx`. O código de produção do motor/UI/serviço não foi alterado neste ciclo; a suíte completa de 216 testes da UI/serviço e os 38 testes focados anteriores não foram reexecutados nesta etapa.

Também foi exercitado um **controle concorrente real sem políticas**, antes do UAC, em `artifacts/quic-routing/20260911T161808183Z-500f123f/`: dois processos distintos, ambos pela Ethernet, com **3.016,352 ms** de sobreposição e configurações preservadas. Isso validou o protocolo de barreira/observação; não foi usado como prova de roteamento por política.

## Execução elevada e resultado

Controlador PID **31148**, de **12:24:24 a 12:25:51**. Destino comum **www.cloudflare.com / 104.16.124.96:443**, ALPN **h3** nas oito conexões. Todos os filhos encerraram e descartaram suas conexões antes da fase seguinte.

| Par | A: PID e saída | B: PID e saída | Sobreposição observada |
| --- | --- | --- | --- |
| Controle inicial | 21056 · Ethernet · 192.168.15.5 | 39420 · Ethernet · 192.168.15.5 | 3.013,8212 ms |
| Rotas distintas | 28604 · Ethernet · 192.168.15.5 | 17572 · Wi-Fi · 192.168.0.102 | 3.004,6516 ms |
| Rotas invertidas | 24916 · Wi-Fi · 192.168.0.102 | 24212 · Ethernet · 192.168.15.5 | 2.976,5200 ms |
| Controle após limpeza | 22420 · Ethernet · 192.168.15.5 | 10112 · Ethernet · 192.168.15.5 | 3.009,1674 ms |

As políticas A/B foram aceitas separadamente e cada conexão confirmou o IP/GUID esperado. A inversão usou os **mesmos dois caminhos**, com novos PIDs; portanto a segunda regra não substituiu indiscriminadamente a primeira. Os dois controles finais voltaram à saída original depois da remoção/restauração.

Esses intervalos não são latência ou velocidade dos links. São duração de sobreposição de conexões estabelecidas e observadas, sem indicação de fechamento remoto durante o intervalo, não duração de transmissão contínua de conteúdo.

`result.json`: `Passed=true`, erros vazios, `PoliciesRemoved=true`, `SessionDisposed=true`, `FlagsRestored=true`, `SettingsUnchanged=true`, `CleanupConfirmed=true`.

## Conferência independente e evidências

O script `scripts/verify-quic-concurrent.ps1` releu os recibos brutos, recalculou os quatro intervalos e consultou o estado atual. **61/61 verificações aprovadas às 12:26:43**:

- Oito PIDs distintos, dois caminhos exatos, barreiras identificadas, oito handshakes `h3` concluídos e recursos descartados.
- Fontes/IPs coerentes com cada papel/regra e retorno dos dois executáveis às fontes dos controles iniciais.
- `routepolicies` IPv4/IPv6 novamente **disabled**, e bindings IPv6 ainda desabilitados nas duas placas.
- Rede e regras idênticas à referência pré-UAC: GUIDs, endereços, gateways, DNS, métricas e hash do arquivo real.
- Nenhum processo `NetLane.QuicRoutingCheck`, `NetLane.QuicProbe` ou `NetLane.Service` restante.
- As duas cópias do probe e a DLL do controlador mantiveram os hashes da execução.

Diretório: `artifacts/quic-routing/20260911T162413002Z-f8a139f9/`. Contém `request.json`, `preflight.json`, `launcher.json`, `controller-start.json`, `before.json`, recibos de políticas `policy-*-A/B`, saídas `raw-*-A/B`, pares `verified-*`, `after-cleanup.json`, `result.json` e `concurrent-verification-20260911T162643149Z.json`, além das cópias A/B. PIDs, IPs e GUIDs registrados são evidência histórica; não reutilizar como alvos sem revalidação.

Hashes SHA-256:

- Regras reais: `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.
- DLL dos probes A/B: `A46C77C48A0CE286015528908ABBB297C64C087CA7791C0271EDD187B2DD5C5E`.
- DLL do controlador: `B06C01D69F6375D9006D58F1DBEF322D45C485D04F384A4DD12D356D9E5087F3`.

## Comandos e limites de autorização

Preparação/build/testes, sem políticas:

```powershell
dotnet test tests/NetLane.QuicProbe.Tests/NetLane.QuicProbe.Tests.csproj -c Release --artifacts-path artifacts/quic-concurrent-20260911
& .\scripts\test-quic-routing.ps1 -ControllerPath .\artifacts\quic-concurrent-20260911\bin\NetLane.QuicRoutingCheck\release\NetLane.QuicRoutingCheck.exe -ProbePath .\artifacts\quic-concurrent-20260911\bin\NetLane.QuicProbe\release\NetLane.QuicProbe.exe -Concurrent
```

Adicionar `-BaselinePair` ao último comando faz duas conexões externas sintéticas **sem políticas**, exigindo ambos os probes prontos e medindo a sobreposição. Esse parâmetro é incompatível com `-RunAuthorized`.

Somente depois de combinar o ensaio, usar `-Concurrent -RunAuthorized` para solicitar UAC manual e executar as fases elevadas. Para conferir uma execução concluída:

```powershell
& .\scripts\verify-quic-concurrent.ps1 -RunDirectory <diretorio-de-recibos-da-execucao>
```

O ensaio encerrado não será reexecutado/agendado automaticamente. Não matar o controlador: em caso de interrupção, solicitar parada cooperativa com `stop.request`, aguardar limpeza e conferir flags/processos. Queda abrupta pode deixar flags globais ativos; falta de recibo exige diagnóstico, não uma nova tentativa elevada presumida.

## O que permanece pendente

Próximo cenário da ordem combinada: **continuidade com o painel minimizado na bandeja**, usando sessão e regras isoladas. Este teste não abriu nem minimizou o painel e não comprova esse comportamento com roteamento ativo. Requer preparação própria e combinação do escopo antes da fase elevada.

Também não comprova download HTTP/3, vazão sob carga, transferência contínua de dados, QUIC de OneDrive/Steam, IPv6, VPN ou recuperação por desconexão/suspensão. A remoção refere-se às políticas da própria sessão e aos retornos das APIs; não foi feita enumeração global de WFP. Instalador, assinatura, publicação e alterações das regras reais continuam fora desta execução.
