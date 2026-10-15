# Recursos de terceiros da interface Rust

O painel usa GPUI, `gpui_platform` e `gpui_windows` da revisão
`decbf641b18f1982b3475c037e7c5c554471574f` do Zed, a mesma do termIAX.
As dependências efetivas estão em `Cargo.lock`; o patch de `async-task` também
segue a revisão fixada no termIAX. O código da aplicação Zed e suas crates de UI
e temas não foram incorporados ao NetLane.

| Recurso | Origem / licença | Aviso incluído |
| --- | --- | --- |
| GPUI | Zed Industries, Apache-2.0 | `assets/licenses/GPUI-APACHE.txt` |
| Entrada de texto | adaptação do componente termIAX e exemplo GPUI, Apache-2.0 | cabeçalho do componente e licença GPUI |
| One Dark / One Light | temas de origem Atom, MIT | `assets/themes/one/LICENSE` |
| One Dark Pro | Binaryify, MIT | `assets/themes/one-dark-pro/LICENSE` |
| Ayu | Ike Ku, MIT | `assets/themes/ayu/LICENSE` |
| Gruvbox | tema de origem morhetz/gruvbox, MIT | `assets/themes/gruvbox/LICENSE` |
| Material Symbols e Material Icons (`play_arrow`, `save`, `folder_open`, `code`, `send`, `remove`, `crop_square`, `filter_none`, `auto_awesome`, `visibility`) | Google, Apache-2.0; `menu.svg` é desenho próprio | `assets/icons/LICENSE` |
| JetBrains Mono | JetBrains, SIL OFL 1.1 | `assets/fonts/jetbrains-mono/OFL.txt` |
| native-tls 0.2.18 | rust-native-tls, MIT ou Apache-2.0; MIT escolhida | `assets/licenses/native-tls-MIT.txt` |
| schannel 0.1.29 | steffengy, MIT; TLS nativo Windows | `assets/licenses/schannel-MIT.txt` |
| httparse 1.10.1 | Sean McArthur, MIT ou Apache-2.0; MIT escolhida | `assets/licenses/httparse-MIT.txt` |

Os arquivos de cores, ícones e fonte vieram da referência local termIAX com os
avisos preservados. `NetLane Dark` adapta os mesmos tokens de aparência; o
ícone da marca vem do próprio NetLane. A coleção `assets/themes/LICENSES`
mantém os avisos dos temas. Estes avisos acompanham a prévia local; a revisão
completa das licenças das dependências transitivas permanece um gate antes de
distribuição pública.

O builder do instalador Rust coleta também textos/avisos das crates transitivas
restauradas e suas bibliotecas nativas, em `licenses/crates`, com origem,
expressão de licença e hashes em `licenses/cargo-licenses.json`. As dependências
sem texto são listadas em `Issues`; o inventário não inventa licenças nem aprova
distribuição. Consulte [empacotamento Rust](docs/instalador-rust.md#licenças).
