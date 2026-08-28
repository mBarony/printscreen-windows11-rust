# RustShot

Aplicação standalone de captura de tela para **Windows 11 (x64)**, escrita em Rust. Um único `rustshot.exe`, sem instalador e sem runtime externo: roda em segundo plano na bandeja do sistema e oferece quatro atalhos globais configuráveis: três de captura e um de reconhecimento de texto.

> Implementação da [Especificação Técnica v1.0](#especificação) (RustShot v1.8).

## Funcionalidades

| Modo | Atalho padrão | Comportamento |
|---|---|---|
| **Tela cheia** | `Ctrl+PrtScr` | Captura e salva automaticamente na pasta configurada |
| **Região** | `Shift+PrtScr` | Congela a tela, você arrasta um retângulo (com `Shift` ele sai quadrado) e ele vai direto para a área de transferência, pronto para colar (`Esc` cancela). `Space` alterna para escolher uma **janela** inteira, `Ctrl+A` pega o monitor todo |
| **Região + edição** | `Ctrl+Shift+PrtScr` | Como acima, mas abre o editor de anotações |
| **Reconhecer texto** | `Ctrl+Alt+PrtScr` | Arraste sobre um texto na tela e ele vai para a área de transferência, com as quebras de linha. Nenhuma janela se abre; um aviso no alto da tela mostra o começo do que foi copiado e permite recopiá-lo emendado numa linha só |

- **Editor de anotações**: linha (`L`), seta (`S`), retângulo (`R`), elipse (`E`), mão livre (`F`), marca-texto (`H`), numerador (`N`), conta-gotas (`I`, com `Shift` para a cor do texto e arrasto para a média de uma área), **ocultar** (`D`), **holofote** (`O`), **remover faixa** (`X`), texto (`T`) e **régua** (`U`, mede a distância em pixels da imagem, com uma ponta em cada extremidade e o valor no meio); **Recortar** (`C`) mantém apenas a área arrastada (confirme com `Enter`), levando as anotações junto e podendo ser desfeita; 8 cores + seletor livre, espessura 1–12 px, **traço sólido, tracejado ou pontilhado**, com variante **desenhada à mão**, fonte 12–72 px (`Ctrl+scroll` ajusta a espessura — ou a fonte, com Texto ativo); retângulo e elipse podem sair preenchidos, e o retângulo aceita cantos arredondados; o texto é multilinha (`Ctrl+Enter` confirma) e pode ganhar uma pílula clara de fundo para continuar legível sobre qualquer imagem; as teclas das ferramentas e o papel da roda (traço×zoom) são configuráveis na janela de Configurações; `Alt+H`/`Alt+V` criam guias de alinhamento e `Alt+Shift+G` as limpa; `Ctrl+Shift+C` copia as anotações selecionadas e `Ctrl+V` cola — anotações copiadas do RustShot, ou uma imagem da área de transferência como camada movível; `Ctrl+Z`/`Ctrl+Y` desfaz/refaz; `Shift` restringe a forma (45°/quadrado/círculo) e `Alt` a desenha a partir do centro; a roda do mouse dá zoom (25–400%) e o botão do meio faz pan; `Ctrl+C` copia a imagem anotada para a área de transferência e fecha o editor; `Ctrl+S` salva como arquivo e fecha; `Ctrl+P` **fixa a imagem na tela** como janela sempre no topo; `Esc` descarta (confirmando se houver edições).
- **Esconder, destacar, encurtar**: **ocultar** (`D`) apaga uma região de vez — em mosaico sintético (o padrão) ou cor chapada; o **holofote** (`O`) escurece o resto da imagem e amplia o que ficou dentro; **remover faixa** (`X`) joga fora uma tira da imagem e junta o que sobrou, levando as anotações junto. Um botão da toolbar põe a captura sobre um **fundo decorativo** com sombra, em quatro variações.
- **Anotações continuam editáveis**: com a ferramenta **Selecionar** (`M`), clique numa anotação para selecioná-la — no contorno, ou no miolo quando não há nada mais sob o cursor — ou arraste a partir de um ponto vazio para **laçar várias**, que passam a se mover e a ser apagadas em bloco — a imagem só é mesclada ao salvar/copiar. Arraste o corpo para reposicionar ou as **alças** para redimensionar (linha, régua e seta têm as duas pontas, e a seta ainda uma alça central que a **dobra num arco**; `Shift` preserva a proporção ou prende em 45°). As setas do teclado empurram 1 px, ou 10 px com `Shift`; `Alt+D` duplica e `Alt`+arrasto duplica já movendo; `Alt+R` inverte a ponta de uma seta; segurar `Espaço` enquanto desenha reposiciona a forma em vez de esticá-la; `Delete` apaga, e `Ctrl+A` seleciona todas de uma vez. Trocar a cor, a espessura ou o tamanho da fonte com algo selecionado **repinta a anotação** em vez de valer só para a próxima. Se o editor fechar sem querer, o menu da bandeja oferece **recuperar a edição não salva** — com o histórico de desfazer intacto.
- **Fixar na tela** (`Ctrl+P` no editor): a captura vira uma janelinha sem bordas, sempre no topo, que fica até você fechá-la com `Esc` — útil para consultar algo enquanto trabalha noutra janela. Arraste pelo corpo para mover; a roda redimensiona.
- **Multi-monitor e DPI alto**: capturas em pixels físicos, manifesto **Per-Monitor V2** embutido, suporte a escalas mistas (100–300%) e coordenadas negativas. Escopo da tela cheia configurável: todos os monitores compostos, apenas o principal, ou o monitor sob o cursor.
- **Bandeja do sistema**: a aplicação não tem janela principal — menu com os três modos, captura com 3 s de atraso, repetir a última região, abrir pasta de capturas, configurações, "Iniciar com o Windows" e Sair.
- **Destino da captura configurável**: o que fazer depois de capturar sem passar pelo editor — salvar em arquivo, copiar para a área de transferência, ou os dois —, escolhido separadamente para a tela cheia (padrão: salvar) e para a região (padrão: copiar).
- **Configuração persistente** em `config.json` (leitura tolerante; arquivo corrompido é renomeado para `.bak` e recriado). Alterações têm efeito imediato, sem reiniciar.
- **Instância única**: uma segunda instância notifica e encerra.
- **Visual Windows 11 (Fluent)**: tema claro/escuro seguindo o sistema, cor de destaque do Windows, cards e cantos arredondados, fonte **Segoe UI Variable** na interface — o texto das anotações permanece na Inter embutida, garantindo que o preview do editor seja idêntico ao arquivo exportado.
- Saída em **PNG ou JPG (qualidade 90), escolhido por imagem**: PNG para texto e interface, onde as bordas têm de sair nítidas; JPG para conteúdo fotográfico, onde ele poupa espaço sem perda visível. Dá para fixar um dos dois em Configurações. Nomes `screenshot_2026-08-02_14-30-05.png` com sufixos `_1`, `_2`… em caso de colisão; notificação de confirmação (balão da bandeja, com o nome e o ícone do RustShot) a cada captura salva.

## Linha de comando

O uso normal é pela bandeja e pelos atalhos globais. Para scripts e atalhos
externos, o executável também aceita:

```powershell
rustshot                          # inicia na bandeja
rustshot foto.png                 # abre a imagem no editor (idem --file)
rustshot --clipboard              # abre a imagem da área de transferência
rustshot --capture-fullscreen     # captura e salva, sem abrir janela
rustshot --capture-fullscreen --copy --save
rustshot --help                   # ajuda completa
rustshot --version
```

Códigos de saída: `0` sucesso, `1` falha ao capturar ou abrir a imagem, `2`
erro de uso. Região e janela não têm modo "sem janela": as duas exigem uma
seleção na tela.

### Reconhecimento de texto

Usa o motor do próprio Windows (`Windows.Media.Ocr`), o mesmo da Ferramenta de Captura: num Windows 11 não há nada a instalar. São três portas de entrada, e as duas primeiras copiam o texto para a área de transferência com as quebras de linha, mostrando um aviso no alto da tela para recopiá-lo emendado numa linha só:

- **`Ctrl+Alt+PrtScr`** — arraste sobre um texto na tela; nenhuma janela se abre.
- **Botão na barra do editor** — reconhece o texto da imagem que está aberta.
- **`rustshot --ocr captura.png`** — lê um arquivo e mostra o texto numa caixa de mensagem, sem copiar. É o modo de linha de comando, anterior aos outros dois.

O aviso some assim que você cola (`Ctrl+V`), em qualquer aplicativo — colar é o fim natural da tarefa. Sem isso, some sozinho em 8 segundos, e o relógio para enquanto o cursor estiver sobre ele.

Se o idioma do seu perfil não tiver pacote de OCR instalado, o motor cai no primeiro pacote disponível em vez de falhar — um Windows em pt-BR com apenas o pacote en-US funciona. Para instalar outro idioma: Configurações › Hora e idioma › Idioma.

O reconhecimento entra nas builds oficiais. Continua atrás da feature de compilação `ocr`, ligada por padrão, porque é o único ponto do programa que usa a crate `windows` (o resto fala Win32 por `windows-sys`) e porque assim dá para medir o custo dela no executável a qualquer momento — hoje 17.920 bytes:

```powershell
cargo build --release                       # com OCR (padrão)
cargo build --release --no-default-features # sem OCR, para comparar
```

O estudo de viabilidade, com o custo medido no binário e a comparação com o PowerOCR do PowerToys, está em [`docs/ocr-viabilidade.md`](docs/ocr-viabilidade.md).

## Build

Toolchain: `stable-x86_64-pc-windows-msvc` (Rust 1.88+, pelo `as_chunks` das conversões de pixel).

**Script de build** (recomendado) — roda as mesmas verificações do CI, compila
e relata o artefato:

```powershell
.\build.ps1                                            # clippy + testes + release
.\build.ps1 -SkipChecks -Run                           # iteração rápida: compila e abre
.\build.ps1 -InstallTo "$env:LOCALAPPDATA\RustShot" -Run   # compila, instala e executa
.\build.ps1 -NewSalt                                   # quando o Smart App Control bloquear (ver abaixo)
```

Outras opções: `-Dev` (perfil debug), `-Clean`, `-Verbose`. `Get-Help .\build.ps1 -Full`
mostra a ajuda completa. Se o PowerShell recusar o script por política de
execução, use `powershell -ExecutionPolicy Bypass -File .\build.ps1`.

Direto pelo cargo, sem o script:

```powershell
cargo build --release
# artefato único:
target\release\rustshot.exe
```

Distribuição portátil: copie o exe. Todo o estado fica no mesmo diretório que o executável (`config.json` + `rustshot.log`); desinstalar = apagar o exe e esses arquivos.

**Publicar uma versão**: atualize a versão em `Cargo.toml` e em
`assets/rustshot.rc`, descreva a mudança no `CHANGELOG.md` e crie a tag —
`git tag v1.3.1 && git push origin v1.3.1`. O workflow *Release* compila no
`windows-latest`, valida que tag/`Cargo.toml`/VersionInfo do exe coincidem e
publica o `rustshot.exe` + `SHA256SUMS.txt` como assets, com as notas
extraídas do CHANGELOG. Também dá para disparar pela aba *Actions* →
*Release* → *Run workflow*, informando a tag (útil quando não se quer criar
a tag localmente).

O executável publicado é assinado pelo workflow (ver [Assinatura digital](#assinatura-digital));
a chave privada vive apenas como segredo do repositório, nunca no código.

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
- `modifiers`/`code` usam `CTRL`/`SHIFT`/`ALT`/`WIN` e os nomes de tecla do padrão W3C (`PrintScreen`, `KeyA`, `F5`…) — mesmo formato desde a v1.0.
- Campos ausentes assumem o padrão; a janela de Configurações (menu da bandeja) edita tudo isso com validação e aviso de conflito entre os três atalhos.

`PrtScr` **sem modificador** não é usado como padrão: desde o Windows 11 22H2 a tecla
abre a Ferramenta de Captura nativa, e `Win+Shift+S` é reservado pelo sistema.

## Arquitetura (resumo)

**Um executável, dois modos.** O que fica de pé a sessão inteira é o *residente*
(`resident.rs`): bandeja, atalhos globais (`WM_HOTKEY`) e captura de tela cheia, tudo em
Win32 puro — sem `eframe`, sem `wgpu`, sem device D3D12. Ele é um loop de mensagens
`GetMessage`/`DispatchMessage` sobre a janela de shell própria (`platform/shell.rs`), e
processa os eventos **fora** do `WndProc`, por fila.

Quando um fluxo precisa de janela, o residente lança `rustshot.exe --gui …`, que sobe o
eframe, cumpre a tarefa e encerra — devolvendo ao SO tudo que a GPU e o driver custavam.
A captura acontece no **residente**, antes de qualquer janela existir, para a tela ficar
congelada no instante do atalho; os pixels chegam ao filho por memória compartilhada
(`platform/ipc.rs`). No sentido inverso, o filho pede balões e avisa que regravou o
`config.json` por `WM_COPYDATA` (`resident_link.rs`) — quem é dono dos atalhos e do
registro do Windows é sempre o residente.

Dentro do processo de GUI, o desenho antigo continua: viewport-raiz de 1×1 px fora da área
visível, overlay de seleção como uma janela por monitor (sempre no topo, captura congelada
e véu escuro) e o editor como viewport adicional. Codificação e escrita de JPG rodam em
threads de trabalho registradas em `jobs.rs`, que o `main` espera antes de deixar o
processo morrer. Exportação do editor rasteriza com o rasterizador próprio
(`editor/raster.rs`) + `ab_glyph` usando a **mesma fonte embutida** (Inter) do preview — o
que você vê é o que sai no JPG.

| Módulo | Responsabilidade |
|---|---|
| `main.rs` | bootstrap, escolha do modo (`--gui`), instância única, logging |
| `resident.rs` | processo residente: bandeja, atalhos, tela cheia, lançamento dos filhos |
| `app.rs` | processo de GUI: máquina de estados `Selecting → Editing → fim` |
| `resident_link.rs` / `jobs.rs` | canal do filho para o residente; trabalhos que não podem morrer com o processo |
| `config.rs` | load/save/validação do `config.json` |
| `hotkeys.rs` | registro/re-registro dos atalhos globais |
| `capture.rs` | enumeração de monitores e captura (GDI), composição, crop |
| `overlay.rs` | viewports de seleção de região |
| `editor/` | `ui/` (janela, toolbar, canvas, interação), `shapes.rs` (modelo das anotações), `document.rs` (log de operações + desfazer), `render.rs` e `raster/` (exportação), `redact.rs`, `spotlight.rs`, `cut.rs`, `backdrop.rs` (ferramentas que mexem em pixels), `session_file.rs` (edição gravada em disco) |
| `settings.rs` | janela de configurações (RF-05) |
| `theme.rs` | tema Fluent (Win11): paleta claro/escuro, cor de destaque, fontes, widgets |
| `clipboard.rs` / `storage.rs` / `tray.rs` / `notify.rs` | cópia, salvamento, bandeja, notificações |
| `platform/` | camada Win32 própria (shell/bandeja/atalhos, captura GDI, lista de janelas via DWM, leitura de imagem via GDI+, clipboard, registro, diálogos…) |
| `jpeg/` / `imgbuf.rs` / `json.rs` / `error.rs` | codificador JPEG incorporado, buffer de imagem, JSON e erro próprios |

## Dependências

A v1.3 tornou o código **standalone**: fora o núcleo de GUI, tudo é código
próprio chamando Win32 diretamente. Dependências diretas:

| Crate | Versão | Papel |
|---|---|---|
| [eframe](https://crates.io/crates/eframe) / [egui](https://crates.io/crates/egui) | 0.32 | Núcleo de GUI: event loop, janelas, overlay, editor, configurações |
| [wgpu](https://crates.io/crates/wgpu) | 25 (só `dx12`) | Renderização D3D12/DXGI (composição DWM, sem unredirection) |
| [windows-sys](https://crates.io/crates/windows-sys) | 0.59 | Bindings Win32 gerados oficialmente (só declarações `extern`) |
| [ab_glyph](https://crates.io/crates/ab_glyph) | 0.2 | Rasterização da fonte na exportação — **já é dependência interna do egui/epaint**; usá-la não adiciona crate novo |
| [log](https://crates.io/crates/log) | 0.4 | Fachada de log (usada também pelo eframe/wgpu) |
| [embed-resource](https://crates.io/crates/embed-resource) | 3 | *(build)* Embute ícone e manifesto Per-Monitor V2 no exe |

**Código incorporado/próprio** (substituindo os crates da v1.x, somente o
necessário para o Windows 11):

| Módulo | Substitui | O que faz |
|---|---|---|
| `platform/shell.rs` | tray-icon + muda + notify-rust + global-hotkey | Janela oculta com WndProc: bandeja (`Shell_NotifyIconW`), menu (`TrackPopupMenu`), notificações em balão e atalhos globais (`RegisterHotKey`/`WM_HOTKEY`) |
| `platform/capture.rs` | xcap | `EnumDisplayMonitors` + `BitBlt`/`CAPTUREBLT` em px físicos, DPI por monitor |
| `platform/clipboard.rs` | arboard | Imagem no clipboard via `CF_DIB` |
| `platform/autostart.rs` | auto-launch | `HKCU\...\Run` direto no registro |
| `platform/instance.rs` | single-instance | Mutex nomeado (`CreateMutexW`) |
| `platform/dialog.rs` | rfd | `SHBrowseForFolderW` (estilo novo) |
| `platform/folders.rs` | dirs | `SHGetKnownFolderPath(FOLDERID_Pictures)` |
| `platform/time.rs` | chrono | `GetLocalTime` para os nomes de arquivo |
| `platform/logger.rs` | simplelog | Logger em arquivo com filtro dos módulos gráficos |
| `json.rs` | serde + serde_json | Parser/escritor JSON mínimo do `config.json` |
| `imgbuf.rs` | image (tipo `RgbaImage`) | Buffer RGBA com crop/colagem/conversão RGB |
| `jpeg/` | image (codificador JPEG) | **Incorporado e reduzido do [image-rs](https://github.com/image-rs/image)** (MIT/Apache-2.0), com o FDCT do Independent JPEG Group — avisos de licença preservados nos arquivos |
| `editor/raster.rs` | tiny-skia | Rasterizador anti-aliased (linha/seta/retângulo/elipse) da exportação |
| `error.rs` | anyhow | Erro encadeável mínimo |

O eframe/egui + wgpu **não** foram incorporados de propósito: são a
plataforma de GUI inteira (janelas, entrada, composição, render D3D12) —
incorporá-los seria "incorporar tudo". Em hosts não-Windows os módulos de
plataforma compilam como stubs, só para rodar os testes de lógica.

> **Nota sobre alertas de segurança em dependências transitivas**: o
> `Cargo.lock` registra dependências de todas as plataformas. O que sobra de
> Linux/macOS ali (`objc2`, `wayland-*`…) **não é compilado no binário
> Windows** — alertas do Dependabot sobre esses crates não afetam o
> `rustshot.exe`.

## Decisões de implementação

- **Widget de atalho**: a tecla `PrtScr` não chega como evento de teclado do egui, então o "clique e pressione a combinação" é complementado por seletores explícitos de modificadores + tecla (único caminho para `PrintScreen`, que já vem nos padrões).
- **Módulo extra** `settings.rs` para a janela de configurações.
- **Residente sem GUI**: o consumo de ~90 MB em repouso era o device D3D12 que o viewport-raiz de 1×1 px mantinha de pé a sessão inteira (driver da GPU mapeado, blocos do alocador do wgpu, swapchain). A saída foi tirar o eframe do processo residente: quem espera na bandeja é Win32 puro, e a GUI virou um processo efêmero. Preço a pagar: o overlay de seleção agora aparece com o atraso de subir um processo e criar o device (algo entre 200 e 400 ms) — a **imagem** continua congelada no instante do atalho, porque a captura acontece no residente, mas o retângulo de seleção demora a surgir. A tela cheia não paga nada: nunca abre janela. Encerrar pelo "Sair" com um editor aberto deixa a janela viva de propósito, para não descartar anotações não salvas.
- **Consumo de memória e alvo único**: no processo de GUI valem os ajustes em `main.rs::wgpu_options` (só `Backends::DX12`, `MemoryHints::MemoryUsage` no lugar dos blocos de 256/64 MiB do padrão, `PowerPreference::LowPower` — reversível por `WGPU_POWER_PREF=high` —, uma frame em voo) e o `platform::memory::trim_working_set()` ao voltar para a bandeja, que devolve ao SO as páginas que a captura tocou. Na mesma direção saíram as fontes embutidas do egui (a UI usa Segoe UI Variable com a Inter como reserva; emoji digitado no editor vira tofu, mas ele já não sobrevivia à exportação) e o `accesskit` — sem provedor de UI Automation, **leitores de tela não enxergam a UI**. O alvo é só Windows 11 x64: build para outra arquitetura falha em `compile_error!`, e em sistema anterior à build 22000 o app mostra uma caixa de mensagem e encerra.
- **v1.1**: saída fixa em JPG (qualidade 90) e todo o estado ao lado do exe — a v1.0 usava PNG por padrão e `%APPDATA%\RustShot`. O retângulo preto que aparecia no canto do monitor era o viewport-raiz "oculto" do eframe: uma janela realmente invisível não recebe `WM_PAINT` e mataria os atalhos, então a solução é o oposto — a janela fica **visível para o SO**, mas com 1×1 px, fora da área da tela (-32000,-32000), sem ativação, sem redimensionar/maximizar, fora do Alt-Tab (`WS_EX_TOOLWINDOW`) e imune a Alt+F4.
- **v1.3 (standalone)**: dependências de conveniência substituídas por código próprio chamando Win32 (tabela acima). Efeitos visíveis: as notificações passam de toasts WinRT (que apareciam como "Windows PowerShell") para **balões da bandeja com o nome e o ícone do RustShot**, e o seletor de pasta usa o diálogo clássico do shell. Formatos e comportamento do `config.json` permanecem idênticos.
- **v1.2**: a captura de região deixou de concluir ao soltar o mouse — a seleção ficava **pendente na tela** e o destino era escolhido pelo teclado (`Ctrl+C` copia, `Ctrl+S` salva). No editor, `Ctrl+C` passou a **fechar a janela** depois de copiar, espelhando o `Ctrl+S` — e isso continua valendo. Detalhe de plataforma: `Ctrl+C` chega ao egui como `Event::Copy` (não como tecla), e é assim que o editor o detecta.
- **v1.8.0**: a região voltou a concluir ao soltar o mouse, agora **sempre copiando** para a área de transferência. O passo de escolha cobrava uma tecla a mais de todo mundo para servir a um caso raro: quem captura uma região quase sempre vai colar em seguida, e quem quer arquivo tem a tela cheia e o editor. Com isso saíram o estado pendente do overlay e o destino "salvar" da seleção.


## Limitações conhecidas (v1)

- Telas protegidas (prompt de UAC, tela de login, janelas com DRM) não são capturáveis — limitação do Windows.
- Atalhos globais não disparam sobre janelas elevadas se o RustShot não estiver elevado.
- A seleção de região fica contida no monitor onde o arrasto começou.
- Formas não são editáveis/movíveis após criadas (undo/redo apenas).
- As notificações usam balões da bandeja; com "Não incomodar"/Assistente de foco ativos o Windows pode suprimi-las (as capturas continuam sendo salvas normalmente — o resultado está na pasta e no log).
- "Iniciar com o Windows" grava o caminho absoluto do exe em `HKCU\...\Run`: mover ou renomear a pasta quebra o autostart até o app ser aberto manualmente (ele então corrige o registro); apagar a pasta com o recurso ativo deixa uma entrada órfã (inofensiva) — desative antes de "desinstalar".
- Interface em pt-BR (multi-idioma no roadmap).
- Sem suporte a leitores de tela: o provedor de UI Automation (`accesskit`) foi removido em troca de memória.
- Windows 10 não é suportado: o app se recusa a iniciar em builds anteriores à 22000.
- O overlay de seleção leva algumas centenas de ms para aparecer (o tempo de subir o processo de GUI); a imagem exibida é a do instante do atalho, não a do momento em que a janela surge.
- "Sair" com o editor aberto encerra apenas o residente (bandeja e atalhos): a janela do editor continua até você fechá-la, e as notificações dela passam a ir só para o log.

## Desenvolvimento

```bash
cargo test  # testes de unidade (lógica pura)
cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo check --target x86_64-pc-windows-msvc
```

O CI (GitHub Actions) compila em `windows-latest`, roda `clippy --all-targets -- -D warnings` + testes e publica o `rustshot.exe` como artefato.

Relatório de testes da v1.1 (método e resultados, incluindo auditoria de
segurança): [docs/relatorio-de-testes-v1.1.md](docs/relatorio-de-testes-v1.1.md).

## Licença

Código sob [BSD 3-Clause](LICENSE). A fonte embutida
[Inter](https://github.com/rsms/inter) é distribuída sob a
[SIL Open Font License 1.1](assets/Inter-LICENSE.txt).

## Download

**[⬇ Baixar a versão mais recente](https://github.com/mBarony/printscreen-windows11-rust/releases/latest/download/rustshot.exe)**
— link permanente: aponta sempre para o `rustshot.exe` da última release.

Sem instalador e sem runtime: baixe, coloque em uma pasta gravável e execute.
O `config.json` e o `rustshot.log` são criados ao lado do executável;
desinstalar = apagar esses arquivos.

O binário é compilado pelo CI no `windows-latest` com o toolchain **MSVC**
(o alvo canônico do projeto) e publicado com `SHA256SUMS.txt` para
conferência — veja [todas as releases](https://github.com/mBarony/printscreen-windows11-rust/releases).
Também dá para compilar você mesmo com `.\build.ps1` (seção [Build](#build)).

## Assinatura digital

O `rustshot.exe` das releases é assinado no CI com **Authenticode** (SHA-256 e carimbo
de tempo RFC 3161). Para ver quem assinou:

```powershell
Get-AuthenticodeSignature .\rustshot.exe | Format-List Status, SignerCertificate, TimeStamperCertificate
```

O certificado é **autoassinado**, e isso tem uma consequência que vale dizer sem
rodeios: **o SmartScreen continua aparecendo** na primeira execução ("Mais informações"
→ "Executar assim mesmo"), e o `Status` do comando acima vem como `UnknownError`,
porque a cadeia não termina numa autoridade certificadora reconhecida pelo Windows.
Só um certificado OV/EV de uma CA (ou o Azure Trusted Signing) elimina esse aviso.

O que a assinatura entrega, mesmo assim: **autoria** — o binário comprovadamente saiu
deste projeto — e **detecção de adulteração**, já que qualquer byte alterado invalida
a assinatura. É uma garantia mais forte que o `SHA256SUMS.txt` sozinho, que um
adversário poderia recalcular junto com o arquivo trocado.

Certificado usado (parte pública em [`assets/rustshot-codesign.cer`](assets/rustshot-codesign.cer),
também anexado a cada release):

| | |
|---|---|
| Titular | `CN=Marcio Baroni, O=RustShot, C=BR` |
| Validade | 13/08/2026 – 13/08/2031 |
| SHA-256 | `9019C1A627A5F5B0F10C454A86C0A1800FB4C2C81CFC586E72C382A7AF5F8E82` |

Confira que o arquivo baixado corresponde a esse certificado:

```powershell
(Get-FileHash .\rustshot-codesign.cer -Algorithm SHA256).Hash
Get-PfxCertificate .\rustshot-codesign.cer | Format-List Subject, Thumbprint, NotAfter
```

### Confiar no certificado (opcional)

Instalar o certificado faz o Windows reconhecer o app como assinado e o `Status` virar
`Valid`. **Pense antes de fazer isso:** ao colocar um certificado autoassinado em
*Trusted Root*, você passa a confiar em **tudo** que for assinado com aquela chave —
não é um passo de instalação de rotina, e só faz sentido em máquinas suas, depois de
conferir o SHA-256 acima. Em PowerShell **como administrador**:

```powershell
Import-Certificate -FilePath .\rustshot-codesign.cer -CertStoreLocation Cert:\LocalMachine\Root
Import-Certificate -FilePath .\rustshot-codesign.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher
```

Para desfazer, remova o certificado desses dois repositórios pelo `certlm.msc`.

## Histórico de versões

Ver [CHANGELOG.md](CHANGELOG.md) — da v1.0 (implementação da especificação) à
v1.3 (código standalone, sem dependências além do núcleo de GUI).

## O que vem a seguir

O [backlog](backlog/README.md) tem uma feature por arquivo, separada por
plataforma: o que é portátil em `nucleo/`, e o que depende de API do sistema
em `windows/`, `linux/` (Hyprland sobre Wayland) e `macos/`. O andamento fica
em [controle_backlog.md](backlog/controle_backlog.md), junto das decisões que
ainda não foram tomadas — entre elas o OCR no Linux, que não tem motor de
sistema para usar.

## Especificação

Este repositório implementa a "RustShot — Especificação Técnica v1.0" (02/08/2026):
três modos de captura (RF-01…RF-03), editor com 5 ferramentas (RF-04), configurações com efeito imediato (RF-05), bandeja (RF-06), nomeação/salvamento com colisões (RF-07) e instância única (RF-08); requisitos não funcionais RNF-01…RNF-08 (exe único ≤ 15 MB, DPI Per-Monitor V2, event-driven, sem privilégios de administrador, sem console).
