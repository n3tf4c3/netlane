# Instalador local — prévia x64

Preparação implementada em **2026-09-13**, após o commit `ba332a2` e os ensaios de bandeja, e versionada em `16a0c49`. **Instalador gerado, mas não executado nem liberado para distribuição.** A instalação real, a desinstalação e a atualização ainda precisam de validação manual autorizada. Em **2026-09-14**, a amostra original iniciou sem o bloqueio anterior, mas retornou código 3. Após diagnóstico e nova autorização, um verificador separado do código atual retornou **0 no cenário sem processos NetLane abertos**. A detecção com processo presente e as guardas integradas ao instalador continuam pendentes; esse sucesso parcial não altera o resultado da amostra original nem significa aprovação do novo hash pela McAfee.

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

### Histórico do bloqueio inicial

O verificador somente leitura `guard-check.exe`, compilado a partir de `GuardCheck.nsi`, teve sua execução recusada pelo Windows com a mensagem de que o arquivo contém vírus ou software possivelmente indesejado. Naquele momento não havia parecer do fornecedor. A consulta local inicial identificou Defender e McAfee registrados, mas não atribuiu o bloqueio a um produto/ameaça específica. O agente não criou exceções, desativou proteção ou restaurou arquivo em quarentena. A retentativa de 13/09 também foi bloqueada. Somente após o e-mail apresentado em 14/09 e nova autorização foi executado o reteste registrado abaixo.

Limite mantido: o detector separado passou somente no cenário sem processos abertos, no ensaio posterior de 14/09 registrado abaixo. Ainda não há validação com processo presente. `GuardFixture.nsi` existe apenas como fixture futura que termina sozinha; não foi executada. O instalador completo não foi aberto, e sua compilação/extração não prova aprovação pelo antivírus. O encaminhamento e a resposta do fornecedor não autorizam, por si sós, novos testes, instalação ou alterações na proteção.

## Próximas etapas

### Verificador atual sem processos abertos — 2026-09-14

Com autorização específica, foi compilado **um novo verificador separado**, sem substituir ou executar novamente a amostra enviada à McAfee. A execução única, sem elevação e com janela oculta, terminou às **12:33:02 (America/Cuiaba)** com **código 0**, esperado quando não há processos NetLane abertos. Não houve bloqueio de início nesta tentativa. `NoProcessesCasePassed=true`; **`FullGuardRuntimeVerified=false`**.

- Artefato: `artifacts/guard-current-20260914T162908Z/guard-check.exe`, **46.729 bytes**, x86, sem assinatura e com manifesto `asInvoker`. SHA-256: `38C274DC2B2E45F3686C1BA2D22D15BA046060DD993F8FA5314D80442D6B70BE`.
- Build: NSIS **3.12**, com `/V2 /WX /INPUTCHARSET UTF8` e saída exclusiva na pasta nova, retorno 0. Antes de compilar, os **441 arquivos** do compilador foram comparados por hash com o ZIP conferido: nenhuma diferença ou arquivo adicional. O instalador principal não foi recompilado.
- Fontes inalteradas: `GuardCheck.nsi` com SHA-256 `FCFDBA2856C1BD1697A9041ADACAD624AF24261C49B9B19A266E168FF807EC29` e `ProcessGuard.nsh` com `E81B8F5E04B47E53DC4E36EA3EE94018700ADF7105320C3E5D80E193553F66FD`. A inspeção estática do novo cabeçalho confirmou `Process32NextW(...) i.r2 ?e`, sem chamada separada a `GetLastError`. Não foi necessária nova correção de código.
- Proteção: Central de Segurança retornou `S_OK`/`GOOD` para a categoria antivírus antes e depois. McAfee estava registrado e seu serviço principal em execução no preflight. Isso informa a saúde agregada da proteção, **não um laudo ou inclusão do novo hash na lista de permitidos**.
- Preservação: hashes da amostra original, ZIP submetido, instalador v5, regras e duas fontes mantidos; nenhum processo NetLane/verificador restante. Sem restauração de quarentena, alteração de antivírus/política de scripts, upload, abertura de fixture, instalação ou início de serviço.

Recibo local: `artifacts/guard-current-20260914T162908Z/result.json`; roteiro `verify-current-once.ps1` na mesma pasta. O roteiro recusa repetição se o recibo já existir; não apagar o recibo para reutilizá-lo. A janela de registro, incluindo preflight, foi de **12:32:58 a 12:33:02**; não representa medição isolada do tempo do processo. A suíte .NET de 336 testes não foi repetida nesta rodada, que não alterou código de produção. Artefatos permanecem locais e ignorados pelo Git; sem novo commit/push.

**Próximo passo sujeito a autorização:** conferir o retorno 2 diante de um processo fictício isolado `NetLane.*`, que termina sozinho, e o retorno a 0 após seu encerramento, sem abrir painel/serviço ou instalador. Só depois considerar o QA autorizado das guardas dentro do instalador. O resultado 0 do novo binário é compatível com o diagnóstico abaixo, mas não reconstrói o ramo exato que levou a amostra antiga ao código 3.

### Resposta da McAfee e reteste original — 2026-09-14

Em **2026-09-14**, o usuário apresentou o texto de e-mail da McAfee informando que a submissão foi analisada e que `NetLane-McAfee-review-44929CBE.zip` foi incluído na lista de permitidos. A fonte é o texto fornecido pelo usuário: não houve acesso à caixa postal, verificação de cabeçalhos ou protocolo novo informado. A mensagem cita o nome do ZIP, não um hash nem uma aprovação do instalador completo ou de futuras compilações. O nome corresponde ao pacote preparado; o ZIP local continua com o SHA-256 registrado no histórico abaixo.

Com autorização específica, às **11:44:38–11:44:39 (America/Cuiaba)** foi feita **uma única tentativa do verificador original**, sem elevação e com janela oculta. A amostra no caminho original manteve SHA-256 `44929CBE1677B7EED6F8AA6A6D00AFF7894500099215AECEF2B42668E688184E`, também conferido diretamente na entrada `guard-check.exe` do ZIP, sem extrair ou restaurar o arquivo. O manifesto `asInvoker` foi conferido; não havia processos NetLane ou outro verificador antes da tentativa.

- **Execução permitida nesta tentativa:** o processo iniciou e encerrou, sem a falha de abertura por antivírus observada em 13/09. O arquivo permaneceu disponível com o mesmo hash.
- **Validação funcional não aprovada:** o código de saída foi **3**, usado pelo auxiliar para indicar que não conseguiu concluir a verificação de processos. No cenário sem processos NetLane era esperado 0. A causa exata desse retorno não foi diagnosticada nesta rodada. O binário enviado é anterior à revisão atual do código-fonte; não atribuir o resultado ao código atual nem substituir a amostra para alterar o resultado deste teste.
- **Proteção verificada sem alteração:** `WscGetSecurityProviderHealth(ANTIVIRUS)` retornou `S_OK` e `GOOD` antes/depois; a API informa a saúde agregada da categoria antivírus, não um laudo do arquivo. McAfee estava registrado e seu serviço principal estava em execução. Referências: [consulta de saúde do Windows Security Center](https://learn.microsoft.com/en-us/windows/win32/api/wscapi/nf-wscapi-wscgetsecurityproviderhealth) e [significado dos estados](https://learn.microsoft.com/en-us/windows/win32/api/wscapi/ne-wscapi-wsc_security_provider_health).
- **Preservação:** hashes da amostra, ZIP, instalador v5 e regras originais mantidos. Nenhum processo NetLane/verificador restante. Sem recompilação, restauração de quarentena, exceção/alteração de antivírus, upload, execução do instalador ou início de serviço. O fixture de processo presente não foi iniciado; `RuntimeGuardFullValidation` permanece `false`.

Recibo local: `artifacts/mcafee-retest-20260914T154141Z/result.json`; roteiro de tentativa única em `retest-original.ps1`, na mesma pasta. O roteiro recusa nova execução se o recibo já existir; não reutilizá-lo ou remover o recibo para repetir automaticamente. Os artefatos de envio anteriores foram preservados, inclusive seus textos históricos. Não houve novo build ou repetição da suíte de 336 testes nesta rodada, que alterou somente registros e o roteiro local do reteste. Sem novo commit/push.

Na conclusão do reteste, o próximo passo era diagnosticar o retorno 3 mantendo a amostra submetida intacta. O resultado dessa investigação está na seção seguinte. Não prosseguir para o instalador enquanto a validação funcional não estiver concluída e a instalação não for autorizada separadamente.

### Diagnóstico do código 3 — 2026-09-14

Às **12:25 (America/Cuiaba)**, uma leitura estática do executável original, com SHA-256 preservado, confirmou que ele contém as chamadas `Process32NextW(p r0, p r1) i.r2` e `GetLastError() i.r2` separadas, sem `?e` na enumeração. O cabeçalho NSIS começa no offset 38400; somente seu bloco de 1192 bytes foi descomprimido em memória, produzindo os 6918 bytes esperados. Nenhum executável ou plug-in foi carregado pelo leitor. O resultado concorda com o código anterior recuperado do registro de preparação, mas a correlação com a amostra foi feita diretamente em seus bytes, não presumida a partir do fonte atual.

**Defeito confirmado por inspeção:** a leitura de `GetLastError` ocorre tarde demais para garantir que preserve o resultado da enumeração. O Windows exige que o erro seja capturado imediatamente, pois outras chamadas podem substituí-lo. O System plug-in Unicode do NSIS prepara uma chamada posterior com operações adicionais, incluindo conversão do nome da função e resolução de endereço. Assim, o fim normal da enumeração pode deixar de ser reconhecido como 18 (`ERROR_NO_MORE_FILES`), mantendo o resultado conservador 3. Referências: [contrato de GetLastError](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-getlasterror), [Process32NextW](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-process32nextw) e [System.c do NSIS 3.12](https://raw.githubusercontent.com/kichik/nsis/v312/Contrib/System/Source/System.c).

**Correção já existente no código atual, não aplicada nesta investigação:** `ProcessGuard.nsh` usa `Process32NextW(...) i.r2 ?e`, retira o erro salvo com `Pop $5` e compara `$5` com 18. A [opção `e` documentada pelo NSIS](https://nsis.sourceforge.io/Docs/System/System.html) captura o erro ao fim da chamada e o entrega pela pilha. A amostra antiga não foi recompilada ou substituída para incorporar isso.

**Limite causal e de execução:** esse defeito é a causa provável do código 3 observado, não uma captura do ramo exato executado pelo processo do reteste. Uma consulta complementar somente leitura foi preparada para x86: a primeira invocação identificou host x64 e recusou continuar; a invocação explícita em PowerShell x86 foi recusada antes do script pela política de execução efetiva `Restricted`. Nenhuma enumeração Toolhelp foi executada por esse roteiro, nenhuma política foi alterada e não se obteve reprodução comparativa em execução naquela investigação. O teste separado posterior aprovou somente o cenário sem processos abertos, conforme a seção acima; não exclui todos os outros caminhos de erro.

Evidências em `artifacts/guard-code3-diagnosis-20260914T161621Z/`: `static-diagnosis.json`, leitor reproduzível `inspect-original-header.ps1`, roteiro complementar `inspect-toolhelp.ps1` e a recusa preservada em `toolhelp-x86.stderr.log` (stdout vazio). Os scripts são apenas de diagnóstico local, não fazem parte do pacote. Amostra original, ZIP, instalador e regras mantiveram os hashes; nenhum processo NetLane restante. Sem alteração de código de produção, nova compilação/execução de NSIS, instalação, serviço, alteração de antivírus, commit ou push.

**Encaminhamento posterior:** a compilação e uma execução de verificador separado foram autorizadas e concluídas às 12:33, conforme a seção de resultado acima, sem substituir a amostra enviada ou executar o instalador. A inclusão na lista de permitidos não é presumida para o novo hash. O teste do detector com processo presente e o QA de instalação continuam etapas próprias.

### Histórico do envio à McAfee — 2026-09-13

Em **2026-09-13 às 08:03**, após o usuário fornecer a captura e autorizar a preparação, foi criado `artifacts/mcafee-review/20260913-075736/NetLane-McAfee-review-44929CBE.zip` (**49.509 bytes**; SHA-256 `841769FB074317A71AF7271214B99D5AF7FEF1A629DD29CB6F7C121D95ABB805`). A captura confirma o alvo `guard-check.exe` e o horário 07:50; o nome específico da ameaça não está visível. O mesmo verificador voltou a estar disponível no disco com hash idêntico, sem restauração feita pelo agente; não houve nova execução.

O ZIP contém somente sete arquivos conferidos: a amostra exata de 46.724 bytes, `System.dll` extraída sem execução, avisos NSIS, relatório técnico em inglês, texto para o campo **Details**, hashes e instruções. A DLL confere com o componente `x86-unicode` do compilador. Não foram incluídos instalador principal, regras pessoais, preferências, logs brutos, caminhos de perfil ou código-fonte completo. O relatório diferencia revisão de código e intenção de comportamento de uma análise independente do binário; o código atual não é apresentado como reprodução exata dessa compilação antiga.

Em **2026-09-13**, o usuário informou ter enviado o ZIP pelo [formulário oficial da McAfee](https://www.mcafee.com/en-us/consumer-support/dispute-detection-allowlisting.html) e que **não foi gerado protocolo**. Naquele momento havia somente o relato do usuário, sem recibo independente do fornecedor ou parecer disponível nesta conversa. Nenhum upload ou abertura de caso foi feito pelo agente. Nome, e-mail e conteúdo pessoal do formulário não foram incluídos neste registro público.

Naquele momento, o próximo passo era aguardar o parecer antes de decidir sobre um novo teste. `verification.json`, junto ao ZIP, comprova somente a preparação local, não o envio ou a liberação pela McAfee. O usuário autorizou registrar aquele ponto e fazer commit/push das fontes, scripts, testes e documentação, sem binários, ZIP ou alterações de dados locais; marco concluído em `16a0c49`. Assinatura, instalação e distribuição não fizeram parte dessa autorização. A resposta posterior e o reteste estão registrados na seção anterior.

### Histórico da retentativa e etapas restantes

Atualização em **2026-09-13 às 07:52 (America/Cuiaba)**: após o usuário informar que retirou a classificação de falso positivo, foi feita **uma retentativa do mesmo verificador**, conferindo antes seu SHA-256 `44929CBE1677B7EED6F8AA6A6D00AFF7894500099215AECEF2B42668E688184E`. O Windows novamente recusou o início por vírus/software possivelmente indesejado. Na conferência posterior, `guard-check.exe` já não existia no disco; não foi restaurado nem recriado. Não houve nova compilação, instalação, início do serviço ou alteração da proteção.

O Defender informou `Not running`, sem detecção correspondente; não houve evento correspondente de Code Integrity. `McAfee Framework Host` e `McAfee WebAdvisor` estavam em execução. Naquele momento isso apontava para McAfee como hipótese, **sem confirmar o produto ou a ameaça responsável**. A captura solicitada foi recebida posteriormente e identificou o arquivo e o horário, mas não mostrou o nome específico da ameaça. Instalador e regras originais mantiveram os hashes registrados acima. Recibo local: `artifacts/installer-antivirus-review-20260913-075215/retry.json`.

1. Com autorização, validar o detector separado com processo fictício presente (esperado 2) e depois encerrado (esperado 0), sem abrir painel/serviço ou instalador. O cenário inicial sem processos já passou; preservar a amostra original e todos os recibos anteriores.
2. Com autorização específica, conferir o detector e a instalação inicial, cancelamento/UAC, atalho sem elevação, salvamento sem escrever em Program Files e ausência de autostart/alterações de rede.
3. Conferir recusa com painel na bandeja/serviço aberto, reinstalação, upgrade/downgrade, falha de cópia e desinstalação preservando regras/preferências/arquivos desconhecidos. Iniciar roteamento durante QA requer autorização própria.
4. Assinatura, auditoria de distribuição e release continuam pendentes. O commit/push anterior foi concluído em `16a0c49`; qualquer nova publicação exige autorização própria.

VPN permanece uma evolução posterior: primeira compatibilidade a investigar é Check Point, com modelo genérico por capacidade do adaptador. Não há suporte universal nem proteção contra vazamentos implementados. Respeitar split tunnel/Hub Mode definido pelo administrador; não iniciar testes de VPN, ativar IPv6 ou alterar políticas corporativas nesta etapa.
