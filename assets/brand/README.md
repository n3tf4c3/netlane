# Identidade do NetLane

Origem: `../netlane-assets.zip`, fornecido pelo usuário em 10/10/2026.
SHA-256: `632F23C824FCC72621DC58F640474DAC48D2D87BA3313FCC2E87A7843D0D6C0E`.
Os quatro SVGs e `identity-source.md` são cópias integrais das entradas do ZIP.

| Recurso | Uso |
| --- | --- |
| `logo-light.svg` / `.png` | Marca em fundo claro |
| `logo-dark.svg` / `.png` | Marca em fundo escuro |
| `icon-app.svg` / `.png` | Ícone com fundo da versão Rust |
| `icon-transparent.svg` / `.png` | Símbolo com transparência |
| `netlane-diagnostics.ico` | Ícone dos executáveis e bandeja Rust |
| `netlane.ico` | Recurso anterior da referência .NET, preservado |

Os PNGs são derivados dos SVGs: ícones em 256 × 256 e logos em 900 × 280.
O ICO contém quadros PNG de 16, 20, 24, 32, 40, 48, 64, 128 e 256 pixels.
Textos dos SVGs usam as fontes do sistema, com Segoe UI como padrão de renderização.

Para regenerar, compile o workspace Release e execute da raiz:

```powershell
& .\scripts\build-brand-assets.ps1
```

O script compila `tools/render-brand.rs` com o `resvg` já usado pelo GPUI no
lockfile. Não baixa ferramentas nem modifica os SVGs. A saída padrão substitui
somente os quatro PNGs e `netlane-diagnostics.ico`; para comparar uma nova
renderização, use `-OutputDirectory artifacts/brand-comparison`.
