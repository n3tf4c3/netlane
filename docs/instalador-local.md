# Instalador local — prévia x64

Preparação implementada em **2026-09-13**, após o commit `ba332a2` e os ensaios de bandeja. **Instalador gerado, mas não executado nem liberado para distribuição.** A instalação real, a desinstalação e a atualização ainda precisam de validação manual autorizada. Um bloqueio de segurança no verificador auxiliar está registrado abaixo e deve ser esclarecido antes dessa etapa.

## Escopo e caminhos

- Prévia `0.3.0`, Windows x64 Intel/AMD; ARM64 não validado. O instalador exige Windows 10 ou posterior; isso não garante que a API de roteamento esteja disponível. Os pré-requisitos continuam sendo verificados pelo serviço.
- Publicações separadas de UI e serviço, **self-contained .NET 8.0.30** neste build, sem exigir SDK/runtime instalado. O script registra a versão efetivamente publicada.
- Programa em `%ProgramFiles%\NetLane`, destino fixo protegido; serviço em `service\NetLane.Service.exe`. Instalador e desinstalador pedem UAC. O painel tem manifesto **asInvoker** e deve ser aberto pelo menu Iniciar sem elevação.
- Regras da instalação: `%LOCALAPPDATA%\NetLane\netlane-rules.json`. Preferências: `%LOCALAPPDATA%\NetLane\interface-selection.json`, mantendo o caminho anterior das preferências.
- O marcador `netlane-installed.layout` separa o pacote do checkout. Mesmo iniciado com o diretório de trabalho dentro do repositório, o pacote não usa suas regras nem eleva um serviço de outro build. Serviço ausente no pacote não causa fallback.
- No checkout, permanecem `src/NetLane.Service/netlane-rules.json` e os caminhos de build anteriores. Uma publicação portátil **sem marcador** mantém o comportamento antigo; não deve ser confundida com a instalação.
- Perfil sem regras começa vazio. **Não há importação automática das regras de desenvolvimento.** Uma eventual migração precisa de cópia/backup separado, com painel e serviço encerrados. Dados existentes do perfil não são substituídos.
- A sessão elevada recebe da UI o caminho absoluto das regras do usuário. Não foi criado serviço Windows permanente, conta de serviço ou configuração compartilhada entre usuários.

## Limites do instalador

Não inicia UI/serviço ao terminar, não oferece execução elevada do painel, não cria autostart, tarefas agendadas ou regras de firewall e não modifica rotas, métricas, DNS, bindings IPv6 ou `routepolicies`.

As guardas escritas e compiladas verificam processos `NetLane.*` por snapshot Toolhelp antes de copiar/remover arquivos, inclusive em outras sessões; não encerram processos. Também verificam links/junções nos caminhos conhecidos e abertura exclusiva dos arquivos existentes, recusam destino arbitrário e downgrade. O usuário deve sair normalmente pela bandeja e aguardar a limpeza do serviço. **O comportamento dessas guardas dentro do instalador ainda não foi comprovado em execução.**

A remoção usa uma lista de arquivos compilada no executável, não um manifesto de deleção editável em disco. Não usa exclusão recursiva, não visita perfis e deixa arquivos desconhecidos nas pastas do programa. Erros de cópia/remoção são tratados como operação incompleta, não como sucesso. Não é um instalador MSI transacional: rollback após falha, concorrência durante a cópia e atualização entre versões diferentes ainda precisam de testes. Versões futuras também precisarão tratar explicitamente arquivos antigos que deixarem o pacote.

## Reprodução do build

Compilador **NSIS 3.12 portátil**, sem instalação global, extraído somente em `artifacts`. A distribuição está documentada na [página oficial do NSIS](https://nsis.sourceforge.io/Download). Neste ambiente, os primeiros downloads retornaram HTML e foram recusados pela conferência de hash. O ZIP foi obtido pelo espelho MacPorts/MIT e conferido contra o [checksum publicado pelo MacPorts](https://github.com/macports/macports-ports/blob/master/devel/nsis/Portfile):

```text
nsis-3.12.zip
SHA-256: 56581f90db321581c5381193d796fffcf2d24b2f8fed2160a6c6a3baa67f2c4f
```

O build não baixa nem instala o compilador automaticamente. Informe `makensis.exe` de uma distribuição conferida; versões diferentes são recusadas.

```powershell
.\scripts\build-windows-installer.ps1 -NsisCompilerPath '.\artifacts\installer-tools-20260913\compiler\nsis-3.12\makensis.exe'
```

O script cria uma pasta **nova** em `artifacts\installer`, publica somente UI/serviço, inclui avisos de terceiros dos pacotes, inventário de arquivos com SHA-256, revisão Git/indicação de worktree alterada, gera o instalador e grava `build-report.json` e `.sha256`. Não executa o instalador e não sobrescreve builds anteriores. NSIS compila com avisos tratados como erros. A [licença do NSIS](https://nsis.sourceforge.io/License) e as licenças fornecidas pelos pacotes acompanham a prévia; revisão completa de redistribuição permanece separada da preparação local.

Conferência estática com 7-Zip já disponível nesta máquina:

```powershell
.\scripts\test-windows-installer.ps1 -BuildDirectory '.\artifacts\installer\preview-20260913-v5' -SevenZipPath 'C:\Program Files\7-Zip\7z.exe'
```

Essa conferência **extrai**, não executa o EXE. Confere hash, inventário, arquitetura x64, manifestos de privilégio e ausência de regras locais/probes/hosts de revisão. O relatório mantém `InstallerExecuted`, `RuntimeGuardVerified` e `InstallationVerified` como `false`.

## Validação e problema encontrado

- Artefato final: `artifacts/installer/preview-20260913-v5/NetLane-0.3.0-preview-win-x64-setup.exe`, sem assinatura e sem execução. SHA-256: `4EDCA5DDFC592545CE91BB099B9172D70C09508AA6BFDBBCE0CB0E7E12D83D3E`.
- Extração estática: **758 arquivos de payload** conferidos, UI/serviço x64, painel `asInvoker` e instalador/desinstalador `requireAdministrator`. Recibo: `artifacts/installer/preview-20260913-v5/archive-check-6ddab9c1/verification.json`. `Passed=true` refere-se somente a essas verificações; instalação e guardas runtime continuam `false`.
- Build Release da solução: **0 avisos, 0 erros**.
- **336/336 testes locais**: 224 de produção (oito novos de caminhos/persistência) + 75 QUIC + 37 do host de bandeja. Não repetem os ensaios elevados de rede.
- Resultados em `artifacts/installer-validation-final-20260913/test-results/`: `production.trx`, `quic.trx`, `tray-review.trx`. Onze scripts PowerShell analisados sem erro de sintaxe.
- O Worker SDK incluía arquivos JSON automaticamente, levando regras locais e heartbeat para a publicação inicial. O build recusou o conteúdo **antes de criar instalador**. O projeto agora permite apenas `appsettings.json` como conteúdo JSON/config distribuído; suas políticas de fábrica precisam ser vazias. O empacotador também recusa regras, preferências, recibos, backups, logs, PDBs e ferramentas de ensaio.
- Foram removidas somente oito cópias geradas nesta rodada antes da correção: quatro no build recusado `preview-20260913-v1` e quatro no primeiro build de testes `installer-validation-20260913`. Originais preservados; essas cópias não eram a fonte dos dados.
- SHA-256 das regras originais, antes/depois: `2E906DDE5D2EAE4CB468650395D19D3E9E68157E562D3AF723AFFAF3984C5B2F`.

### Revalidação antes do commit/push

Em **2026-09-13**, após autorização específica para registrar o envio à McAfee e versionar esta preparação, foi repetido o build Release da solução (**0 avisos, 0 erros**) e as três suítes: **224 + 75 + 37 = 336 aprovados**, sem falhas ou testes ignorados. Resultados em `artifacts/prepublish-installer-20260913T141340Z/test-results/`, nos arquivos `production.trx`, `quic.trx` e `tray-review.trx`. Onze scripts PowerShell passaram pela análise sintática, sem executar os roteiros de rede ou os auxiliares NSIS.

Os hashes do instalador v5, do ZIP enviado para análise e das regras originais foram conferidos e preservados. O instalador e o verificador não foram recompilados nem executados nesta revalidação; a prova por extração continua sendo a conferência estática anterior, sem comprovação de instalação ou das guardas em execução. Não havia processos NetLane restantes após os testes. Não há workflow de CI configurado; esses resultados são locais. Binários, ZIP, recibos e alterações de regras pessoais não entram no commit.

### Bloqueio de segurança — pendente

O verificador somente leitura `guard-check.exe`, compilado a partir de `GuardCheck.nsi`, teve sua execução recusada pelo Windows com a mensagem de que o arquivo contém vírus ou software possivelmente indesejado. **Não foi estabelecido que seja falso positivo.** A consulta local inicial identificou Defender e McAfee registrados, mas não atribuiu o bloqueio a um produto/ameaça específica. O agente não criou exceções, desativou proteção ou restaurou arquivo em quarentena. A única retentativa e o encaminhamento posterior estão registrados abaixo; não houve nova execução após esse bloqueio repetido.

Consequência: não há prova runtime do detector com processo ausente/presente. `GuardFixture.nsi` existe apenas como fixture futura que termina sozinha; não foi executada. O instalador completo não foi aberto, e sua compilação/extração não prova aprovação pelo antivírus. O usuário informou o envio para análise do fornecedor, conforme a seção seguinte. O encaminhamento não libera novos testes, instalação ou alterações na proteção.

## Próximas etapas

### Envio informado à McAfee — análise pendente

Em **2026-09-13 às 08:03**, após o usuário fornecer a captura e autorizar a preparação, foi criado `artifacts/mcafee-review/20260913-075736/NetLane-McAfee-review-44929CBE.zip` (**49.509 bytes**; SHA-256 `841769FB074317A71AF7271214B99D5AF7FEF1A629DD29CB6F7C121D95ABB805`). A captura confirma o alvo `guard-check.exe` e o horário 07:50; o nome específico da ameaça não está visível. O mesmo verificador voltou a estar disponível no disco com hash idêntico, sem restauração feita pelo agente; não houve nova execução.

O ZIP contém somente sete arquivos conferidos: a amostra exata de 46.724 bytes, `System.dll` extraída sem execução, avisos NSIS, relatório técnico em inglês, texto para o campo **Details**, hashes e instruções. A DLL confere com o componente `x86-unicode` do compilador. Não foram incluídos instalador principal, regras pessoais, preferências, logs brutos, caminhos de perfil ou código-fonte completo. O relatório diferencia revisão de código e intenção de comportamento de uma análise independente do binário; o código atual não é apresentado como reprodução exata dessa compilação antiga.

Em **2026-09-13**, o usuário informou ter enviado o ZIP pelo [formulário oficial da McAfee](https://www.mcafee.com/en-us/consumer-support/dispute-detection-allowlisting.html) e que **não foi gerado protocolo**. Essa informação vem do relato do usuário; não há recibo independente do fornecedor ou parecer disponível nesta conversa. Nenhum upload ou abertura de caso foi feito pelo agente. Nome, e-mail e conteúdo pessoal do formulário não foram incluídos neste registro público.

**Próximo passo atual:** aguardar o parecer e, quando recebido, conferir sua correspondência com a amostra/hash submetidos antes de decidir sobre novo teste. O falso positivo continua não confirmado; verificador e instalador permanecem sem nova execução. `verification.json`, junto ao ZIP, comprova somente a preparação local, não o envio ou a liberação pela McAfee. O usuário autorizou registrar este ponto e fazer commit/push das fontes, scripts, testes e documentação, sem binários, ZIP ou dados locais. Assinatura, instalação e distribuição não fazem parte dessa autorização.

### Histórico da retentativa e etapas restantes

Atualização em **2026-09-13 às 07:52 (America/Cuiaba)**: após o usuário informar que retirou a classificação de falso positivo, foi feita **uma retentativa do mesmo verificador**, conferindo antes seu SHA-256 `44929CBE1677B7EED6F8AA6A6D00AFF7894500099215AECEF2B42668E688184E`. O Windows novamente recusou o início por vírus/software possivelmente indesejado. Na conferência posterior, `guard-check.exe` já não existia no disco; não foi restaurado nem recriado. Não houve nova compilação, instalação, início do serviço ou alteração da proteção.

O Defender informou `Not running`, sem detecção correspondente; não houve evento correspondente de Code Integrity. `McAfee Framework Host` e `McAfee WebAdvisor` estavam em execução. Naquele momento isso apontava para McAfee como hipótese, **sem confirmar o produto ou a ameaça responsável**. A captura solicitada foi recebida posteriormente e identificou o arquivo e o horário, mas não mostrou o nome específico da ameaça. Instalador e regras originais mantiveram os hashes registrados acima. Recibo local: `artifacts/installer-antivirus-review-20260913-075215/retry.json`.

1. Aguardar a análise da McAfee e esclarecer o bloqueio de segurança sem contornar a proteção.
2. Com autorização específica, conferir o detector e a instalação inicial, cancelamento/UAC, atalho sem elevação, salvamento sem escrever em Program Files e ausência de autostart/alterações de rede.
3. Conferir recusa com painel na bandeja/serviço aberto, reinstalação, upgrade/downgrade, falha de cópia e desinstalação preservando regras/preferências/arquivos desconhecidos. Iniciar roteamento durante QA requer autorização própria.
4. Assinatura, auditoria de distribuição e release continuam pendentes, com decisões separadas do commit/push de código autorizado nesta rodada.

VPN permanece uma evolução posterior: primeira compatibilidade a investigar é Check Point, com modelo genérico por capacidade do adaptador. Não há suporte universal nem proteção contra vazamentos implementados. Respeitar split tunnel/Hub Mode definido pelo administrador; não iniciar testes de VPN, ativar IPv6 ou alterar políticas corporativas nesta etapa.
