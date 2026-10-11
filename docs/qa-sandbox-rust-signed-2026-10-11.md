# QA Rust no Windows Sandbox — pacote assinado — 2026-10-11

Rodada do instalador Rust por máquina **assinado** (SimplySign/Certum) no Windows
Sandbox com **Smart App Control aplicando** (`VerifiedAndReputablePolicyState=1`),
o mesmo cenário que [bloqueou o pacote sem assinatura](qa-sandbox-rust-permachine-2026-10-11.md).
**Resultado: todos os casos aprovados, sem bloqueio de Integridade de Código.**
Nenhuma proteção foi alterada; o Sandbox foi encerrado e descartado.

## Assinatura

- Certificado `CN=Paulo Engels Palma Gomes` (Certum Code Signing 2021 CA),
  thumbprint `0C061E39B11C1A9092043F143F0CAA685BC94F62`, válido até 08/10/2027,
  chave no SimplySign Desktop; SHA-256 com timestamp RFC 3161 `time.certum.pl`.
- `scripts/build-rust-installer.ps1 -CertificateThumbprint … -SignToolPath …`
  assina: `netlane.exe` e `netlane-agent.exe` (antes dos hashes do manifesto), os
  plug-ins NSIS `System.dll` e `nsDialogs.dll`, o desinstalador (`!uninstfinalize`)
  e o instalador. `test-rust-installer.ps1` conferiu as seis assinaturas, inclusive
  dos plug-ins e do desinstalador extraídos.
- Pacote: `artifacts/rust-installer-signed-20261011-111847`, instalador SHA-256
  `28B4961EB2DB1574BFF0E1EF98329D4425C3CA77F3AE8866103541C3A2D32443`, commit
  `b75544d`; 710 arquivos; oito recusas estáticas aprovadas.

## Primeira tentativa: plug-in NSIS sem assinatura

O primeiro pacote assinado (`rust-installer-signed-20261011-110841`, sem os
plug-ins assinados) **foi executado** pelo Smart App Control, mas o `System.dll`
extraído em `%TEMP%` foi bloqueado (eventos 3077/3033/3118). Sem o plug-in,
`${IsNativeAMD64}` falhou e o instalador saiu com código 5 sem instalar nada
(`artifacts/rust-sandbox-signed-20261011-111048`). A correção assina os plug-ins
embutidos; o verificador estático passou a exigi-los assinados.

## Resultados

Evidências: `artifacts/rust-sandbox-signed-20261011-112041` (Sandbox
`d2a74ebd-6efa-4c2b-9496-7139b2d9f0f9`, Windows 11 Enterprise 26100).

| Caso | Resultado |
| --- | --- |
| Preflight | coletado; Smart App Control = 1 |
| Runtime VC++ (só no Sandbox) | 14.51.36247 instalado, retorno 0 |
| Prévia per-user existente | recusa com código 4, nada instalado |
| Sem administrador (token restrito + `RunAsInvoker`) | recusa com código 9 pela guarda do instalador |
| Instalação limpa | retorno 0; nome no registro correto; pasta e agente sem escrita para usuários comuns; nenhum evento de Integridade de Código |
| Smoke `netlane-agent --validate` | aprovado |
| Reinstalação | 710 arquivos + manifesto íntegros; fixtures preservadas |
| Remoção | 711 arquivos conhecidos removidos; registro e atalho removidos; fixtures preservadas |
| Diagnóstico final | zero eventos 3033/3077/3118 |

Não aprova SmartScreen em download real, distribuição pública, logon/autorun,
upgrade entre versões nem a demonstração visual da UI.
