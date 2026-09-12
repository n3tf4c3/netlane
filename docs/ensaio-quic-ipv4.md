# Ensaio isolado de QUIC/IPv4

Estado em **2026-09-11**: **ensaio QUIC/IPv4 elevado concluído**, somente no executável de prova, com saída Ethernet → Wi-Fi e retorno ao controle Ethernet. O usuário respondeu “vamos continuar” à solicitação específica; o UAC foi manual. Políticas da sessão removidas, flags restaurados e conferência independente aprovada. Nenhuma regra real, binding IPv6, aplicativo OneDrive/Steam ou serviço comum foi alterado/iniciado. O baseline anterior fica preservado abaixo como evidência histórica.

## O que foi implementado

> Evolução posterior neste mesmo dia: a [concorrência entre dois executáveis QUIC](ensaio-quic-concorrente.md) foi concluída com rotas distintas/invertidas e 61 verificações independentes. Os resultados sequenciais abaixo permanecem históricos; não representam tráfego de aplicativos reais ou continuidade com a bandeja.

`poc/NetLane.QuicProbe` é um executável independente, sem referências ao serviço, UI ou motor WFP. Sem argumentos, mostra ajuda. `--check-support` consulta `QuicConnection.IsSupported` sem resolver DNS ou conectar. `--handshake` abre exatamente uma conexão QUIC com destino IPv4 na porta 443 e ALPN `h3`.

A API QUIC é preview no .NET 8; por isso `EnablePreviewFeatures` está somente no probe e em seus testes, não nos projetos de produção. No Windows, o runtime inclui MsQuic; a verificação local foi positiva em **.NET 8.0.30 / Windows build 26200**. Referência: [suporte QUIC no .NET](https://learn.microsoft.com/en-us/dotnet/fundamentals/networking/quic/quic-overview).

O IP de destino pode ser fixado com `--ipv4` para comparar o mesmo endpoint em fases futuras; o nome `--host` continua sendo validado no certificado TLS. Sem `--ipv4`, seleciona o primeiro IPv4 unicast do DNS, sem tentativas ocultas em outros endereços. Não configura `LocalEndPoint`: o Windows escolhe a origem. Não altera confiança/certificados, usa proxy, abre socket TCP ou faz fallback. Referência: [opções de conexão QUIC](https://learn.microsoft.com/en-us/dotnet/fundamentals/networking/quic/quic-options#localendpoint).

O recibo JSON distingue suporte, tentativa de rede, handshake, interface observada e descarte. Associação ausente/ambígua ou interface não conectada não vira sucesso. Falha de transporte/descarte, cancelamento e prazo esgotado não viram prova de roteamento. Timeout cobre DNS/handshake; o coletor também limita o processo a `TimeoutSeconds + 15` segundos e pode encerrar **somente seu filho sem políticas**. Esse mecanismo não é um controlador de limpeza para serviço elevado.

## Executar sem roteamento

Estes projetos são opt-in, fora de `NetLane.sln`; não acrescentam tráfego externo a `dotnet test` da solução. A suíte abaixo usa dados sintéticos, não Internet, WFP ou alteração de conectividade:

```powershell
dotnet build poc/NetLane.QuicProbe/NetLane.QuicProbe.csproj -c Release --artifacts-path artifacts/quic-20260911
dotnet test tests/NetLane.QuicProbe.Tests/NetLane.QuicProbe.Tests.csproj -c Release --artifacts-path artifacts/quic-20260911 --logger "trx;LogFileName=quic-probe.trx" --results-directory artifacts/quic-20260911/test-results
```

Suporte, sem DNS/tráfego:

```powershell
& .\scripts\test-quic-baseline.ps1 -ProbePath .\artifacts\quic-20260911\bin\NetLane.QuicProbe\release\NetLane.QuicProbe.exe
```

Uma conexão externa sintética, sem políticas:

```powershell
& .\scripts\test-quic-baseline.ps1 -ProbePath .\artifacts\quic-20260911\bin\NetLane.QuicProbe\release\NetLane.QuicProbe.exe -Handshake -ServerName www.cloudflare.com
```

O coletor cria um diretório único em `artifacts/quic-baseline/`, guarda stdout/resultado e compara regras, bindings, endereços, rotas padrão, DNS, métricas, flags e processos do serviço antes/depois. Recusa serviço real em execução, flags diferentes de `disabled` e estado não verificável; não tenta corrigi-los. Exige as placas Ethernet/Wi-Fi identificáveis neste computador. Estado inalterado é uma comparação dessas observações, não monitoramento contínuo de toda configuração do Windows.

Códigos do probe: `0` suporte/handshake observado; `1` falha/cancelamento; `2` suporte indisponível; `4` interface não confirmada; `64` argumentos inválidos. Suporte sozinho não prova conectividade. O coletor retorna falha se a conferência final não coincidir.

## Evidência real do baseline

Build isolado sem avisos/erros; **33 testes aprovados, 0 falhas e 0 ignorados**, incluindo prazo real de cancelamento, entrada inválida, suporte sem I/O, destino fixo, preservação da validação TLS, ausência de fallback e observação ambígua/indisponível. Recibo `artifacts/quic-20260911/test-results/quic-probe-final.trx`. A suíte histórica de 216 testes da UI/serviço não foi reexecutada neste trabalho isolado; nenhum código de produção foi alterado nesta retomada.

Execução **2026-09-11 11:23:05–11:23:12 (America/Cuiaba)**, sem solicitar elevação:

- Runtime suportado; `NetworkAttempted=false` na consulta de suporte.
- Probe PID `25916`, caminho `artifacts/quic-20260911/bin/NetLane.QuicProbe/release/NetLane.QuicProbe.exe`.
- `www.cloudflare.com` → **104.16.124.96:443**, ALPN **h3**.
- Origem **192.168.15.5:62383**, associada unicamente à **Ethernet**, GUID `{D3AE43D2-8203-41D6-9D7F-0B6FA59518FD}`, índice observado `22`.
- `HandshakeObserved`, `ConnectionDisposed=true`; duração do probe **1.173 ms**. Não houve requisição HTTP nem conteúdo do usuário.
- SHA-256 da DLL: `E32261F8FF5C9CCFFAD01D1DCB2D3AE0581A0A8692AE41034EB824BCC1C186DD`.
- `SettingsUnchanged=true`: Ethernet `192.168.15.5`, Wi-Fi `192.168.0.102`, bindings IPv6 desabilitados, `routepolicies` IPv4/IPv6 desativados e nenhum processo do serviço nos dois snapshots.
- SHA-256 das regras reais: `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`, preservado.

Recibos em `artifacts/quic-baseline/20260911T152305576Z-30a27fdf/`: `baseline.stdout.json`, `support.stdout.json`, `before.json`, `after.json`, `summary.json`. PID, porta, GUID/índice, IP e DNS acima são evidência desta execução; revalidar antes de qualquer nova política.

O coletor sem `-Handshake` também foi exercitado às **11:26:35–11:26:42**, com `NetworkAttempted=false`, `Supported` e configurações preservadas. Recibos em `artifacts/quic-baseline/20260911T152635688Z-8ef6b5b4/`. A DLL recompilada mantém o mesmo hash da conexão externa. Após o teste, nenhum processo do probe, UI, host fictício ou serviço permaneceu em execução.

## Controlador elevado e segurança

`poc/NetLane.QuicRoutingCheck` reutiliza o motor `WfpRoutingEngine` e a restauração `TemporaryRoutePolicies`, sem carregar o arquivo real de regras: o arquivo é somente lido para hash. O alvo WFP é exclusivamente o caminho completo de `NetLane.QuicProbe.exe` da compilação isolada. UI e `NetLane.Service.exe` não participam deste ensaio.

O lançador `scripts/test-quic-routing.ps1` cria um diretório novo, registra a referência de rede e hashes do probe/coletor e exige preflight aprovado. No uso padrão sem `-RunAuthorized`, faz apenas preflight: não solicita UAC, abre conexão externa nem ativa opções. A opção adicional explícita `-Concurrent -BaselinePair`, introduzida depois, faz somente o controle concorrente com tráfego sintético, sem políticas. Com autorização específica e `-RunAuthorized`, pede UAC manual e inicia somente o controlador oculto. Não responde ao UAC e não tenta contorná-lo.

O controlador exige pedido recente, hash correspondente, binários inalterados, processo único, APIs/privilégios disponíveis, flags inicialmente desativados, interfaces/GUIDs/IPv4 inequívocos, IPv6 desabilitado e serviço real ausente. Cada política é seguida por um processo novo; após o primeiro DNS, fixa o mesmo destino IPv4 mantendo o nome TLS. Referência de rede/regras é revalidada entre fases.

`routepolicies` de IPv4 e IPv6 é ativado temporariamente em `store=active`, pois o motor exige os dois flags. Isso **não habilita IPv6 nas placas**; o motor registrou políticas somente IPv4 neste ensaio. A limpeza remove as políticas da própria sessão, fecha a sessão WFP dinâmica e restaura apenas os flags alterados, inclusive em falha/cancelamento. Erro de remoção, restauração, recibo ou conferência impede aprovação. Não há remoção global de políticas nem alteração persistente por `netsh`.

O controlador tem prazo de três minutos e aceita `stop.request` em seu diretório para cancelamento cooperativo. Cada filho QUIC tem limite de 30 segundos; somente esse filho pode ser terminado pelo controlador, que permanece vivo para limpar a sessão. **Não finalizar à força o controlador**: queda abrupta pode deixar flags globais ativos; falta de `result.json`/limpeza confirmada exige conferir estado antes de outro ensaio. Não há repetição automática elevada.

Preparação e testes, sem roteamento:

```powershell
dotnet build poc/NetLane.QuicRoutingCheck/NetLane.QuicRoutingCheck.csproj -c Release --artifacts-path artifacts/quic-routing-20260911
dotnet test tests/NetLane.QuicProbe.Tests/NetLane.QuicProbe.Tests.csproj -c Release --artifacts-path artifacts/quic-routing-20260911
& .\scripts\test-quic-routing.ps1 -ControllerPath .\artifacts\quic-routing-20260911\bin\NetLane.QuicRoutingCheck\release\NetLane.QuicRoutingCheck.exe -ProbePath .\artifacts\quic-routing-20260911\bin\NetLane.QuicProbe\release\NetLane.QuicProbe.exe
```

Somente após combinar o escopo, executar o último comando com `-RunAuthorized`. O diretório devolvido identifica a execução; aguardar `result.json`, fim do processo e conferir:

```powershell
& .\scripts\verify-quic-routing.ps1 -RunDirectory <diretorio-de-recibos-da-execucao>
```

Os **54 testes do probe/controlador** passaram (33 anteriores + 21 novos), sem falhas/ignorados, em `artifacts/quic-routing-20260911/test-results/quic-routing.trx`. Os **38 testes focados** do motor, pré-requisitos e flags temporários também passaram, em `routing-safety.trx` no mesmo diretório. Foram exercitados recusa sem autorização, falhas em cada fase, ativação parcial, cancelamento, ordem da limpeza, erros de remoção/descarte/restauração, recibo ausente, PID repetido e saída incorreta. São testes sintéticos, não provas de tráfego ou de UAC. A primeira passagem do preflight real recusou GUID com chaves; o lançador foi corrigido para o formato canônico e revalidado **antes de qualquer elevação**.

## Resultado elevado real e conferência final

Controlador PID **13408**, execução **2026-09-11 11:41:27–11:42:15 (America/Cuiaba)**. Destino único **www.cloudflare.com / 104.16.124.96:443**, certificado/nome validados pelo probe, ALPN **h3** nas quatro conexões:

| Fase | PID novo | Origem observada | Tempo do probe |
| --- | --- | --- | --- |
| Controle sem política | 24544 | Ethernet · 192.168.15.5:60911 | 547 ms |
| Política Ethernet | 1376 | Ethernet · 192.168.15.5:65491 | 516 ms |
| Política Wi-Fi | 22832 | Wi-Fi · 192.168.0.102:61714 | 695 ms |
| Controle após limpeza | 21036 | Ethernet · 192.168.15.5:55396 | 531 ms |

Os tempos acima incluem o trabalho do probe, não são um benchmark de latência dos links. Houve política aceita **e** handshake concluído com o IP/GUID esperado em cada fase, sem bind de origem, proxy ou fallback TCP. O retorno à Ethernet foi observado em outra conexão, após remoção das políticas e restauração dos flags.

`result.json`: `Passed=true`, nenhuma falha, `PoliciesRemoved=true`, `SessionDisposed=true`, `FlagsRestored=true`, `SettingsUnchanged=true` e `CleanupConfirmed=true`. O verificador independente passou **33/33 verificações às 11:43:58**, incluindo releitura da rede, flags e processos:

- `routepolicies` IPv4/IPv6 novamente **disabled**; binding IPv6 continua desabilitado nas duas placas.
- GUIDs, endereços, gateways, DNS, métricas e regras reais coincidem com a referência pré-UAC.
- Nenhum `NetLane.QuicRoutingCheck`, `NetLane.QuicProbe` ou `NetLane.Service` em execução.
- Hash das regras reais preservado: `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.
- DLL do controlador: `D87C5966354966AB6C28215503F344EF814474FF13C85DCBDC1B74E8BE103A40`; DLL do probe: `E32261F8FF5C9CCFFAD01D1DCB2D3AE0581A0A8692AE41034EB824BCC1C186DD`.

Evidências em `artifacts/quic-routing/20260911T154116966Z-09f1169f/`: `request.json`, `preflight.json`, `launcher.json`, `controller-start.json`, `before.json`, recibos `policy-*`, `raw-*`/`verified-*`, `after-cleanup.json`, `result.json` e `verification-20260911T154358083Z.json`. Os PIDs/portas/índices são históricos, não alvos para reutilizar. A remoção é confirmada pelos retornos das APIs da própria sessão e pelo controle posterior; não foi feita enumeração global de todas as políticas WFP do Windows.

## Limites e próxima fase

Este é **um handshake QUIC/IPv4 autenticado**, não uma implementação de HTTP/3 completo: não abre stream de requisição, não envia GET, não mede download, vazão ou sincronização. A Cloudflare documenta [HTTP/3 sobre QUIC](https://developers.cloudflare.com/speed/optimization/protocol/http3/); a disponibilidade do destino foi verificada por este handshake, não presumida a partir dessa documentação.

O baseline isolado anterior, por si só, não comprova causalidade de roteamento. A sequência elevada acrescentou a mudança de saída para a Wi-Fi pela política do motor e o retorno ao controle após remoção. Essa prova permanece limitada ao handshake QUIC/IPv4 desse executável, nesse destino e nas condições registradas. Não comprova QUIC do OneDrive/Steam, IPv6, continuidade com a bandeja, concorrência, mudança de rede ou recuperação.

A [concorrência isolada](ensaio-quic-concorrente.md) foi concluída em uma execução posterior, após novo pedido do usuário. O próximo cenário é continuidade com o painel minimizado, com configuração isolada e escopo combinado; ainda não foi executado. **Não habilitar binding IPv6**, iniciar o painel com regras reais, reiniciar aplicativos ou desconectar placas. O coletor de baseline permanece somente leitura quanto a políticas.

Instalador local, assinatura, commit e push continuam etapas separadas. Ver também [ampliação dos ensaios de rede](ampliacao-ensaios-rede.md).
