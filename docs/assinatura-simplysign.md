# Revisão e assinatura local com SimplySign

**Pacote Rust (2026-10-11):** assinado com o certificado
`0C061E39B11C1A9092043F143F0CAA685BC94F62` depois do QA em laboratório; executáveis,
plug-ins NSIS, desinstalador e instalador com timestamp Certum, aprovados no
[Sandbox com Smart App Control](qa-sandbox-rust-signed-2026-10-11.md). O build .NET
abaixo continua com a ordem e as pendências registradas; ao assinar o .NET, assinar
também os plug-ins NSIS embutidos, pelo mesmo motivo.

**Ordem vigente em 2026-10-08:** concluir as [pendências e ensaios do NetLane](pendencias-pre-assinatura.md) antes de retomar a assinatura. A prévia atual para QA é `pre-sign-20261008-035-v2`, sem assinatura. A tentativa e os artefatos descritos abaixo permanecem evidência histórica; não repetir a tentativa enquanto os gates anteriores estiverem abertos.

Procedimento preparado em 8 de outubro de 2026 para o NetLane Windows x64. A assinatura usa o certificado de code signing disponibilizado no repositório pessoal do Windows pelo SimplySign Desktop; não exporta a chave privada. Login, token ou PIN, quando solicitados, são informados diretamente no aplicativo do fornecedor.

## Revisão anterior à assinatura

A revisão desta rodada cobriu dependências NuGet, IPC/UAC, persistência, inicialização e empacotamento. O serviço dependia transitivamente de `System.Text.Json 8.0.0`, apontado pelo NuGet para CVE-2024-30105 e CVE-2024-43485. Foi acrescentada referência explícita a `8.0.6`, a versão estável mais recente da linha 8 disponível no feed consultado. Os avisos oficiais descrevem as correções em [8.0.4](https://github.com/dotnet/runtime/security/advisories/GHSA-hh2w-p6rv-4g7w) e [8.0.5](https://github.com/dotnet/runtime/security/advisories/GHSA-8g4q-xg66-9fp4). A presença da dependência afetada é confirmada; exploração em produção não foi demonstrada.

Validação local: Release sem avisos/erros, 283 testes da solução, 75 testes QUIC e 37 testes do host de bandeja aprovados, total 395. A consulta posterior ao NuGet não apontou dependências vulneráveis na solução. Quatorze scripts PowerShell passaram pela análise sintática. Nenhum roteiro real de roteamento, troca de métricas, qualidade de rede, instalação ou reinicialização foi iniciado.

O protocolo limita mensagens a 64 KiB, verifica o PID dos pares e restringe ACLs; a inicialização só altera a entrada esperada no perfil do usuário; persistência usa controle de revisão e gravação substituída. Esses controles foram inspecionados e cobertos pelas suítes existentes. Essa revisão não é prova de todos os cenários de produção.

## Ordem de assinatura

1. Publicar UI e serviço em uma pasta nova de `artifacts/installer/`, com self-contained x64.
2. Recusar dados do usuário, regras, preferências de qualidade/conexão, logs, probes e hosts de revisão no payload.
3. Assinar os oito binários próprios: UI/serviço EXE e DLL e duas cópias de Core/Network DLL. Bibliotecas de terceiros conservam o conteúdo original.
4. Verificar certificado, SHA-256, timestamp RFC 3161 e retorno do SignTool. Só então calcular hashes/inventário do payload.
5. Assinar o desinstalador com `!uninstfinalize` durante a compilação NSIS, sem executar o instalador ou o desinstalador.
6. Assinar o instalador completo; gerar hash e recibo após a assinatura.
7. Extrair estaticamente e conferir inventário, dados ausentes, arquitetura, privilégios, ícone e assinaturas/timestamps dos dez binários próprios, incluindo o desinstalador extraído.

O build recusa saída existente e certificado implícito. A assinatura não é ativada quando `CertificateThumbprint` é omitido. Falha de assinatura, timestamp ou verificação interrompe a entrega; não produz liberação presumida.

## Comandos

Pré-requisitos: compilador NSIS 3.12 conferido, SignTool disponível, certificado de code signing válido com chave privada no `CurrentUser/My` e SimplySign conectado. Não há instalação automática de ferramentas.

```powershell
.\scripts\build-windows-installer.ps1 `
  -NsisCompilerPath '<caminho absoluto do makensis.exe conferido>' `
  -Version 0.3.5 `
  -CertificateThumbprint '<40 dígitos hexadecimais do certificado>' `
  -SignToolPath '<caminho absoluto do signtool.exe>'

.\scripts\test-windows-installer.ps1 `
  -BuildDirectory '<pasta nova indicada pelo build>' `
  -SevenZipPath 'C:\Program Files\7-Zip\7z.exe'
```

O helper [sign-windows-artifacts.ps1](../scripts/sign-windows-artifacts.ps1) recebe arquivos explícitos, exige executáveis PE locais ainda sem assinatura e confirma o certificado e timestamp após assinar. O empacotador [build-windows-installer.ps1](../scripts/build-windows-installer.ps1) usa esse helper para o payload, desinstalador e instalador.

Referências: [manual oficial Certum para assinatura em nuvem](https://files.certum.eu/documents/manual_en/CS-Code_Signing_in_the_Cloud_Signtool_jarsigner_signing.pdf), [comando NSIS !uninstfinalize](https://nsis.sourceforge.io/Docs/Chapter5.html#uninstfinalize).

## Limites da entrega

O pacote permanece uma prévia local. Assinatura comprova identidade/integridade, sem concluir instalação, guardas runtime, logon, downgrade ou rollback. Os cenários pendentes continuam em [instalador-local.md](instalador-local.md); assinatura não implica ausência de alertas de SmartScreen/antivírus. Dados e recibos antigos são preservados. O auxiliar historicamente bloqueado pelo antivírus permanece fora da compilação e execução.

Os resultados desta rodada ficam em `artifacts/revisao-assinatura-20261008/`; hashes de arquivos protegidos são conferidos antes/depois. Nenhum commit, push ou distribuição é efetuado pelo procedimento.

## Estado da rodada de 8 de outubro

A prévia de revisão foi gerada em `artifacts/installer/review-20261008-035/NetLane-0.3.5-preview-win-x64-setup.exe`, SHA-256 `D99396118020FF18D0F3B1F12F31CBBB1283879A023BEF820FFF4C8E367DA561`. A extração estática passou em 757 arquivos de payload, confirmou x64, privilégios, ícone e ausência de dados locais. Instalador, programas e desinstalador não foram executados.

**Assinatura ainda pendente.** A tentativa separada em `signed-20261008-035/` interrompeu antes da geração do instalador: SignTool retornou 1, `No certificates were found that met all the given criteria`. O certificado está presente em `CurrentUser/My`, válido de 08/10/2026 a 08/10/2027, com chave privada declarada e cadeia validada online. Isso não demonstra disponibilidade da chave no provedor no momento da assinatura. Os executáveis publicados continuaram `NotSigned`; não há pacote assinado liberado.

O estado de conexão/login/PIN no SimplySign foi solicitado ao usuário. Após confirmar esse acesso, diagnosticar a seleção com SignTool se necessário e repetir o build em uma pasta nova; verificar as dez assinaturas e a extração antes de declarar a entrega assinada. A tentativa anterior e seus logs são preservados.
