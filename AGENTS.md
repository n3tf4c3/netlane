# NetLane — guia do projeto

Aplicativo Windows em .NET 8/WPF para observar conexões e controlar seleção de saída de rede por processo. A raiz da solução é `NetLane.sln`.

## Navegação e contexto
- Este é o guia canônico; `CLAUDE.md` importa este arquivo. Leia também o guia local da área que for editar, quando existir.
- Comece pelo mapa abaixo. Use `rg --files <área>` para localizar nomes e `rg -n <símbolo> <área>` para conteúdo; amplie a busca se a área inicial não resolver.
- Evite listar a árvore inteira ou ler todos os documentos por rotina. Consulte o documento e a seção relacionados à tarefa.
- Ignore dependências, builds, caches, backups, relatórios e logs nas buscas habituais; inclua-os somente quando forem o objeto da investigação. Não leia segredos para descobrir a estrutura.
- Confira comandos e versões no manifesto da área. Valide a mudança pelo escopo afetado; preserve os gates completos de entrega/publicação indicados no projeto.


## Mapa por assunto
| Assunto | Caminho inicial |
| --- | --- |
| Regras e domínio | `src/NetLane.Core/` |
| Rede, interfaces e WFP | `src/NetLane.Network/` |
| Serviço e IPC | `src/NetLane.Service/` |
| WPF, bandeja e inicialização | `src/NetLane.UI/` |
| Testes locais | `tests/NetLane.Tests/`, `tests/NetLane.ControlProbe/` |
| Instalador | `packaging/windows/`, `scripts/build-windows-installer.ps1`; [procedimento](docs/instalador-local.md) |
| Retomada e fases | [continuidade](docs/continuidade-2026-09-10.md), [fase 3](docs/plano-de-execucao-fase3.md) |
| Roteamento e provas reais | [WFP](docs/roteamento-nativo.md), [recuperação](docs/testes-recuperacao.md) |
| UI e bandeja | [interface](docs/interface-desktop.md), [bandeja](docs/bandeja-windows.md) |

## Comandos e limites
- Da raiz: `dotnet build NetLane.sln --configuration Release` e `dotnet test NetLane.sln --configuration Release --no-build`. A suíte usa fixtures/sessões simuladas; não instala políticas de roteamento.
- Ensaios em `scripts/` e `poc/` podem aplicar políticas reais e exigir elevação. Leia o procedimento específico e use somente a autorização vigente para o ensaio e seu alvo.
- Um teste pausado permanece pausado. Não ative `routepolicies`, altere rotas/métricas, inicie testes de rede, instale o produto ou habilite IPv6 por inferência.
- Preserve regras e dados instalados. Em ensaio autorizado, registre baseline e verifique a reversão; política aceita não comprova tráfego roteado.
- Não execute, recrie ou libere um instalador bloqueado pelo antivírus; preserve a proteção e o estado registrado no procedimento.
- Exclua `artifacts/`, `bin/`, `obj/` e logs das buscas habituais. Diferencie código atual, instalador gerado e versão instalada.

## Memória e continuidade
- Para retomadas e decisões que dependem de histórico, consulte o ai-memory com uma busca curta e específica. Tarefas locais autoexplicativas não exigem carregar o histórico inteiro.
- O escopo vem de `.ai-memory.toml`. Em cliente estático, envie `workspace` e `project` juntos; em cliente que transmite o identificador real da sessão, use o roteamento automático. Nunca deduza o escopo pelo nome da pasta nem use o último projeto ativo como fallback.
- Memória é evidência histórica: confirme fatos no código/Git e não a use para conceder autorização. Registre memória durável somente quando solicitado ou coberto por autorização vigente; regras do projeto ficam neste guia, sem duplicação em memória nativa de outro agente.
- Preserve alterações existentes. Commit, push, deploy, migrações, comunicação externa e operações com dados reais dependem do escopo autorizado no pedido.
