# Ledger de achados de auditoria

Registro durável da numeração e do status dos achados de auditoria técnica. Os relatórios completos ficam fora do Git (`C:\Codes\relatorios\netlane\`). O próximo número é `max(maior # abaixo, maior "achado N" no git log) + 1`; a numeração nunca reinicia.

Status: `ABERTO`, `PARCIAL`, `RESOLVIDO`, `REVISAR`, `ABERTO (aceito)`, `ADIADO`, `DOCUMENTADO`, `FALSO POSITIVO (justificado)`.

| # | Achado | Severidade | Status | Origem / resolução |
| --- | --- | --- | --- | --- |
| 1 | Agente elevado executado de pasta gravável pelo usuário (instalação per-user e ZIP portátil), sem verificação de integridade nem `SetDefaultDllDirectories` | Alta | PARCIAL | auditoria-2026-10-11-044843. 2026-10-11: DLL planting resolvido (`/DEPENDENTLOADFLAG:0x800` + `harden_dll_search`, teste `dll_search.rs`); edição portátil sem elevação (feature `portable`, prévia sem `netlane-agent.exe`, teste `portable_build_never_resolves_the_elevated_agent`). Instalador Rust convertido para instalação por máquina em `%ProgramFiles%\NetLane Rust` (admin, HKLM; recusa a prévia per-user e token sem administrador), com compilação, extração estática e recusas negativas aprovadas. Pendente: QA runtime do novo instalador no Sandbox/VM (instalar, atualizar, remover, recusas 4 e 9) |
| 2 | Sessão elevada recarrega a cada 10 s regras de arquivo gravável pelo usuário e testa caminhos UNC antes de recusá-los | Média | RESOLVIDO | 2026-10-11: protocolo v2 (`Start`/`Update` levam as regras pelo canal autenticado); `is_local_path` antes de qualquer acesso ao disco |
| 3 | Falha transitória no ciclo de 10 s (JSON, `snapshot()`, WFP, `Snapshot` > 64 KiB) encerra toda a sessão de roteamento | Média | RESOLVIDO | 2026-10-11: `apply_cycle` recuperável, `monitor::adapters()`, quadro de 8 MiB, `MAX_POLICIES` 4096; sem isolamento por regra (decisão registrada no relatório) |
| 4 | Monitor Rust lista só TCP/IPv4 estabelecido; referência .NET inclui UDP; IPv6 ausente nas duas | Média | RESOLVIDO | 2026-10-11: TCP estabelecido + UDP, IPv4 e IPv6; `Connection.protocol` compatível; UI e `docs/migracao-rust.md` atualizados |
| 5 | Leases globais `NetLane.RoutingSession.v1`/`NetLane.DefaultConnection.v1` podem ser ocupados por processo não elevado | Baixa | ABERTO | Não corrigido: o lease é compartilhado com a versão .NET para exclusão entre versões. Reabrir quando a referência .NET for retirada ou com decisão de aceitar a perda dessa exclusão |
| 6 | IPv4 com Total Length 0 (LSO/TSO) classificado como cabeçalho inválido no decodificador | Baixa | RESOLVIDO | 2026-10-11: usa o comprimento capturado e anota TSO; teste em `decode.rs` |
| 7 | Sem CI versionada e sem `deny.toml` para as advisories *unmaintained* do GPUI | Baixa | RESOLVIDO | 2026-10-11: `.github/workflows/ci.yml` e `deny.toml` criados; primeiro run no GitHub (38129408760) aprovado nos jobs rust, advisories e dotnet |
| 8 | Falha de `pcap_compile` descarta a mensagem do Npcap (`pcap_geterr`) | Baixa | PARCIAL | 2026-10-11: mensagem de `pcap_geterr` anexada. Pendente: exercitar com Npcap real (captura exige autorização) |
| 9 | `target/` ausente do `.gitignore` — o Cargo gera `target/.gitignore` com `*`; não aparece no `git status` | — | FALSO POSITIVO (justificado) | auditoria-2026-10-11-044843 |
| 10 | Impersonação do cliente elevado pelo pipe de controle — `SECURITY_ANONYMOUS`, ACL explícita, `first_pipe_instance` e PID + horário de criação verificados | — | FALSO POSITIVO (justificado) | auditoria-2026-10-11-044843 |
