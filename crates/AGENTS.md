# NetLane em Rust

Workspace Cargo paralelo à solução .NET, que permanece referência durante a migração.
GUI GPUI na mesma revisão do termIAX; cores somente no tema e recursos com licenças preservadas.

- `netlane-core`: modelos, JSON compatível, revisões, regras e medições, sem rede.
- `netlane-capture`: importação PCAP/PCAPNG, análise incremental limitada, comparação de referência e escrita rotativa com fonte tipada; testes usam fonte fictícia.
- `netlane-ai`: contexto revisável, probe de metadados e conversa explícita com Codex App Server em perfil próprio sem ferramentas; testes comuns usam servidores fictícios, sem login ou inferência reais.
- `netlane-windows`: consultas Win32, WFP e adaptador Npcap optativo; FFI isolado e documentado. Não abrir captura real para validar o port sem escopo/alvo autorizados.
- `netlane-agent`: planejamento e execução explícita de sessões.
- `netlane-app`: painel GPUI e preferências próprias de aparência.

Validação: `cargo fmt --all -- --check`, `cargo test --workspace --locked`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo build --workspace --release --locked`.
Testes comuns usam fixtures e diretórios temporários. Não ativar roteamento, qualidade,
alterar rotas/IPv6 ou instalar binários para validar o port. Ensaios reais seguem AGENTS.md da raiz.
Modo `--demo` usa dados fictícios e não consulta a rede nem o perfil instalado.
