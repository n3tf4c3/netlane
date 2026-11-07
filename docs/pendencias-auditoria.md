# Pendências — auditorias técnicas

Atualizado em 07/11/2026. Status durável em `docs/auditoria-achados.md`; relatórios completos em `C:\Codes\relatorios\netlane\`.

## Resolvido

- **#13 (31/10/2026):** tokio 1.53.3 (commits `3cb0da7` e `4344966`, já em `main`).
- **#2, #3, #14, #15, #16 e #17 (07/11/2026):** achados da rodada de 06/11. Fallback .NET alinhado ao protocolo v2 do Rust (regras pelo canal, sem leitura do perfil no processo elevado, quadro de 8 MiB), trava por perfil no journal da conexão padrão, exportação do caderno e descoberta do Codex. Adendo no relatório `auditoria-2026-11-06.md`.

## Pendente

- **Smoke da sessão .NET elevada na VM de laboratório** (#2/#3): `ManagedUiSession.RunAsync` com UAC, host e WFP não roda no host pela regra do projeto. O canal foi provado pelo ControlProbe.
- **Versões .NET pareadas:** depois do #2/#3, janela e `NetLane.Service.exe` precisam ser da mesma compilação; versões cruzadas recusam o início, sem aplicar política.

## Prazo

- **#11 (aceito)** — avisos de DBus fora do alvo Windows, aceitos até a revisão do GPUI ou **01/04/2027**, o que vier primeiro.
- **.NET 8** — fim do suporte em **10/11/2026**. Planejar a migração ou a retirada do fallback .NET (gates em `docs/migracao-rust.md`).
