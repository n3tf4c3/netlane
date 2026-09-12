# Ampliação dos ensaios de rede

> Concluído em **2026-09-12 às 07:56:33**: [continuidade com painel na bandeja](ensaio-bandeja-roteamento.md#resultado-completo-da-rodada-de-30-minutos), seis fases e verificação após fechamento aprovadas. Novas conexões QUIC/IPv4 do probe pela Wi-Fi com janela visível, oculta e restaurada, na mesma sessão; retorno à Ethernet após parada. Rede/regras preservadas, flags restaurados, IPv6 desligado e nenhum processo restante. Primeira rodada parcial e preparação recusada preservadas. Sem repetição automática; próxima etapa na ordem combinada é a preparação do instalador local, ainda não iniciada.

Preparação iniciada em 2026-09-10, após a revisão básica da [bandeja do Windows](bandeja-windows.md). Este documento separa preparação, autorização e comprovação de tráfego. O primeiro ensaio QUIC/IPv4 elevado foi concluído em 2026-09-11, com autorização específica, UAC manual e limpeza conferida.

**QUIC/IPv4 isolado concluído em 2026-09-11.** A retomada genérica inicialmente cobriu apenas preparação/baseline. Depois, o usuário respondeu “vamos continuar” à pergunta específica sobre Ethernet → Wi-Fi, UAC manual e roteamento temporário só do probe. Esse ensaio terminou com retorno à Ethernet e configurações restauradas; não há autorização automática para novos cenários. A pausa anterior está preservada no [ponto de retomada](continuidade-2026-09-10.md).

## Referência histórica, somente leitura

Consulta em **2026-09-10 às 11:28:30** (America/Cuiaba); a referência atualizada está na seção seguinte:

- Ethernet e Wi-Fi conectadas, índices atuais 22 e 19, respectivamente. Revalidar identidade, GUID, endereços, rotas e métricas antes de qualquer execução; não reutilizar índices históricos como autoridade.
- Binding `ms_tcpip` habilitado nas duas placas; `ms_tcpip6` desabilitado nas duas.
- Nenhum endereço IPv6 nem rota padrão IPv6 nessas duas interfaces. A ausência de conectividade IPv6 neste estado não é uma falha demonstrada do NetLane.
- `netsh interface ipv4 show global` e a consulta equivalente IPv6 mostraram **Políticas de Rota: disabled**.
- Nenhum `NetLane.Service` em execução. O painel da bandeja permanece aberto; o host fictício da validação anterior já foi encerrado.
- Regras reais inalteradas: SHA-256 `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.

Nenhum binding, adaptador, gateway, DNS, métrica, regra de firewall, arquivo sincronizado ou regra real do NetLane foi alterado. Não foi iniciada captura de pacotes.

## Preparação verificada em 2026-09-11

- Novo `poc/NetLane.QuicProbe`, sem dependência da UI, serviço ou motor de roteamento. `--check-support` não faz DNS/tráfego; `--handshake` exige destino explícito, abre uma conexão QUIC/IPv4 nova, valida certificado/nome normalmente e registra ALPN, endpoints e interface associada ao IP local. Não usa bind de origem, proxy ou fallback TCP.
- Build isolado sem avisos/erros e **33 testes aprovados**, sem falhas/ignorados; suíte própria em `tests/NetLane.QuicProbe.Tests`, fora da solução de produção. A opção de APIs preview do .NET 8 fica restrita a esses dois projetos.
- Controle sem políticas realizado às **11:23:11–11:23:12**: `www.cloudflare.com`, destino `104.16.124.96:443`, ALPN **h3**, origem **Ethernet / 192.168.15.5:62383**. Handshake e descarte da conexão concluídos; tempo total do probe **1.173 ms**. Isso comprova uma troca QUIC de estabelecimento, não uma resposta HTTP/3 ou download.
- O coletor `scripts/test-quic-baseline.ps1` conferiu antes/depois: Ethernet/Wi-Fi conectadas, mesmos GUIDs/IPs/gateways/DNS/métricas, IPv6 ainda desabilitado, `routepolicies` IPv4/IPv6 ainda desativado, nenhum serviço real e hash das regras preservado. Resultado `SettingsUnchanged=true` no intervalo **11:23:05–11:23:12**.
- Evidência bruta: `artifacts/quic-baseline/20260911T152305576Z-30a27fdf/`, com `support`, `baseline`, `before`, `after` e `summary`. Comandos, limites e próxima fase em [ensaio QUIC/IPv4](ensaio-quic-ipv4.md).

## Cenário QUIC/IPv4 executado

**QUIC sobre IPv4 em um executável de prova isolado**, antes de ampliar a observação para aplicativos reais. Uma resposta DNS/STUN sobre UDP não comprova QUIC; os cenários UDP genéricos já existentes em `RouteVerification.cs` são referência, não substitutos desse teste. Nenhum resultado do executável de prova será atribuído ao OneDrive ou a seus auxiliares.

Critérios adotados para a execução:

1. Conferir suporte QUIC/MsQuic do runtime e selecionar um destino que responda ao protocolo, com tráfego sintético mínimo e sem dados do usuário.
2. Medir o controle sem política e confirmar a conectividade do destino. Timeout isolado não identifica defeito do motor.
3. Usar exclusivamente o AppId/caminho completo do executável de prova e conexões novas a cada mudança. Não usar bind manual de origem, `curl --interface`, proxy ou `IP_UNICAST_IF` como evidência do roteamento pelo NetLane.
4. Registrar separadamente política aceita, resposta recebida, interface/IP local observado e limitações da medição. A aceitação da política não basta para aprovação.
5. Encerrar normalmente, remover as políticas da sessão, restaurar somente opções temporárias alteradas pelo ensaio e comparar configuração/regras com a referência. Falta de recibo ou divergência impede declarar limpeza confirmada.

O controlador `NetLane.QuicRoutingCheck` foi implementado e executado às **11:41:27–11:42:15**, com quatro processos novos no mesmo destino: controle Ethernet → política Ethernet → política Wi-Fi → controle Ethernet após limpeza. As quatro conexões QUIC negociaram `h3`; a fase Wi-Fi observou **192.168.0.102**, e os controles/Ethernet observaram **192.168.15.5**. Não se usou bind manual, proxy ou fallback TCP.

**54 testes do probe/controlador**, **38 regressões focadas** do motor/flags e **33 verificações independentes** finais passaram. Políticas da própria sessão removidas, flags IPv4/IPv6 restaurados para `disabled`, IPv6 das placas ainda desligado, regras/rede preservadas e nenhum processo do ensaio ou serviço real restante. Evidência e limites em [ensaio QUIC/IPv4](ensaio-quic-ipv4.md#resultado-elevado-real-e-conferência-final). O arquivo real de regras foi somente lido para hash; o painel e o serviço comum não foram iniciados.

## Limites e etapas seguintes

A [concorrência QUIC/IPv4](ensaio-quic-concorrente.md) também foi concluída em **2026-09-11 às 12:25:51**, após “vamos seguir” para esse próximo cenário: A → Ethernet/B → Wi-Fi, depois A → Wi-Fi/B → Ethernet, com cerca de três segundos de sobreposição por par. Oito processos novos e dois AppIds distintos; ambos voltaram à Ethernet após limpeza. **75 testes** e **61 verificações independentes** passaram, com rede/regras preservadas e nenhum processo do ensaio restante. A continuidade com o painel minimizado foi concluída posteriormente, no ensaio separado registrado acima; nenhum desses cenários deve ser repetido automaticamente.

- IPv6 permanece desativado. Habilitá-lo pode mudar a conectividade dos aplicativos e exige uma decisão separada do usuário, além de conferir se os links oferecem endereço e rota IPv6 utilizáveis. Não será habilitado automaticamente pelo ensaio.
- A concorrência foi comprovada somente para conexões QUIC/IPv4 observadas dos dois probes. A continuidade com o painel minimizado foi comprovada separadamente, com janela/serviço reais e regra própria do probe; nenhum dos ensaios comprova transferência sustentada ou QUIC de aplicativos reais.
- Não desconectar Wi-Fi/Ethernet, suspender/reiniciar o Windows, reiniciar OneDrive, executar Steam/auxiliares ou testar VPN sem combinar especificamente esse escopo.
- Instalador local, assinatura, distribuição, commit e push continuam etapas separadas; nada foi publicado nesta preparação.
