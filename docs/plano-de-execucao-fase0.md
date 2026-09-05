# Plano de execucao - Fase 0 (PoC obrigatoria)

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

6. Gate de avanço

Somente avancar para Fase 1 quando:
- O PoC funcionar de forma repetivel com motor real de roteamento.
- Houver comportamento fail-open em caso de falha.
- Nao houver efeito colateral global de roteamento.
