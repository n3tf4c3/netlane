# NetLane

Cada app, sua rota. Projeto experimental Windows para configurar a conexão de rede por executável.

## Estado atual

- Monitor de adaptadores, processos conhecidos e conexões TCP/UDP IPv4.
- Serviço com políticas em JSON e roteamento por AppId/LUID usando a API nativa `FwpmConnectionPolicyAdd0` (IPv4/IPv6), sem driver próprio.
- Fase 3 em andamento: [interface WPF renovada](docs/interface-desktop.md), com navegação lateral, cartões de conexão, editor de regras e diagnóstico do serviço. Inclui pesquisa, habilitação e persistência com backup.
- Painel de consumo por interface: download/upload em Mbps, gráfico de 60 segundos e volume recebido/enviado/total durante a medição.
- Motor de seleção de saída implementado; [teste real do OneDrive](docs/teste-onedrive-wifi.md#segundo-ensaio-de-2026-09-07-novas-conexões-na-wi-fi) confirmou 17 novas conexões TCP/IPv4 pela Wi-Fi. UDP/QUIC, IPv6 e sincronização prolongada continuam pendentes. Consulte [ativação, prova isolada e reversão](docs/roteamento-nativo.md). Requer API disponível, Administrador e `routepolicies` habilitado no Windows.

O status “conectada” da interface e o status “salvo” da regra não confirmam roteamento. A UI agora recebe o resultado do serviço por heartbeat em arquivo, com revisão e expiração. “Política aceita” confirma o registro pelo Windows, não tráfego medido. Steam inclui seu auxiliar conhecido quando a opção estiver marcada; jogos precisam de regras próprias.

## Executar no Windows

Requer .NET SDK 8. A interface pode ser aberta sem elevação:

```powershell
dotnet run --project "C:\Codes\netlane\src\NetLane.UI"
```

O serviço deve rodar em outro PowerShell, como administrador, para usar WFP. Antes do primeiro teste, consulte os [pré-requisitos do Windows](docs/roteamento-nativo.md#pré-requisito-global-do-windows); a aplicação não habilita opções globais automaticamente:

```powershell
dotnet run --project "C:\Codes\netlane\src\NetLane.Service"
```

Reinicie o serviço após atualizar o código. Alterações de regras salvas pela UI são lidas no próximo ciclo do serviço, de 10 segundos por padrão.

O alvo local de teste agora é **OneDrive → Wi-Fi**, com a regra do Steam desabilitada. Para [validar a sincronização](docs/teste-onedrive-wifi.md), o controlador oferece `-UntilStopped`: mantém o serviço ativo até solicitar `-Stop`, que restaura as opções globais habilitadas. O ensaio com prazo continua disponível para provas curtas.

No checkout, a UI usa `src/NetLane.Service/netlane-rules.json`, inclusive quando aberta de outra pasta. O caminho aparece em **Diagnóstico → Configuração local**, onde também é possível copiar um resumo do estado do serviço. Fora do checkout, procura o arquivo junto ao executável da UI; instalação e caminho compartilhado de produção ainda estão pendentes.

Em **Visão geral**, abra **Gerenciar interfaces** e marque as placas desejadas, como Ethernet e Wi-Fi. A preferência é salva automaticamente em `%LOCALAPPDATA%\NetLane\interface-selection.json`; o botão de salvar permite repetir uma gravação que tenha falhado. A seleção controla o painel e as opções oferecidas nas regras do NetLane, sem ligar/desligar placas no Windows e sem remover regras existentes. A seleção inicial prioriza interfaces conectadas com gateway; adaptadores virtuais podem ser selecionados manualmente.

Clique no cartão de uma conexão para ver sua medição. As leituras são atualizadas a cada 2 segundos. **Reiniciar medição** zera somente os totais e o gráfico daquela interface. Os volumes começam quando a UI abre e não são um histórico diário/mensal; incluem tráfego local e Internet de todos os aplicativos. Intervalos com falha de leitura são desconsiderados. As preferências permanecem ao reabrir a UI, mas os totais da medição não são persistidos.

- “Adicionar aplicativo” seleciona um arquivo `.exe`; a regra começa em Automático.
- Escolher uma placa grava o modo e o GUID correspondente. Uma escolha explícita indisponível não é trocada silenciosamente por outra placa.
- “Automático”, desabilitar ou remover a regra libera o aplicativo no próximo ciclo do serviço.
- “Salvar alterações” grava todas as regras, inclusive as ocultas pela pesquisa.
- A versão anterior fica em `netlane-rules.json.bak`. JSON inválido e conflito com alteração externa são informados sem substituir o arquivo por exemplos.

## Verificar

```powershell
dotnet build "C:\Codes\netlane\NetLane.sln" --configuration Release
dotnet test "C:\Codes\netlane\NetLane.sln" --configuration Release --no-build
```

Os testes automatizados usam arquivos temporários e sessões WFP simuladas, incluindo rollback/ABI/heartbeat. `WindowsCounters` lê os contadores reais. Nenhum teste de `dotnet test` instala políticas ou altera a conectividade. A prova opt-in `--verify-routing` aplica políticas temporárias somente ao próprio executável da PoC e exige elevação; não se confunde com a antiga consulta `--check-public-ip`, que usa bind manual no curl.

## Planejamento

- [Visão do produto](docs/Planejamento%20do%20Projeto%20NetLane.md)
- [Fase 0 — evidências e limites da PoC](docs/plano-de-execucao-fase0.md)
- [Fase 1 — monitor](docs/plano-de-execucao-fase1.md)
- [Fase 2 — políticas no serviço](docs/plano-de-execucao-fase2.md)
- [Fase 3 — interface e próximas entregas](docs/plano-de-execucao-fase3.md)
- [Evolução do motor WFP](docs/wfp-engine-roadmap.md)
- [Roteamento nativo — ativar, verificar e reverter](docs/roteamento-nativo.md)
