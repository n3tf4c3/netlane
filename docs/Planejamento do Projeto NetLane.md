# NetLane
### Cada app, sua rota.

## 1. Visão do produto

O **NetLane** será uma aplicação para Windows que permitirá ao usuário visualizar os programas que estão utilizando a rede e escolher por qual conexão cada aplicativo deve acessar a Internet.

Exemplo:

| Aplicativo | Saída |
|---|---|
| Chrome | 📶 Wi-Fi |
| Steam | 🔌 Ethernet |
| OneDrive | 🔌 Ethernet |
| Discord | ⚙️ Automático |
| Aplicativo X | 🚫 Bloqueado |

A proposta principal deve permanecer simples:

> **Escolher a conexão de Internet utilizada por cada aplicativo do Windows.**

O NetLane não deve inicialmente tentar substituir firewall, VPN, monitor de rede ou ferramentas avançadas de segurança.

---

# 2. Problema que queremos resolver

Em um computador com mais de uma conexão de rede, como:

- Ethernet;
- Wi-Fi;
- eventualmente outras interfaces;

o Windows normalmente escolhe a rota de saída para todo o sistema.

O usuário não possui uma maneira simples e visual de determinar:

> Chrome usa Wi-Fi, enquanto Steam usa Ethernet.

O NetLane preencherá exatamente essa lacuna.

---

# 3. Objetivo principal

Permitir que o usuário configure:

**Aplicativo → Interface de rede**

Exemplo:

```text
Chrome.exe
      ↓
Wi-Fi

Steam.exe
      ↓
Ethernet
```

Sem modificar o comportamento dos outros aplicativos.

---

# 4. Público-alvo inicial

O produto pode atender principalmente:

- usuários com duas conexões de Internet;
- gamers;
- streamers;
- profissionais trabalhando remotamente;
- usuários de Starlink + fibra;
- notebooks conectados simultaneamente por cabo e Wi-Fi;
- laboratórios e ambientes técnicos;
- usuários que desejam separar determinados tipos de tráfego.

Exemplo bastante interessante:

```text
Ethernet
   ↓
Fibra

Wi-Fi
   ↓
Starlink
```

Com:

```text
Steam       → Fibra
Teams       → Starlink
Chrome      → Starlink
Downloads   → Fibra
```

---

# 5. Princípios do produto

O NetLane deve seguir quatro princípios.

## Simplicidade

O usuário não precisa entender:

- rotas;
- gateway;
- métricas;
- TCP;
- UDP;
- interfaces;
- WFP.

Ele apenas escolhe:

```text
Chrome → Wi-Fi
```

---

## Segurança

Uma alteração feita em um aplicativo não deve afetar o restante do Windows.

---

## Reversibilidade

Deve existir sempre:

```text
⚙️ Automático
```

Essa opção devolve o controle da conexão ao Windows.

---

## Visibilidade

O usuário deve conseguir entender rapidamente:

- quais conexões estão disponíveis;
- quais aplicativos estão usando rede;
- qual conexão cada aplicativo está utilizando.

---

# 6. Tela principal

A tela principal pode ser dividida em duas áreas.

## Interfaces

```text
CONEXÕES

🔌 Ethernet
192.168.1.25
● Conectado

📶 Wi-Fi
192.168.50.20
● Conectado
```

Podemos futuramente mostrar:

```text
Ethernet
↓ 430 Mbps
↑ 62 Mbps

Wi-Fi
↓ 190 Mbps
↑ 38 Mbps
```

Mas isso não precisa fazer parte da primeira versão.

---

# 7. Lista de aplicativos

A área principal mostrará os aplicativos com atividade de rede.

```text
APLICATIVOS

🌐 Google Chrome
chrome.exe
📶 Wi-Fi

🎮 Steam
steam.exe
🔌 Ethernet

💬 Discord
discord.exe
⚙️ Automático
```

Cada aplicativo terá um seletor:

```text
[ Automático ▼ ]
```

Opções:

```text
⚙️ Automático
🔌 Ethernet
📶 Wi-Fi
🚫 Bloquear
```

---

# 8. Regra principal

A regra será vinculada ao executável.

Exemplo:

```text
C:\Program Files\Google\Chrome\Application\chrome.exe
```

e não ao PID.

Isso é importante porque o PID muda cada vez que o programa é executado.

Exemplo de regra interna:

```text
Application
Google Chrome

Executable
C:\Program Files\Google\Chrome\Application\chrome.exe

Network
Wi-Fi

Fallback
Automatic
```

---

# 9. Estados possíveis

Cada programa poderá estar em um destes estados.

## Automático

```text
Chrome
⚙️ Automático
```

O Windows decide a rota normalmente.

---

## Ethernet

```text
Chrome
🔌 Ethernet
```

Novas conexões do programa devem utilizar a interface Ethernet escolhida.

---

## Wi-Fi

```text
Chrome
📶 Wi-Fi
```

Novas conexões do programa devem utilizar a interface Wi-Fi escolhida.

---

## Bloqueado

```text
Chrome
🚫 Bloqueado
```

O aplicativo não poderá acessar a rede.

Essa função pode ficar para depois do primeiro MVP caso aumente demais a complexidade.

---

# 10. Arquitetura inicial

A aplicação será dividida em componentes.

```text
                 NetLane
                    │
       ┌────────────┴─────────────┐
       │                          │
   NetLane UI              NetLane Service
       │                          │
       │                    Network Engine
       │                          │
       └────────── IPC ───────────┘
                                  │
                          Windows Networking
                                  │
                         ┌────────┴────────┐
                         │                 │
                     Ethernet           Wi-Fi
```

---

# 11. NetLane UI

Responsável pela interface gráfica.

Sugestão:

**C# + .NET**

Podemos avaliar:

- WinUI 3;
- WPF.

Para um aplicativo desktop simples, rápido e estável, WPF pode ser uma excelente opção inicial.

Responsabilidades:

- mostrar interfaces;
- mostrar aplicativos;
- mostrar ícones;
- receber escolhas;
- exibir status;
- configurar regras;
- abrir configurações.

A UI não deverá manipular diretamente componentes sensíveis da rede.

---

# 12. NetLane Service

Será um serviço do Windows executado em segundo plano.

Exemplo:

```text
NetLaneService
```

Responsabilidades:

- receber regras da interface;
- detectar processos;
- monitorar interfaces;
- aplicar regras;
- persistir configurações;
- conversar com o mecanismo de rede.

Isso permite separar:

```text
Interface gráfica
```

de:

```text
funções que precisam de privilégios administrativos.
```

---

# 13. Network Engine

Será o componente mais importante do projeto.

Responsabilidades:

```text
Processo
   ↓
identificar regra
   ↓
interface escolhida
   ↓
controlar conexão
```

Exemplo:

```text
chrome.exe
      ↓
regra encontrada
      ↓
Wi-Fi
      ↓
nova conexão
      ↓
interface Wi-Fi
```

---

# 14. Estratégia técnica

O projeto deve estudar principalmente tecnologias nativas do Windows, especialmente:

### Windows Filtering Platform — WFP

Ela poderá ajudar a identificar e controlar conexões associadas aos aplicativos.

Entretanto, antes de construir todo o produto, precisamos validar tecnicamente uma questão crítica:

> Conseguimos encaminhar de forma confiável todas as novas conexões de determinado executável para uma interface específica sem alterar a rota global do Windows?

Por isso haverá uma fase específica de **Proof of Concept — PoC**.

---

# 15. PoC obrigatório

Antes de criar uma interface bonita, devemos provar que o motor funciona.

Criar uma aplicação simples:

```text
netlane-poc.exe
```

ou ferramenta de linha de comando.

Objetivo:

Com:

```text
Ethernet conectado
Wi-Fi conectado
```

executar:

```text
Chrome → Wi-Fi
```

e:

```text
curl.exe → Ethernet
```

Depois validar o IP público das duas conexões.

Exemplo ideal:

```text
Chrome
IP público: 177.xxx.xxx.xxx

curl
IP público: 200.xxx.xxx.xxx
```

Se isso funcionar de maneira confiável, seguimos para o produto.

---

# 16. Testes da PoC

Precisamos testar:

### TCP

```text
HTTP
HTTPS
```

### UDP

Especialmente aplicações modernas.

### QUIC / HTTP3

Muito importante porque navegadores como Chrome utilizam QUIC.

### IPv4

Obrigatório.

### IPv6

Pode entrar depois, mas deve estar previsto na arquitetura.

---

# 17. Aplicativos com múltiplos processos

Precisamos considerar aplicativos como:

```text
Chrome
├ chrome.exe
├ chrome.exe
├ chrome.exe
├ chrome.exe
└ chrome.exe
```

A regra deverá identificar o executável e aplicar-se a todas as instâncias relacionadas.

---

# 18. Aplicativos especiais

Em fases posteriores devemos testar:

- Microsoft Store;
- UWP;
- jogos;
- Steam;
- Battle.net;
- Electron;
- Discord;
- Teams;
- browsers;
- launchers;
- VPNs.

---

# 19. Persistência

Inicialmente poderemos utilizar:

```text
SQLite
```

Banco extremamente simples.

Estrutura aproximada:

## Applications

```text
id
name
executablePath
iconPath
lastSeen
```

## NetworkRules

```text
id
applicationId
interfaceId
mode
fallback
enabled
```

## Interfaces

```text
id
adapterGuid
name
type
lastIp
```

---

# 20. Identificação das interfaces

Não devemos salvar apenas:

```text
Wi-Fi
```

Porque o usuário pode possuir várias interfaces.

Internamente devemos utilizar o identificador do adaptador do Windows.

Visualmente:

```text
📶 Wi-Fi
Intel Wi-Fi 7 BE200
```

ou:

```text
🔌 Ethernet
Intel I225-V
```

---

# 21. Fallback

Uma funcionalidade extremamente importante.

Exemplo:

```text
Chrome
Principal: Wi-Fi
Fallback: Ethernet
```

Se o Wi-Fi cair:

```text
Wi-Fi
   X
   ↓
Ethernet
```

Outra opção:

```text
Wi-Fi
   X
   ↓
Bloquear
```

O usuário decidirá o comportamento.

Essa funcionalidade pode entrar após o MVP.

---

# 22. Perfis

Depois que o motor principal estiver funcionando, podemos implementar perfis.

Exemplo:

## Gaming

```text
Steam        → Ethernet
Battle.net   → Ethernet
Discord      → Wi-Fi
Chrome       → Wi-Fi
```

## Trabalho

```text
Teams        → Ethernet
Outlook      → Ethernet
Chrome       → Wi-Fi
OneDrive     → Ethernet
```

## Padrão

```text
Tudo → Automático
```

---

# 23. Bandeja do sistema

O NetLane não precisa ficar aberto.

Ele poderá funcionar na bandeja do Windows.

```text
        ▲
     🌐 NetLane
```

Clique:

```text
Ethernet   ●
Wi-Fi      ●

Apps com regras: 4
```

Menu:

```text
Abrir NetLane
Pausar regras
Perfil
Sair
```

---

# 24. Pausar regras

Uma função que considero importante desde cedo:

```text
⏸ Pausar NetLane
```

Isso deve temporariamente retornar todos os aplicativos para:

```text
Automático
```

sem apagar as regras.

Depois:

```text
▶ Retomar NetLane
```

As configurações anteriores voltam.

---

# 25. Segurança contra perda de Internet

Devemos prever um mecanismo de emergência.

Se o serviço do NetLane falhar:

```text
NetLane Service
      X
```

o computador deverá voltar ao funcionamento normal.

Nunca queremos:

```text
falha NetLane
      ↓
usuário sem Internet
```

Portanto o comportamento padrão deverá ser:

> **Fail Open**

Se o NetLane não estiver funcionando, o Windows assume novamente as decisões de roteamento.

---

# 26. Dashboard futuro

Depois do produto principal estar pronto:

```text
Hoje

Ethernet
42.3 GB

Wi-Fi
18.7 GB
```

E por aplicativo:

```text
Steam
24 GB

Chrome
11 GB

OneDrive
9 GB
```

Mas isso é secundário.

---

# 27. Funcionalidades que NÃO entram inicialmente

Para evitar perder o foco:

Não implementar inicialmente:

- VPN;
- firewall completo;
- controle parental;
- DNS customizado;
- captura de pacotes;
- análise profunda de protocolos;
- IDS;
- antivírus;
- QoS complexo;
- limite de banda;
- controle de portas;
- proxy configurável.

Podemos estudar alguns futuramente, mas o NetLane precisa primeiro resolver muito bem:

> **App → Conexão.**

---

# 28. Fases do projeto

## Fase 0 — Viabilidade técnica

Criar PoC.

Objetivo:

```text
app A → Ethernet
app B → Wi-Fi
```

Comprovar que o tráfego realmente sai pelas interfaces diferentes.

---

## Fase 1 — Monitor

Criar aplicação capaz de:

- detectar adaptadores;
- detectar Ethernet;
- detectar Wi-Fi;
- mostrar IP;
- mostrar status;
- detectar aplicativos com conexões;
- associar conexão ao processo;
- mostrar ícone;
- mostrar caminho do executável.

Nenhuma alteração de tráfego ainda.

---

## Fase 2 — Routing Engine

Implementar:

```text
Automático
Ethernet
Wi-Fi
```

Salvar regras.

Aplicar regras em novas conexões.

---

## Fase 3 — Interface final

Criar a experiência visual completa:

```text
App → Interface
```

com:

- ícones;
- status;
- filtros;
- pesquisa;
- bandeja do Windows.

---

## Fase 4 — Resiliência

Implementar:

- reconexão;
- interface indisponível;
- fallback;
- restauração automática;
- fail open;
- logs.

---

## Fase 5 — Recursos avançados

Implementar:

- bloqueio;
- perfis;
- estatísticas;
- consumo por aplicativo;
- notificações.

---

# 29. MVP

Considerarei MVP quando tivermos:

✅ detectar Ethernet e Wi-Fi

✅ listar aplicativos utilizando a rede

✅ identificar executável

✅ mostrar ícone

✅ selecionar:

```text
Automático
Ethernet
Wi-Fi
```

✅ salvar configuração

✅ aplicar configuração novamente quando o aplicativo for aberto

✅ retornar ao modo automático

✅ iniciar com o Windows

✅ funcionar minimizado

✅ possuir Fail Open

Com isso já teremos um produto utilizável.

---

# 30. Experiência ideal do usuário

Instala o NetLane.

Abre.

Enxerga:

```text
🔌 Ethernet     Online
📶 Wi-Fi        Online
```

Abaixo:

```text
Chrome        Automático
Steam         Automático
Discord       Automático
OneDrive      Automático
```

Clica:

```text
Chrome
```

Seleciona:

```text
📶 Wi-Fi
```

Pronto.

Não precisa configurar IP.

Não precisa configurar gateway.

Não precisa criar rotas.

Não precisa utilizar PowerShell.

Essa deve ser a essência do produto.

---

# 31. Frase que define o escopo

Sempre que surgir uma nova ideia para o NetLane devemos perguntar:

> **Isso ajuda o usuário a decidir por qual conexão um aplicativo deve acessar a Internet?**

Se a resposta for não, provavelmente não pertence ao núcleo do produto.

---

# 32. Primeira meta técnica

A primeira entrega do projeto não deverá ser a interface.

Será:

```text
NETLANE PoC

Ethernet: conectado
Wi-Fi: conectado

chrome.exe → Wi-Fi
curl.exe   → Ethernet
```

E provar por IP público que:

```text
chrome.exe
      ↓
Wi-Fi
      ↓
Internet B

curl.exe
      ↓
Ethernet
      ↓
Internet A
```

Depois dessa validação, iniciamos o desenvolvimento do NetLane propriamente dito.

---

# 33. Nome provisório

**Produto:** NetLane

**Slogan:**

> **Cada app, sua rota.**

Alternativa em inglês:

> **Your apps. Your routes.**

Nome interno do projeto:

```text
netlane
```

Possível estrutura inicial:

```text
netlane/
│
├── src/
│   ├── NetLane.UI/
│   ├── NetLane.Service/
│   ├── NetLane.Core/
│   └── NetLane.Network/
│
├── tests/
│
├── poc/
│   └── NetLane.NetworkPoC/
│
├── docs/
│
└── README.md
```