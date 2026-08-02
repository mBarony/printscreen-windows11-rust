# RustShot

Aplicação standalone de captura de tela para **Windows 11 (x64)**, escrita em Rust.
Um único `rustshot.exe`, sem instalador e sem runtime externo: roda em segundo plano
na bandeja do sistema e oferece três modos de captura por atalhos globais configuráveis.

> Implementação da [Especificação Técnica v1.0](#especificação) (RustShot v1.0).

## Funcionalidades

| Modo | Atalho padrão | Comportamento |
|---|---|---|
| **Tela cheia** | `Ctrl+PrtScr` | Captura e salva automaticamente na pasta configurada |
| **Região** | `Shift+PrtScr` | Congela a tela, você arrasta um retângulo; ao soltar, salva |
| **Região + edição** | `Ctrl+Shift+PrtScr` | Como acima, mas abre o editor de anotações |

- **Editor de anotações**: linha, seta, retângulo, elipse e texto; 8 cores + seletor livre,
  espessura 1–12 px, fonte 12–72 px; `Ctrl+Z`/`Ctrl+Y` desfaz/refaz; `Shift` restringe a forma
  (45°/quadrado/círculo); `Ctrl+scroll` dá zoom (25–400%) e o botão do meio faz pan;
  `Ctrl+C` copia a imagem anotada para a área de transferência (o editor continua aberto);
  `Ctrl+S` salva e fecha; `Esc` descarta (confirmando se houver anotações).
- **Multi-monitor e DPI alto**: capturas em pixels físicos, manifesto **Per-Monitor V2**
  embutido, suporte a escalas mistas (100–300%) e coordenadas negativas.
  Escopo da tela cheia configurável: todos os monitores compostos, apenas o principal,
  ou o monitor sob o cursor.
- **Bandeja do sistema**: a aplicação não tem janela principal — menu com os três modos,
  abrir pasta de capturas, configurações, "Iniciar com o Windows" e Sair.
- **Configuração persistente** em `config.json` (leitura tolerante; arquivo corrompido é
  renomeado para `.bak` e recriado). Alterações têm efeito imediato, sem reiniciar.
- **Instância única**: uma segunda instância notifica e encerra.
- PNG (RGBA 8 bits) por padrão; nomes `screenshot_2026-08-02_14-30-05.png` com sufixos
  `_1`, `_2`… em caso de colisão; toast de confirmação a cada captura salva.

## Build

Toolchain: `stable-x86_64-pc-windows-msvc` (Rust 1.81+).

```powershell
cargo build --release
# artefato único:
target\release\rustshot.exe
```

Distribuição portátil: copie o exe. Todo o estado fica em `%APPDATA%\RustShot\`
(`config.json` + `rustshot.log`); desinstalar = apagar o exe e essa pasta.

> Um exe não assinado pode disparar o SmartScreen na primeira execução em outras
> máquinas; assinatura de código (OV/EV) é opcional e recomendada para distribuição ampla.

Em hosts não-Windows é possível validar tipos e lints sem linkar:
`cargo check --target x86_64-pc-windows-msvc` (o script de build só embute os
recursos Win32 quando o alvo é Windows).

## Configuração (`config.json`)

Local padrão: `%APPDATA%\RustShot\config.json` — criado com padrões na primeira execução.
**Modo portátil**: se existir um `config.json` ao lado do `rustshot.exe`, ele tem
precedência (crie um arquivo vazio ao lado do exe para optar por esse modo).

```json
{
  "version": 1,
  "output_dir": "C:\\Users\\voce\\Pictures\\RustShot",
  "filename_template": "screenshot_{date}_{time}",
  "image_format": "png",
  "fullscreen_scope": "all_monitors",
  "hotkeys": {
    "fullscreen": { "modifiers": ["CTRL"], "code": "PrintScreen" },
    "region":     { "modifiers": ["SHIFT"], "code": "PrintScreen" },
    "edit":       { "modifiers": ["CTRL", "SHIFT"], "code": "PrintScreen" }
  },
  "editor": {
    "default_color": "#FF3B30",
    "default_stroke_width": 3,
    "default_font_size": 24
  },
  "start_with_windows": false
}
```

- `output_dir` vazio ⇒ padrão `Imagens\RustShot` (respeita redirecionamento do OneDrive).
  Se a pasta configurada não puder ser criada, a aplicação usa o padrão e notifica.
- `image_format`: `"png"` (padrão) ou `"jpg"` (qualidade 90).
- `fullscreen_scope`: `"all_monitors"` | `"primary"` | `"monitor_under_cursor"`.
- `modifiers`/`code` usam os nomes dos tipos do crate `global-hotkey`
  (`CTRL`, `SHIFT`, `ALT`, `WIN` e códigos W3C como `PrintScreen`, `KeyA`, `F5`…).
- Campos ausentes assumem o padrão; a janela de Configurações (menu da bandeja)
  edita tudo isso com validação e aviso de conflito entre os três atalhos.

`PrtScr` **sem modificador** não é usado como padrão: desde o Windows 11 22H2 a tecla
abre a Ferramenta de Captura nativa, e `Win+Shift+S` é reservado pelo sistema.

## Arquitetura (resumo)

Processo único, event loop único (`eframe`/`egui` com o viewport principal oculto);
overlay de seleção (uma janela por monitor, sempre no topo, com a captura congelada e
véu escuro) e editor abrem como viewports adicionais sob demanda. Atalhos globais
(`global-hotkey`) e menu da bandeja (`tray-icon`) acordam a UI via fila de eventos +
`request_repaint`. Codificação e escrita de PNG rodam em threads de trabalho.
Exportação do editor rasteriza com `tiny-skia` + `ab_glyph` usando a **mesma fonte
embutida** (Inter) do preview — o que você vê é o que sai no PNG.

| Módulo | Responsabilidade |
|---|---|
| `main.rs` | bootstrap, instância única, logging, eframe |
| `app.rs` | estado global, máquina de estados `Idle → Selecting → Editing` |
| `config.rs` | load/save/validação do `config.json` |
| `hotkeys.rs` | registro/re-registro dos atalhos globais |
| `capture.rs` | enumeração de monitores (`xcap`), captura, composição, crop |
| `overlay.rs` | viewports de seleção de região |
| `editor/` | `ui.rs` (janela), `shapes.rs` (modelo + undo), `render.rs` (exportação) |
| `settings.rs` | janela de configurações (RF-05) |
| `clipboard.rs` / `storage.rs` / `tray.rs` / `notify.rs` | cópia, salvamento, bandeja, toasts |

## Decisões de implementação

Pontos em que a especificação era ambígua ou citava versões indisponíveis:

- **PNG vs JPG**: RF-01 menciona "salva um JPG", mas RF-07 define PNG como formato de
  arquivo e o `config.json` de referência traz `"image_format": "png"`. Implementado
  **PNG como padrão**, com `"jpg"` aceito na configuração (qualidade 90). JPG com
  qualidade configurável segue no roadmap (v1.2).
- **Local do config**: RF-05 fala em "ao lado do executável" e §13 em `%APPDATA%\RustShot`.
  Implementado `%APPDATA%` como padrão + **modo portátil opt-in** (config ao lado do exe
  tem precedência quando existir).
- **Versões de crates**: `single-instance` é `0.3` (não existe `1.x`) e `simplelog` é
  `0.12` (não existe `12`); demais versões conforme a especificação.
- **Widget de atalho**: a tecla `PrtScr` não chega como evento de teclado do egui, então
  o "clique e pressione a combinação" é complementado por seletores explícitos de
  modificadores + tecla (único caminho para `PrintScreen`, que já vem nos padrões).
- **Módulo extra** `settings.rs` para a janela de configurações (a tabela de módulos da
  especificação não previa um arquivo para a UI de RF-05).

## Limitações conhecidas (v1)

- Telas protegidas (prompt de UAC, tela de login, janelas com DRM) não são capturáveis —
  limitação do Windows.
- Atalhos globais não disparam sobre janelas elevadas se o RustShot não estiver elevado.
- A seleção de região fica contida no monitor onde o arrasto começou.
- Formas não são editáveis/movíveis após criadas (undo/redo apenas).
- Toasts aparecem com origem "Windows PowerShell" enquanto o exe não tem AUMID
  registrado (comportamento padrão de apps não instalados).
- Interface em pt-BR (multi-idioma no roadmap).

## Desenvolvimento

```bash
cargo test                                        # testes de unidade (lógica pura)
cargo clippy --tests --target x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc
```

O CI (GitHub Actions) compila em `windows-latest`, roda clippy + testes e publica o
`rustshot.exe` como artefato.

## Licença

Código sob [BSD 3-Clause](LICENSE). A fonte embutida
[Inter](https://github.com/rsms/inter) é distribuída sob a
[SIL Open Font License 1.1](assets/Inter-LICENSE.txt).

## Especificação

Este repositório implementa a "RustShot — Especificação Técnica v1.0" (02/08/2026):
três modos de captura (RF-01…RF-03), editor com 5 ferramentas (RF-04), configurações
com efeito imediato (RF-05), bandeja (RF-06), nomeação/salvamento com colisões (RF-07)
e instância única (RF-08); requisitos não funcionais RNF-01…RNF-08 (exe único ≤ 15 MB,
DPI Per-Monitor V2, event-driven, sem privilégios de administrador, sem console).
