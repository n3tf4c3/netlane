# Plano de execucao - Fase 0 (PoC obrigatoria)

Revisão em 2026-09-06: instalação de filtros confirmada pelos logs anteriores; viabilidade de redirecionamento por aplicativo ainda pendente. A leitura do código mostrou que `--check-public-ip` usa `curl --interface`, e não o tráfego dos executáveis mapeados. Ver os limites no [plano da Fase 3](plano-de-execucao-fase3.md).

## Objetivo

Comprovar que e possivel associar regras de saida por executavel para interfaces diferentes, sem alterar a rota global do Windows.

## Entregaveis da Fase 0

- Lista de interfaces identificada por AdapterGuid (nao por nome apenas).
- Comando PoC com pelo menos dois apps mapeados para interfaces distintas.
- Validacao pratica de IP publico por aplicacao:
  - app A usando Internet A
  - app B usando Internet B
- Relatorio de risco/limitacao tecnica antes de iniciar Fase 1.

## Lista de tarefas

1. Provisionar infraestrutura basica do repositorio
   - Criar solucao/pacotes base conforme arquitetura em 6 diretorios.
   - Registro inicial concluido.

2. Definir abordagem de rede para PoC
   - Estudar opcoes: WFP, WinDivert, ou abordagem alternativa.
   - Definir API com melhor controle por processo e menor impacto.
   - Definir requisitos de privilegio (admin/service).

3. Implementar PoC minima (CLI)
   - Mapear adapters conectados e exibir status.
   - Descobrir aplicacoes candidatas por executavel.
   - Coletar conexoes ativas (TCP/UDP IPv4) e correlacionar por PID com o executavel.
   - Implementar regra de saida por executavel para:
     - `chrome.exe -> Wi-Fi`
     - `curl.exe -> Ethernet`
   - Modos ativos: `dry-run` (padrao), `--firewall` (experimental), `--wfp` (preflight).
   - Mecanismo de fallback: se WFP falhar, volta automaticamente para `dry-run`.
   - Proximo: substituir stubs por filtro WFP real e validar persistencia enquanto o app estiver ativo.
   - `--check-public-ip` para coletar IP publico via curl durante a verificacao.

4. Validar protocolos exigidos
   - HTTP/HTTPS (TCP)
   - QUIC/HTTP3 (quando possivel)
   - UDP basico
   - IPv4 obrigatorio; IPv6 fica para fase seguinte

5. Registrar evidencias
   - Comandos utilizados
   - Logs de saida
   - Resultado de IP publico por app
   - Limitacoes encontradas

### Evidencias coletadas (2026-09-05)

- Ferramenta instalada: .NET SDK 8.0.424 (C:\Program Files\dotnet\dotnet.exe), permitindo `dotnet build` e execução do PoC.
- Comandos e resultados:
  - `dotnet run --project poc/NetLane.NetworkPoC/NetLane.NetworkPoC.csproj -- --firewall --target-wifi explorer --target-ethernet msedge`
    - Modo ativo: `firewall (experimental)` com aplicação de regras para `explorer.exe` e `msedge.exe` sem erro.
  - `dotnet run --project poc/NetLane.NetworkPoC/NetLane.NetworkPoC.csproj -- --firewall --target-wifi explorer --target-ethernet msedge --check-public-ip`
    - IP público coletado por app (Wi-Fi e Ethernet), com valores distintos no instante da execução.
  - `dotnet run --project poc/NetLane.NetworkPoC/NetLane.NetworkPoC.csproj -- --wfp --target-wifi explorer --target-ethernet msedge`
    - Queda controlada para `dry-run` com mensagem de permissão insuficiente, validando fallback do motor.
  - `dotnet run --project poc/NetLane.NetworkPoC -- --wfp --target-wifi explorer.exe --target-ethernet steam.exe --check-public-ip`
    - Mantido em `dry-run` por falta de privilégios de administrador; não há enforcement WFP neste ambiente.
    - IP público por app exibido e diferente por interface, útil como baseline, mas não prova isolamento por app no kernel.
- Limitação observada: sem privilégios de administrador não é possível validar implementação ativa de WFP neste ambiente.

6. Gate de avancÌ§o

Somente avancar para Fase 1 quando:
- O PoC funcionar de forma repetivel com motor real de roteamento.
- Houver comportamento fail-open em caso de falha.
- Nao houver efeito colateral global de roteamento.

### Validacao WFP em modo kernel (administrador)

- Executar em PowerShell elevado:
  - `dotnet run --project poc/NetLane.NetworkPoC -- --wfp --target-wifi explorer.exe --target-ethernet steam.exe --check-public-ip`
- Validacao esperada:
  - `Modo ativo: wfp (kernel)`.
  - `Roteamento aplicado: Sim (intencional)`.
  - `Regras preparadas:` com contagem de filtros por app (`filtros > 0`).
- Critério de aceitação:
  - IP publico por app deve refletir interfaces distintas (ou evidência equivalente de isolamento por aplicação).
- Falhas:
  - Se cair em dry-run, revisar privilégio de administrador.
  - Se `filtros=0`, registrar `code=` retornado em falha de criação de filtro e ajustar regra de camada/condição.

### Resultado de validacao (2026-09-05)

- Sessao administrador (comando: `dotnet run --project "C:\Codes\netlane\poc\NetLane.NetworkPoC" -- --wfp --target-wifi explorer.exe --target-ethernet steam.exe --check-public-ip`):
  - Modo ativo: `wfp (kernel)`.
  - Roteamento aplicado: `Sim (intencional)`.
  - Regras preparadas:
    - `explorer.exe => WiFi (...) | filtros=2`
    - `steam.exe => Ethernet (...) | filtros=2`
  - Evidência coletada: IP público reportado sob o nome de cada app, mas consultado via `curl --interface`; valores distintos comprovam apenas o acesso das interfaces naquele instante.
  - Status revisado: instalação de filtros por app validada; direcionamento real do tráfego por aplicativo não demonstrado.

### Conclusao da fase

- Caminho de instalação WFP funcional no PoC, com fallback inicial para dry-run. A conclusão anterior de viabilidade de roteamento foi revista.
- Evidências obtidas:
  - Modo `wfp (kernel)` em sessão admin.
  - Roteamento intencional ativo (`Sim (intencional)`).
  - `filtros > 0` por app (`2`) em regras aplicadas.
- Critérios ainda pendentes:
  - implementar e comprovar direcionamento de novas conexões dos executáveis alvo; PERMIT/BLOCK não escolhem uma nova saída;
  - verificar a tabela de rotas antes/depois, fail-open em falhas e encerramento abrupto;
  - validar TCP, UDP/QUIC e comportamento sob carga.
- As fases de monitoramento, políticas e editor avançaram como infraestrutura; não substituem essa prova de viabilidade.

### Transicao para Fase 1

- A Fase 1 (Monitor) foi iniciada com o serviço `NetLane.Service` operando em loop de coleta:
  - detecta adaptadores;
  - detecta aplicações com conexões (processo/executável);
  - detecta conexões ativas e associa por PID para ranking de apps.
- Não há mudanças de rota nesta fase; continua como observabilidade.

- Transição para a Fase 2:
  - implementação inicial de aplicação de política de rota por aplicativo com rollback no `StopAsync` do serviço.
