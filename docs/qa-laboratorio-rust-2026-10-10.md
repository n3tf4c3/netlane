# Preparação do laboratório Rust — 2026-10-10

Esta etapa prepara insumos e coleta em leitura. Os cenários de instalação Rust
ainda não foram executados. A autorização de ensaios antigos do pacote .NET
não é usada como autorização para instalar Rust, iniciar agente ou alterar rede.

## Alvo e acesso

`vmrun list` confirmou a VM `Win11 - Work` em execução em
`C:\VMs\Win11 - Work\Win11 - Work.vmx`. O MAC configurado é
`00:0c:29:2f:4a:d0`; Windows 11 x64 é o sistema declarado na configuração,
ainda sem conferência no convidado. `listSnapshots` recusou a consulta com
`A password is required for this operation`. Nenhuma senha de login foi
reutilizada, nenhum snapshot foi criado e nenhum arquivo foi transferido.

O acesso precisa ser resolvido localmente no VMware, sem colocar senhas nos
planos/recibos ou mudar proteções. O snapshot deve ser criado/conferido e ter
nome e registro antes dos passos que alteram o produto. Um arquivo `.vmsd`
vazio não comprova snapshot; a leitura da configuração não comprova o estado
do Windows convidado.

## Preparar os insumos

```powershell
.\scripts\prepare-rust-installer-lab.ps1 `
  -BuildDirectory '.\artifacts\rust-installer-AAAAmmdd-HHMMSS' `
  -SevenZipPath 'C:\Program Files\7-Zip\7z.exe'
```

A preparação reconfere a extração estática e cria uma pasta nova sob `artifacts`,
com somente setup Rust, manifesto, recibo do build/verificação, coletor, plano
incompleto e este roteiro. Cada arquivo tem SHA-256 no inventário do bundle.
Não inclui instalador .NET, perfil, regras, credenciais nem ferramentas remotas.
Não transfere arquivos nem executa o setup. A versão/hash vêm do pacote validado.

## Identificar e capturar no convidado

Com acesso disponível, escolher uma pasta nova no convidado e conferir os hashes
do bundle recebido. Abrir PowerShell x64 com token normal do usuário de QA,
sem elevar a instalação por usuário. O plano já contém o nome do host recusado
e o MAC esperado; identidade do convidado e snapshot começam vazios.

```powershell
.\scripts\capture-rust-installer-lab-state.ps1 `
  -PlanPath '.\plan.json' -IdentifyOnly -Label 'identity'
```

O recibo informa computador, SID, UUID, MAC/IP, sistema, token e pasta. Conferir
esses dados com a VM aberta antes de preencher `Target.Computer`, `UserSid`,
`Uuid` e `LabRoot`. Guardar em `input` um registro da conferência do snapshot;
preencher `Target.Snapshot.Name`, `Verified`, `RecordPath` e `RecordSha256`.
O coletor verifica o hash desse registro; a confirmação de que o snapshot é
recuperável continua sendo uma verificação externa explícita. Preservar o plano
original e salvar um plano novo preenchido na mesma pasta, sem substituir recibos.

```powershell
.\scripts\capture-rust-installer-lab-state.ps1 `
  -PlanPath '.\plan-confirmado.json' -Label 'before-clean-install'
```

O baseline inclui arquivos/registro Rust e .NET, perfis, atalho, hashes de todos
os valores HKCU Run, processos e estado de interfaces/bindings/métricas/rotas
padrão/DNS. Guarda hashes dos perfis, sem exportar seu conteúdo. Não segue links
encontrados nas árvores. Leituras que falham interrompem a coleta; não representam
um baseline aprovado. Os recibos sempre mantêm `RuntimeQaVerified=false`.

## Primeira rodada proposta

| Caso | Passo após identificação/snapshot e autorização de execução | Evidência de aprovação |
| --- | --- | --- |
| Cancelamento | abrir setup Rust e cancelar antes da cópia | estado do produto/autorun/perfis igual ao baseline; relato visual |
| Instalação limpa | executar setup Rust validado no token normal | payload por hash, registro/atalho corretos, nenhuma abertura automática/UAC/rede; UI instalada em demonstração verificada separadamente |
| Reinstalação/reparo simples | adicionar dados fictícios/arquivo desconhecido, reinstalar a mesma versão | payload íntegro e fixtures preservadas; não é prova de upgrade entre versões ou falha parcial |
| Remoção | remover a instalação Rust desta rodada | arquivos conhecidos removidos, perfil/fixtures preservados, registro/atalho Rust removidos e .NET/Run estrangeiro preservados |

Capturar estado antes/depois de cada caso com um `Label` distinto. Comparar
caminhos/tamanhos/hashes, registro e configuração, sem exigir igualdade de
horários, PIDs ou estados de conexão voláteis. O código de saída sozinho não
aprova o caso. Guardar início/fim, relato/captura dos passos visuais e a prova
de ausência de UAC na UI. Não abrir painel e agente ao mesmo tempo por inferência.
O primeiro painel em `--demo` deve ter seu encerramento normal confirmado antes
da reinstalação/remoção; não encerrar processos à força.

Interromper ao encontrar bloqueio de proteção, identidade divergente, produto
ou dados anteriores sem baseline, processo em uso ou qualquer alteração de rede
inesperada. Recuperação de snapshot só ocorre no alvo autorizado; não é prova
de rollback do instalador. Guardar estados de falha antes de decidir reparo.

## Gates separados

Upgrade real/downgrade precisam de dois pacotes Rust com versões distintas.
Falta/versão antiga de Visual C++ e ICU, falha parcial, links/arquivos em uso,
outros usuários, autorun/logon e demais escalas precisam de cenários próprios.
Não remover runtimes compartilhados nem alterar o logon por inferência.
Agente elevado, qualidade, WFP, rotas/métricas, IPv6 e reboot continuam fora
desta primeira rodada. Assinatura, publicação Git e entrega pública seguem
pendentes. O coletor não é um executor desses cenários.
