# RustShot

Aplicação standalone de captura de tela para **Windows 11 (x64)**, escrita em Rust. Um único `rustshot.exe`, sem instalador e sem runtime externo: roda em segundo plano na bandeja do sistema e oferece três modos de captura por atalhos globais configuráveis.

> Implementação da [Especificação Técnica v1.0](#especificação) (RustShot v1.1).

## Funcionalidades

| Modo | Atalho padrão | Comportamento |
|---|---|---|
| **Tela cheia** | `Ctrl+PrtScr` | Captura e salva automaticamente na pasta configurada |
| **Região** | `Shift+PrtScr` | Congela a tela, você arrasta um retângulo; ao soltar, salva e copia para a área de transferência |
| **Região + edição** | `Ctrl+Shift+PrtScr` | Como acima, mas abre o editor de anotações |

- **Editor de anotações**: linha, seta, retângulo, elipse e texto; 8 cores + seletor livre, espessura 1–12 px, fonte 12–72 px; `Ctrl+Z`/`Ctrl+Y` desfaz/refaz; `Shift` restringe a forma (45°/quadrado/círculo); `Ctrl+scroll` dá zoom (25–400%) e o botão do meio faz pan; `Ctrl+C` copia a imagem anotada para a área de transferência (o editor continua aberto); `Ctrl+S` salva e fecha; `Esc` descarta (confirmando se houver anotações).
- **Multi-monitor e DPI alto**: capturas em pixels físicos, manifesto **Per-Monitor V2** embutido, suporte a escalas mistas (100–300%) e coordenadas negativas. Escopo da tela cheia configurável: todos os monitores compostos, apenas o principal, ou o monitor sob o cursor.
- **Bandeja do sistema**: a aplicação não tem janela principal — menu com os três modos, abrir pasta de capturas, configurações, "Iniciar com o Windows" e Sair.
- **Configuração persistente** em `config.json` (leitura tolerante; arquivo corrompido é renomeado para `.bak` e recriado). Alterações têm efeito imediato, sem reiniciar.
- **Instância única**: uma segunda instância notifica e encerra.
- **Visual Windows 11 (Fluent)**: tema claro/escuro seguindo o sistema, cor de destaque do Windows, cards e cantos arredondados, fonte **Segoe UI Variable** na interface — o texto das anotações permanece na Inter embutida, garantindo que o preview do editor seja idêntico ao JPG exportado.
- Saída sempre em JPG (qualidade 90); nomes `screenshot_2026-08-02_14-30-05.jpg` com sufixos `_1`, `_2`… em caso de colisão; toast de confirmação a cada captura salva.

## Build

Toolchain: `stable-x86_64-pc-windows-msvc` (Rust 1.81+).

```powershell
cargo build --release
# artefato único:
target\release\rustshot.exe
```

Distribuição portátil: copie o exe. Todo o estado fica no mesmo diretório que o executável (`config.json` + `rustshot.log`); desinstalar = apagar o exe e esses arquivos.

> Um exe não assinado pode disparar o SmartScreen na primeira execução em outras
> máquinas; assinatura de código (OV/EV) é opcional e recomendada para distribuição ampla.

Em hosts não-Windows é possível validar tipos e lints sem linkar:
`cargo check --target x86_64-pc-windows-msvc` (o script de build só embute os
recursos Win32 quando o alvo é Windows).

> **Smart App Control**: em máquinas com SAC em modo de imposição, artefatos
> intermediários recém-compilados (build scripts, proc-macros) podem ser
> bloqueados por reputação de hash (`os error 4551` ou E0463 "can't find
> crate") — e o veredito por hash é permanente, então repetir o build não
> resolve. O `.cargo/config.toml` deste repo fixa um `-Cmetadata`; se um
> bloqueio aparecer, troque o rótulo (ex.: `sacfresh3`) para re-rolar os
> hashes de todos os artefatos e compile de novo. Não desative o SAC.

## Configuração (`config.json`)

Local padrão: `config.json` — criado no mesmo diretório que o executável com padrões na primeira execução.

```json
{
  "version": 1,
  "output_dir": "C:\\Users\\voce\\Pictures\\RustShot",
  "filename_template": "screenshot_{date}_{time}",
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

- `output_dir` vazio ⇒ padrão `Imagens\RustShot` (respeita redirecionamento do OneDrive). Se a pasta configurada não puder ser criada, a aplicação usa o padrão e notifica.
- Formato de saída: **JPG (qualidade 90), sempre** — não há campo de formato; um `image_format` legado (v1.0) no arquivo é ignorado.
- `fullscreen_scope`: `"all_monitors"` | `"primary"` | `"monitor_under_cursor"`.
- `modifiers`/`code` usam os nomes dos tipos do crate `global-hotkey` (`CTRL`, `SHIFT`, `ALT`, `WIN` e códigos W3C como `PrintScreen`, `KeyA`, `F5`…).
- Campos ausentes assumem o padrão; a janela de Configurações (menu da bandeja) edita tudo isso com validação e aviso de conflito entre os três atalhos.

`PrtScr` **sem modificador** não é usado como padrão: desde o Windows 11 22H2 a tecla
abre a Ferramenta de Captura nativa, e `Win+Shift+S` é reservado pelo sistema.

## Arquitetura (resumo)

Processo único, event loop único (`eframe`/`egui` com o viewport principal de 1×1 px fora da área visível — visível para o SO, imperceptível para o usuário);
overlay de seleção (uma janela por monitor, sempre no topo, com a captura congelada e
véu escuro) e editor abrem como viewports adicionais sob demanda. Atalhos globais
(`global-hotkey`) e menu da bandeja (`tray-icon`) acordam a UI via fila de eventos +
`request_repaint`. Codificação e escrita de JPG rodam em threads de trabalho.
Exportação do editor rasteriza com `tiny-skia` + `ab_glyph` usando a **mesma fonte
embutida** (Inter) do preview — o que você vê é o que sai no JPG.

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
| `theme.rs` | tema Fluent (Win11): paleta claro/escuro, cor de destaque, fontes, widgets |
| `clipboard.rs` / `storage.rs` / `tray.rs` / `notify.rs` | cópia, salvamento, bandeja, toasts |

## Dependências

Diretas (versões completas travadas no `Cargo.lock`):

| Crate | Versão | Papel |
|---|---|---|
| [eframe](https://crates.io/crates/eframe) / [egui](https://crates.io/crates/egui) | 0.32 | GUI: event loop, overlay de seleção, editor e configurações |
| [global-hotkey](https://crates.io/crates/global-hotkey) | 0.7 | Atalhos de teclado globais (RF-01…RF-03) |
| [tray-icon](https://crates.io/crates/tray-icon) | 0.21 | Ícone e menu da bandeja do sistema (RF-06) |
| [xcap](https://crates.io/crates/xcap) | 0.6 | Enumeração de monitores e captura de tela (pixels físicos) |
| [arboard](https://crates.io/crates/arboard) | 3.5 | Área de transferência de imagem (`Ctrl+C` no editor) |
| [image](https://crates.io/crates/image) | 0.25 | Codificação JPG, crop e composição |
| [tiny-skia](https://crates.io/crates/tiny-skia) | 0.11 | Rasterização vetorial das anotações na exportação |
| [ab_glyph](https://crates.io/crates/ab_glyph) | 0.2 | Rasterização da fonte Inter na exportação de texto |
| [serde](https://crates.io/crates/serde) / [serde_json](https://crates.io/crates/serde_json) | 1 | Leitura/gravação do `config.json` |
| [chrono](https://crates.io/crates/chrono) | 0.4 | Data/hora no nome dos arquivos (RF-07) |
| [dirs](https://crates.io/crates/dirs) | 6 | Resolução da pasta Imagens |
| [rfd](https://crates.io/crates/rfd) | 0.15 | Diálogo nativo de seleção de pasta (configurações) |
| [notify-rust](https://crates.io/crates/notify-rust) | 4 | Toasts de notificação do Windows |
| [auto-launch](https://crates.io/crates/auto-launch) | 0.5 | "Iniciar com o Windows" (`HKCU\...\Run`) |
| [single-instance](https://crates.io/crates/single-instance) | 0.3 | Garantia de instância única via mutex (RF-08) |
| [anyhow](https://crates.io/crates/anyhow) | 1 | Propagação de erros com contexto |
| [log](https://crates.io/crates/log) / [simplelog](https://crates.io/crates/simplelog) | 0.4 / 0.12 | Log em `rustshot.log` no mesmo diretório do executável |
| [windows-sys](https://crates.io/crates/windows-sys) | 0.59 | APIs Win32 (somente alvo Windows) |
| [embed-resource](https://crates.io/crates/embed-resource) | 3 | *(build)* Embute ícone e manifesto Per-Monitor V2 no exe |

> **Nota sobre alertas de segurança em dependências transitivas**: o `Cargo.lock`
> registra dependências de todas as plataformas. A pilha GTK3 (`gtk`/`glib`/
> `libappindicator`, via `tray-icon`) é usada apenas no Linux e **não é compilada
> no binário Windows** — alertas do Dependabot sobre esses crates (ex.: `glib`
> < 0.20) não afetam o `rustshot.exe` e podem ser dispensados como
> "vulnerable code is not actually used".

## Decisões de implementação

- **Widget de atalho**: a tecla `PrtScr` não chega como evento de teclado do egui, então o "clique e pressione a combinação" é complementado por seletores explícitos de modificadores + tecla (único caminho para `PrintScreen`, que já vem nos padrões).
- **Módulo extra** `settings.rs` para a janela de configurações.
- **v1.1**: saída fixa em JPG (qualidade 90) e todo o estado ao lado do exe — a v1.0 usava PNG por padrão e `%APPDATA%\RustShot`. O retângulo preto que aparecia no canto do monitor era o viewport-raiz "oculto" do eframe: uma janela realmente invisível não recebe `WM_PAINT` e mataria os atalhos, então a solução é o oposto — a janela fica **visível para o SO**, mas com 1×1 px, fora da área da tela (-32000,-32000), sem ativação, sem redimensionar/maximizar, fora do Alt-Tab (`WS_EX_TOOLWINDOW`) e imune a Alt+F4.


## Limitações conhecidas (v1)

- Telas protegidas (prompt de UAC, tela de login, janelas com DRM) não são capturáveis — limitação do Windows.
- Atalhos globais não disparam sobre janelas elevadas se o RustShot não estiver elevado.
- A seleção de região fica contida no monitor onde o arrasto começou.
- Formas não são editáveis/movíveis após criadas (undo/redo apenas).
- Toasts aparecem com origem "Windows PowerShell" enquanto o exe não tem AUMID registrado (comportamento padrão de apps não instalados).
- "Iniciar com o Windows" grava o caminho absoluto do exe em `HKCU\...\Run`: mover ou renomear a pasta quebra o autostart até o app ser aberto manualmente (ele então corrige o registro); apagar a pasta com o recurso ativo deixa uma entrada órfã (inofensiva) — desative antes de "desinstalar".
- Interface em pt-BR (multi-idioma no roadmap).

## Desenvolvimento

```bash
cargo test  # testes de unidade (lógica pura)
cargo clippy --tests --target x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc
```

O CI (GitHub Actions) compila em `windows-latest`, roda clippy + testes e publica o `rustshot.exe` como artefato.

Relatório de testes da v1.1 (método e resultados, incluindo auditoria de
segurança): [docs/relatorio-de-testes-v1.1.md](docs/relatorio-de-testes-v1.1.md).

## Licença

Código sob [BSD 3-Clause](LICENSE). A fonte embutida
[Inter](https://github.com/rsms/inter) é distribuída sob a
[SIL Open Font License 1.1](assets/Inter-LICENSE.txt).

## Especificação

Este repositório implementa a "RustShot — Especificação Técnica v1.0" (02/08/2026):
três modos de captura (RF-01…RF-03), editor com 5 ferramentas (RF-04), configurações com efeito imediato (RF-05), bandeja (RF-06), nomeação/salvamento com colisões (RF-07) e instância única (RF-08); requisitos não funcionais RNF-01…RNF-08 (exe único ≤ 15 MB, DPI Per-Monitor V2, event-driven, sem privilégios de administrador, sem console).

---

https://github.com/mBarony/printscreen-windows11-rust/raw/main/dist/rustshot.exe
