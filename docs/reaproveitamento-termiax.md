# Reaproveitamento do termIAX no diagnóstico NetLane

Referência consultada em 10/10/2026: checkout `C:\Codes\termiax`, HEAD
`2f91ac5aa7b38e1408a446af27df0bf4fa379883` mais o conteúdo local observado.
O checkout foi usado somente para leitura. O usuário solicitou reaproveitamento
da própria ferramenta. O código original é proprietário:
**Copyright (c) 2026 Paulo Gomes. Todos os direitos reservados.**
Esta adaptação mantém esse aviso; não altera a licença nem incorpora dependência
de build em caminho absoluto do termIAX.

| Referência | Adaptação no NetLane |
| --- | --- |
| `termiax-app/src/capture_panel.rs` | `netlane-app/src/packet_view.rs`: tabela virtualizada, colunas, cores por protocolo, serviço estimado e RAW opcional |
| `termiax-capture/src/stream.rs` | Referência de fila/limites; o transporte SSH não foi copiado nem ativado |
| `termiax-acp/src/agent.rs` | `netlane-ai/src/agent.rs`: comandos/eventos tipados por conversa; contexto de pacote substitui SSH |
| `termiax-acp/src/codex.rs` | `netlane-ai/src/bridge.rs`: App Server, perfil separado, login, streaming, modelo e interrupção |
| `termiax-acp/src/codex-model.json` | `netlane-ai/src/proposal-model.json`: catálogo sem ferramentas, instruções adaptadas para diagnóstico |

Codex usa App Server; o contrato comum do termIAX permite adaptadores ACP para
outros provedores. Claude/Gemini ainda não têm adaptadores no NetLane. Não há SSH,
execução de comandos, sondas ou acesso do agente aos arquivos da captura. O
adaptador local Npcap foi escrito no NetLane, sem copiar o transporte SSH.

A captura de referência do termIAX tem limite de 300 segundos, 1024 pacotes e
8 MiB. Copiar esses limites não atende uma coleta longa. O NetLane passa a ter
scanner de dumps completos, estatísticas limitadas, até 512 amostras distribuídas,
cronologia que coalesce e baseline explícito. A pipeline de captura local com
rotação/perdas e análise de segmentos fechados já tem provas fictícias em D2;
driver real e operação prolongada ainda precisam de QA próprio.

Na adaptação da IA, as filas passaram a ser limitadas; frames parciais sobrevivem
a cancelamento de leitura. O cancelamento aguarda conclusão do turno ou fecha o
processo após o prazo. Respostas de outras threads/turnos são ignoradas. A versão
suportada é Codex 0.162.0, verificada com perfil próprio e thread efêmera, sem
inferência. Testes do termIAX não substituem as provas específicas do NetLane.

As estruturas e limites atuais, provas e pendências estão em
[diagnóstico com IA](diagnostico-rede-ia.md).
