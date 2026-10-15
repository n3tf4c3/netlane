# NetLane portátil

O NetLane observa as conexões de rede deste computador, captura e analisa
pacotes e, se você quiser, discute a análise com o Codex. Esta é a versão
portátil: não precisa de instalação nem de permissão de administrador.

## Requisitos

- Windows 10 ou 11, x64 (Intel/AMD).
- Captura de pacotes: [Npcap](https://npcap.com) já instalado. O NetLane não
  instala o driver; sem ele, só a captura fica indisponível.
- Codex (opcional): Codex CLI 0.162 instalado (`npm i -g @openai/codex@0.162.1`)
  e uma conta ChatGPT.

## Como abrir

Extraia o zip numa pasta e abra `netlane.exe`. Fechar a janela (X) leva o
NetLane para a bandeja, perto do relógio. Para encerrar, use **Sair**, o ícone
⏻ na barra inferior, ou o menu do ícone na bandeja.

Para conhecer a interface sem tocar na rede, abra `netlane.exe --demo`: os dados
são fictícios e nada é gravado no seu perfil.

## O que a versão portátil faz

- **Visão geral e Conexões**: aplicativos com conexões abertas, interfaces,
  tráfego por interface e qualidade da conexão (ping ou site).
- **Pacotes**: captura ao vivo em memória, captura longa gravada em disco
  (de 15 min a 72 h) e abertura de arquivos PCAP/PCAPNG.
- **Regras**: cadastro de regras por aplicativo. Com o botão direito numa
  conexão, você cria a regra daquele aplicativo.
- **Codex**: painel à direita para perguntas sobre a rede ou sobre os pacotes
  que você anexar.

Rotear um aplicativo por uma interface e trocar a conexão padrão exigem a
versão instalada. No portátil, as regras ficam salvas, mas não são aplicadas,
e a conexão padrão só é exibida. "Abrir com o Windows" também é exclusivo da
versão instalada.

## Seus dados

Configurações, regras e capturas longas ficam em `%LOCALAPPDATA%\NetLane-Rust`.
Para usar outra pasta: `netlane.exe --profile C:\MinhaPasta`. A captura ao vivo
fica só na memória até você clicar em **Salvar captura**.

O NetLane não envia nada ao Codex sem a sua ação:

- Uma pergunta sem anexo leva só o texto que você digitou.
- Os pacotes anexados vão depois que você revisa o contexto.
- **Acompanhar** (ícone de olho) é opcional e pede confirmação no primeiro uso.
  Ligado, ele envia a cada 30 s os agregados da captura e os metadados dos
  8 pacotes mais recentes (endereços, portas, protocolos), sem conteúdo.

O Codex usa um perfil próprio do NetLane, sem acesso a comandos ou arquivos
deste computador. A conversa só é gravada se você salvar o caderno.

## Conferir o pacote

- `netlane.exe` → Propriedades → Assinaturas digitais: assinado por
  **Paulo Engels Palma Gomes**, com carimbo de tempo.
- O SHA-256 do zip está no arquivo `.sha256` que o acompanha:
  `Get-FileHash .\NetLane-<versão>-portatil-win-x64.zip`.
- `build-receipt.json` registra a versão, o commit e o SHA-256 do executável.

Licenças de terceiros: `THIRD_PARTY.md` e a pasta `licenses`.
