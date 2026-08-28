//! RustShot — captura de tela para Windows 11 (bootstrap).
//!
//! Um executável, dois modos:
//!
//! * **residente** (sem argumentos) — bandeja, atalhos globais e captura de tela
//!   cheia em Win32 puro. É o que fica de pé a sessão inteira, e por isso não
//!   carrega eframe/wgpu: um device D3D12 aberto 24/7 custava ~90 MB.
//! * **GUI** (`--gui …`) — overlay de seleção, editor ou configurações. Sobe o
//!   eframe, cumpre a tarefa recebida na linha de comando e encerra.
//!
//! Ambos passam por aqui para instância única (RF-08, só o residente), logging
//! em `rustshot.log` (na pasta do executável) e carga do `config.json`.

#![cfg_attr(windows, windows_subsystem = "windows")]

// O alvo é um só: Windows 11 x64. O manifesto embutido declara
// `processorArchitecture="amd64"`, então um build para ARM64 ou 32 bits sairia
// descrito errado — falha aqui, com mensagem, em vez de gerar esse exe.
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("RustShot só suporta Windows x86_64 (o manifesto declara amd64)");

mod app;
mod capture;
mod clipboard;
mod color;
mod config;
mod editor;
mod error;
mod gif;
mod hotkeys;
mod imgbuf;
mod imgout;
mod jobs;
mod jpeg;
mod last_region;
mod json;
mod notify;
mod ocr_layout;
mod ocr_popup;
mod overlay;
mod pinned;
mod platform;
mod resident;
mod resident_link;
mod settings;
mod smartpick;
mod storage;
mod theme;
mod tray;

use config::APP_NAME;

const SINGLE_INSTANCE_NAME: &str = "rustshot-single-instance-mutex";
/// Alterar este valor re-rola o hash do exe quando o SAC bloquear o binário
/// recém-compilado (veredito por hash é permanente; ver README, Build).
#[allow(dead_code)]
const SAC_EXE_SALT: u32 = 1;
/// Acima disso o log é rotacionado para `rustshot.log.old`.
const LOG_ROTATE_BYTES: u64 = 2 * 1024 * 1024;

fn main() {
    // Antes de qualquer coisa: sistema anterior ao Windows 11 não é alvo
    // suportado. Caixa de mensagem (e não toast) porque não há bandeja ainda e
    // o processo encerra em seguida.
    if !platform::version::is_supported() {
        platform::msgbox::info(
            "RustShot requer Windows 11",
            "Este aplicativo tem como alvo o Windows 11 (build 22000) ou superior.",
        );
        return;
    }

    let code = match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Mode::Resident) => {
            run_resident();
            0
        }
        Ok(cli::Mode::Gui(request)) => {
            run_gui(request);
            0
        }
        // Um app de janela não tem console para escrever: a ajuda e a versão
        // saem numa caixa de mensagem, que é onde quem clicou duas vezes no
        // exe vai olhar.
        Ok(cli::Mode::Print(text)) => {
            platform::msgbox::info("RustShot", &text);
            0
        }
        Ok(cli::Mode::EditFile(path)) => run_edit_image(
            platform::imagefile::load(&path),
            &format!("Não foi possível abrir {}", path.display()),
        ),
        Ok(cli::Mode::OcrFile(path)) => run_ocr(&path),
        Ok(cli::Mode::QuickCapture { copy, save }) => run_quick_capture(copy, save),
        Ok(cli::Mode::EditClipboard) => run_edit_image(
            platform::clipboard::get_image(),
            "Não foi possível ler a área de transferência",
        ),
        Err(err) => {
            // Linha de comando malformada só acontece por engano de quem chamou
            // o exe à mão: reportar e sair, sem tocar em bandeja nem em log.
            platform::msgbox::info("RustShot — argumentos inválidos", &err);
            2
        }
    };
    jobs::join_all();
    if code != 0 {
        std::process::exit(code);
    }
}

/// Reconhece o texto de uma imagem do disco e o mostra.
///
/// É o único ponto de entrada do OCR por enquanto. Sem ele o módulo seria
/// inalcançável a partir do `main`, e o LTO o descartaria por inteiro do
/// binário — o que aliás foi medido: sem esta função, ligar a feature `ocr`
/// produzia um exe byte a byte idêntico.
#[cfg(feature = "ocr")]
fn run_ocr(path: &std::path::Path) -> i32 {
    let image = match platform::imagefile::load(path) {
        Ok(image) => image,
        Err(err) => {
            platform::msgbox::info(
                "RustShot — OCR",
                &format!("Não foi possível abrir {}: {err}", path.display()),
            );
            return 1;
        }
    };
    match platform::ocr::recognize(&image, None) {
        Ok(text) => {
            platform::msgbox::info("RustShot — texto reconhecido", &text);
            0
        }
        Err(err) => {
            platform::msgbox::info("RustShot — OCR", &format!("{err}"));
            1
        }
    }
}

#[cfg(not(feature = "ocr"))]
fn run_ocr(_path: &std::path::Path) -> i32 {
    platform::msgbox::info(
        "RustShot — OCR",
        "Esta build foi compilada sem OCR (feature `ocr`).",
    );
    2
}

/// Captura a tela e entrega o resultado sem abrir janela nenhuma.
fn run_quick_capture(copy: bool, save: bool) -> i32 {
    init_logging(false);
    install_panic_hook();

    let config = config::load().config;
    let image = match capture::capture_fullscreen(config.fullscreen_scope) {
        Ok(image) => image,
        Err(err) => {
            platform::msgbox::info("RustShot", &format!("Falha na captura\n\n{err:#}"));
            return 1;
        }
    };

    let mut failed = false;
    if copy {
        if let Err(err) = clipboard::copy_image(&image) {
            log::error!("falha ao copiar: {err:#}");
            failed = true;
        }
    }
    if save {
        match storage::write_image(&storage::SaveTarget::from_config(&config), &image) {
            Ok(path) => log::info!("captura salva em {}", path.display()),
            Err(err) => {
                log::error!("falha ao salvar: {err:#}");
                failed = true;
            }
        }
    }
    if failed {
        1
    } else {
        0
    }
}

/// Abre o editor sobre uma imagem já carregada. Devolve o código de saída.
fn run_edit_image(loaded: error::Result<imgbuf::RgbaImage>, what: &str) -> i32 {
    init_logging(false);
    install_panic_hook();

    let image = match loaded {
        Ok(image) => image,
        Err(err) => {
            platform::msgbox::info("RustShot", &format!("{what}\n\n{err:#}"));
            return 1;
        }
    };

    let config = config::load().config;
    let task = app::Task::EditImage(Box::new(image));
    run_event_loop(config, task);
    0
}

/// Modo residente: instância única, bandeja e loop de mensagens (RF-06/RF-08).
fn run_resident() {
    // RF-08 primeiro: a segunda instância não deve tocar (nem rotacionar) o
    // log que a instância em execução mantém aberto.
    let Some(_instance) = platform::instance::acquire(SINGLE_INSTANCE_NAME) else {
        notify::toast_blocking(
            "RustShot já está em execução",
            "Use o ícone na bandeja do sistema.",
        );
        return;
    };

    init_logging(true);
    install_panic_hook();

    if !config::state_dir_writable() {
        log::warn!("pasta do executável sem permissão de escrita");
        notify::toast_error(
            "Pasta do executável sem permissão de escrita",
            "Configurações e log não serão gravados. Mova o RustShot para uma pasta gravável (ex.: Documentos).",
        );
    }

    let loaded = config::load();
    log::info!(
        "RustShot {} iniciando (residente); config em {}",
        env!("CARGO_PKG_VERSION"),
        config::config_path().display()
    );

    resident::run(loaded);
    log::info!("RustShot encerrado");
}

/// Modo GUI: uma tarefa, uma janela, e o processo morre em seguida.
fn run_gui(request: cli::GuiRequest) {
    // Sem instância única: o residente é que é singleton, e ele pode ter mais de
    // um filho vivo (uma captura e as configurações). Sem rotação de log
    // também — o arquivo é do residente, que o mantém aberto.
    init_logging(false);
    install_panic_hook();
    resident_link::set_resident(request.parent);

    let config = config::load().config;
    let task = match request.task {
        cli::GuiTask::Select { shots, len, purpose } => match load_shots(&shots, len) {
            Ok((shots, windows)) => app::Task::Select { shots, windows, purpose },
            Err(err) => {
                log::error!("capturas não recebidas do residente: {err:#}");
                notify::toast_error("Falha na captura", &format!("{err:#}"));
                return;
            }
        },
        cli::GuiTask::Settings => app::Task::Settings,
        cli::GuiTask::Recover => match editor::session_file::load(&config::state_dir()) {
            Some(doc) => app::Task::Recover(Box::new(doc)),
            None => {
                notify::toast_error(
                    "Nada a recuperar",
                    "A edição gravada não pôde ser lida.",
                );
                return;
            }
        },
    };

    log::info!("RustShot {} iniciando (GUI)", env!("CARGO_PKG_VERSION"));

    // O viewport-raiz precisa ficar VISÍVEL para o SO (janela oculta não recebe
    // WM_PAINT e o `update` nunca roda), mas imperceptível para o usuário: 1×1
    // px, sem decoração, fora da área da tela, sem ativação e sem
    // redimensionar/maximizar. `visible(false)` era a causa do retângulo preto:
    // em máquinas onde a flag não segurava a janela, ela surgia no canto do
    // monitor. O App ainda a remove do Alt-Tab (WS_EX_TOOLWINDOW).
    let options = eframe::NativeOptions {
        // wgpu (D3D12/DXGI): apresentação composta pelo DWM como qualquer
        // app moderno — o glow/OpenGL sofria unredirection em tela cheia.
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title(app::ROOT_TITLE)
            .with_decorations(false)
            .with_taskbar(false)
            .with_active(false)
            .with_resizable(false)
            .with_maximize_button(false)
            .with_minimize_button(false)
            .with_position(egui::Pos2::new(-32000.0, -32000.0))
            .with_inner_size(egui::Vec2::new(1.0, 1.0))
            .with_icon(std::sync::Arc::new(app::app_icon_data())),
        persist_window: false,
        centered: false,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };

    run_event_loop_with(config, task, options);
}

/// Sobe o eframe com a janela-raiz padrão deste processo.
fn run_event_loop(config: config::Config, task: app::Task) {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title(app::ROOT_TITLE)
            .with_decorations(false)
            .with_taskbar(false)
            .with_active(false)
            .with_resizable(false)
            .with_maximize_button(false)
            .with_minimize_button(false)
            .with_position(egui::Pos2::new(-32000.0, -32000.0))
            .with_inner_size(egui::Vec2::new(1.0, 1.0))
            .with_icon(std::sync::Arc::new(app::app_icon_data())),
        persist_window: false,
        centered: false,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    run_event_loop_with(config, task, options);
}

fn run_event_loop_with(
    config: config::Config,
    task: app::Task,
    options: eframe::NativeOptions,
) {
    let result = eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::GuiApp::new(cc, config, task)))),
    );
    if let Err(err) = result {
        log::error!("falha fatal no event loop: {err}");
    }
    log::info!("processo de GUI encerrado");
}

#[cfg(windows)]
fn load_shots(
    name: &str,
    len: usize,
) -> error::Result<(Vec<capture::MonitorShot>, Vec<platform::window_list::WindowTarget>)> {
    platform::ipc::consume(name, len)
}

#[cfg(not(windows))]
fn load_shots(
    _name: &str,
    _len: usize,
) -> error::Result<(Vec<capture::MonitorShot>, Vec<platform::window_list::WindowTarget>)> {
    Err(error::err!("memória compartilhada disponível apenas no Windows"))
}

/// Ajustes de memória do backend wgpu: os padrões do eframe são de aplicativo
/// de tela cheia, e este processo é efêmero.
///
/// * `MemoryUsage`: o alocador do dx12 passa de blocos de 256 MiB (device) e
///   64 MiB (host-visível, que é RAM do sistema) para 8 MiB e 4 MiB. Sem isso o
///   primeiro upload — o atlas de fontes, já no boot — reserva um heap UPLOAD
///   de 64 MiB.
/// * `LowPower`: em máquina de GPU híbrida escolhe a integrada. Uma ferramenta
///   de captura não ganha nada da dedicada, e o driver de usuário dela (dezenas
///   de MB mapeados) é a maior fatia isolada do consumo — além de manter a
///   placa acordada. `WGPU_POWER_PREF=high` reverte sem recompilar.
/// * `desired_maximum_frame_latency: 1`: os overlays de seleção cobrem
///   monitores inteiros; cada buffer de swapchain a menos é um 4K (33 MB) a
///   menos. Latência baixa é o que interessa aqui, não vazão.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup};

    let mut options = WgpuConfiguration {
        desired_maximum_frame_latency: Some(1),
        ..Default::default()
    };
    if let WgpuSetup::CreateNew(setup) = &mut options.wgpu_setup {
        // Só D3D12: é o único backend compilado (feature "dx12" do wgpu) e o
        // único que faz sentido no alvo. O padrão do eframe pede PRIMARY | GL,
        // o que ainda faz o wgpu enumerar e sondar backends inexistentes no
        // boot — e deixa `WGPU_BACKEND` capaz de pedir um caminho que não
        // existe no binário.
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
        setup.power_preference =
            wgpu::PowerPreference::from_env().unwrap_or(wgpu::PowerPreference::LowPower);
        setup.device_descriptor = std::sync::Arc::new(|_adapter| wgpu::DeviceDescriptor {
            label: Some("rustshot"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                // Igual ao padrão do eframe: textura grande o bastante para a
                // captura inteira de um monitor 4K/5K.
                max_texture_dimension_2d: 8192,
                // O padrão do wgpu é 1.000.000, e no D3D12 esse número não é
                // um teto: o backend cria um descriptor heap shader-visible
                // com exatamente essa quantidade de descritores no
                // `CreateDevice` (`wgpu-hal/src/dx12/device.rs`, capacity_views
                // → `descriptor.rs`, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE).
                // São dezenas de MB comprometidos antes de existir um pixel.
                //
                // Este app usa entre cinco e nove descritores: o atlas de
                // fontes, uma textura por monitor e os uniformes. 4096 é três
                // ordens de grandeza acima disso e custa algumas centenas de
                // KB. Não aperto mais porque estourar o heap não degrada — o
                // wgpu-hal devolve OutOfMemory e o device cai.
                max_non_sampler_bindings: 4096,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        });
    }
    options
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));
}

/// Logger em arquivo com rotação simples (`.log` → `.log.old` acima de 2 MB).
/// Só o residente rotaciona: o arquivo fica aberto por ele, e um filho
/// renomeando-o levaria o log da sessão embora.
fn init_logging(rotate: bool) {
    let dir = config::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("rustshot.log");

    if rotate {
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > LOG_ROTATE_BYTES {
                let _ = std::fs::rename(&path, dir.join("rustshot.log.old"));
            }
        }
    }

    let file = std::fs::OpenOptions::new().create(true).append(true).open(&path);
    if let Ok(file) = file {
        // Build debug loga em nível Debug (diagnóstico de geometria/DPI).
        // O logger próprio já filtra os módulos gráficos ruidosos
        // (naga/wgpu/egui_wgpu — ver platform::logger).
        let level = if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        };
        platform::logger::init(file, level);
    }
}

/// Linha de comando.
///
/// A maior parte é interna — quem monta os argumentos `--gui …` é o próprio
/// residente. O que é público e documentado no `--help`: abrir uma imagem
/// para anotar, e as consultas `--help`/`--version`.
mod cli {
    use crate::overlay::Purpose;

    pub const HELP: &str = "\
RustShot — captura de tela para Windows 11

USO:
    rustshot                      inicia na bandeja do sistema
    rustshot <imagem>             abre a imagem no editor de anotações
    rustshot --file <imagem>      idem, explícito
    rustshot --clipboard          abre a imagem da área de transferência
    rustshot --capture-fullscreen [--copy] [--save]
                                  captura a tela e sai, sem abrir janela
    rustshot --ocr <imagem>       reconhece o texto da imagem e o mostra
    rustshot --help               mostra esta ajuda
    rustshot --version            mostra a versão

Sem --copy nem --save, a captura de tela cheia é salva na pasta configurada.
Região e janela exigem a seleção na tela, e por isso vêm pelos atalhos
globais configuráveis (por padrão Ctrl+PrtScr, Shift+PrtScr e
Ctrl+Shift+PrtScr) ou pelo menu da bandeja.

ESQUEMA DE URL:
    rustshot://fullscreen[?copy=1][&save=1]
    rustshot://open?file=<caminho>     abre a imagem no editor
    rustshot://clipboard               abre a imagem da área de transferência
    rustshot://ocr?file=<caminho>      reconhece o texto da imagem

Registre o esquema na janela de Configurações. Ele faz o mesmo que as opções
acima, e só o que roda sem janela nem residente: região e edição dependem de
um overlay sobre a tela.

CÓDIGOS DE SAÍDA:
    0  sucesso
    1  falha ao capturar ou ao abrir a imagem
    2  erro de uso";

    pub enum Mode {
        Resident,
        /// Só imprimir algo e sair com sucesso.
        Print(String),
        /// Abrir o editor sobre uma imagem do disco.
        EditFile(std::path::PathBuf),
        /// Reconhecer o texto de uma imagem do disco e mostrá-lo.
        OcrFile(std::path::PathBuf),
        /// Abrir o editor sobre a imagem que está na área de transferência.
        EditClipboard,
        /// Capturar a tela e sair, sem janela nenhuma.
        QuickCapture { copy: bool, save: bool },
        Gui(GuiRequest),
    }

    pub struct GuiRequest {
        /// `HWND` da janela de shell do residente, para o caminho de volta.
        pub parent: isize,
        pub task: GuiTask,
    }

    pub enum GuiTask {
        Select { shots: String, len: usize, purpose: Purpose },
        Settings,
        /// Retomar a edição gravada em disco.
        Recover,
    }

    /// `rustshot://<comando>?<chave>=<valor>&…` → o mesmo `Mode` que a linha
    /// de comando produziria.
    ///
    /// Só entram os comandos que rodam **sem janela e sem residente**: a
    /// captura de tela cheia, o OCR de um arquivo e a abertura do editor.
    /// Região e edição dependem de um overlay sobre a tela, que é trabalho do
    /// residente e não de um processo disparado por um link.
    ///
    /// A decodificação de `%XX` é feita aqui porque o shell entrega a URL
    /// como o autor a escreveu; um caminho com espaço chega percent-encoded.
    fn parse_url(url: &str) -> Result<Mode, String> {
        let resto = url
            .strip_prefix("rustshot://")
            .or_else(|| url.strip_prefix("rustshot:"))
            .unwrap_or("")
            .trim_end_matches('/');
        let (comando, consulta) = match resto.split_once('?') {
            Some((c, q)) => (c, q),
            None => (resto, ""),
        };
        let param = |nome: &str| -> Option<String> {
            consulta.split('&').find_map(|par| {
                let (chave, valor) = par.split_once('=')?;
                (chave == nome).then(|| percent_decode(valor))
            })
        };
        let ligado = |nome: &str| {
            param(nome).is_some_and(|v| matches!(v.as_str(), "1" | "true" | "sim" | ""))
        };

        match comando.trim_end_matches('/') {
            "fullscreen" | "capture" => {
                let copy = ligado("copy");
                // Mesma regra da linha de comando: sem pedido explícito,
                // salvar é o que a captura de tela cheia faz.
                let save = ligado("save") || !copy;
                Ok(Mode::QuickCapture { copy, save })
            }
            "open" | "edit" => match param("file") {
                Some(caminho) => Ok(Mode::EditFile(std::path::PathBuf::from(caminho))),
                None => Ok(Mode::EditClipboard),
            },
            "clipboard" => Ok(Mode::EditClipboard),
            "ocr" => match param("file") {
                Some(caminho) => Ok(Mode::OcrFile(std::path::PathBuf::from(caminho))),
                None => Err("rustshot://ocr exige file=<caminho>".to_owned()),
            },
            outro => Err(format!(
                "comando desconhecido na URL: {outro:?}. \
                 Conhecidos: fullscreen, open, clipboard, ocr"
            )),
        }
    }

    /// Decodifica `%XX` e `+`, o bastante para um caminho de arquivo numa URL.
    fn percent_decode(texto: &str) -> String {
        let bytes = texto.replace('+', " ").into_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let hex = (i + 2 < bytes.len()).then(|| {
                std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
            });
            match (bytes[i], hex.flatten()) {
                (b'%', Some(byte)) => {
                    out.push(byte);
                    i += 3;
                }
                (b, _) => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// `--gui select --shots <nome> --len <bytes> --purpose region|edit
    /// --parent <hwnd>` ou `--gui settings --parent <hwnd>`.
    pub fn parse(args: impl Iterator<Item = String>) -> Result<Mode, String> {
        let mut kind: Option<String> = None;
        let mut shots: Option<String> = None;
        let mut len: Option<usize> = None;
        let mut purpose: Option<Purpose> = None;
        let mut parent: isize = 0;

        let mut file: Option<std::path::PathBuf> = None;
        let mut ocr: Option<std::path::PathBuf> = None;
        let mut clipboard = false;
        let mut fullscreen = false;
        let mut copy = false;
        let mut save = false;

        let mut args = args.peekable();
        if args.peek().is_none() {
            return Ok(Mode::Resident);
        }

        // O shell entrega a URL inteira como um argumento só. Ela é traduzida
        // para os mesmos modos da linha de comando, e não para um caminho
        // paralelo: o que a URL pode pedir é exatamente o que a CLI já faz.
        if let Some(url) = args.peek().filter(|a| a.starts_with("rustshot:")).cloned() {
            return parse_url(&url);
        }

        while let Some(arg) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{arg} exige um valor"));
            match arg.as_str() {
                "--gui" => kind = Some(value()?),
                "--recover" => kind = Some("recover".to_owned()),
                "--shots" => shots = Some(value()?),
                "--len" => {
                    let raw = value()?;
                    len = Some(raw.parse().map_err(|_| format!("--len inválido: {raw}"))?);
                }
                "--purpose" => {
                    purpose = Some(match value()?.as_str() {
                        "region" => Purpose::CopyDirect,
                        "edit" => Purpose::Edit,
                        "ocr" => Purpose::Ocr,
                        other => return Err(format!("--purpose desconhecido: {other}")),
                    });
                }
                "--parent" => {
                    let raw = value()?;
                    parent = raw.parse().map_err(|_| format!("--parent inválido: {raw}"))?;
                }
                "--help" | "-h" => return Ok(Mode::Print(HELP.to_owned())),
                "--version" | "-V" => {
                    return Ok(Mode::Print(format!(
                        "{} {}",
                        env!("CARGO_PKG_NAME"),
                        env!("CARGO_PKG_VERSION")
                    )))
                }
                "--file" => file = Some(std::path::PathBuf::from(value()?)),
                "--ocr" => ocr = Some(std::path::PathBuf::from(value()?)),
                "--clipboard" => clipboard = true,
                "--capture-fullscreen" => fullscreen = true,
                "--copy" => copy = true,
                "--save" => save = true,
                // Um caminho solto abre a imagem: é o que acontece ao
                // arrastar um arquivo sobre o executável.
                other if !other.starts_with('-') && file.is_none() => {
                    file = Some(std::path::PathBuf::from(other));
                }
                other => return Err(format!("argumento desconhecido: {other}")),
            }
        }

        if let Some(path) = ocr {
            if clipboard || file.is_some() || kind.is_some() || fullscreen {
                return Err("--ocr não combina com outra origem".to_owned());
            }
            return Ok(Mode::OcrFile(path));
        }
        if fullscreen {
            if clipboard || file.is_some() || kind.is_some() {
                return Err("--capture-fullscreen não combina com outra origem".to_owned());
            }
            // Sem pedido explícito, salvar é o comportamento do atalho de
            // tela cheia — a linha de comando não inventa outro.
            let save = save || !copy;
            return Ok(Mode::QuickCapture { copy, save });
        }
        if (copy || save) && kind.is_none() {
            return Err("--copy e --save exigem --capture-fullscreen".to_owned());
        }
        if clipboard && file.is_some() {
            return Err("--clipboard não combina com um arquivo".to_owned());
        }
        if (clipboard || file.is_some()) && kind.is_some() {
            return Err("uma imagem não combina com --gui".to_owned());
        }
        if clipboard {
            return Ok(Mode::EditClipboard);
        }
        if let Some(path) = file {
            return Ok(Mode::EditFile(path));
        }

        let task = match kind.as_deref() {
            Some("select") => GuiTask::Select {
                shots: shots.ok_or("--gui select exige --shots")?,
                len: len.ok_or("--gui select exige --len")?,
                purpose: purpose.ok_or("--gui select exige --purpose")?,
            },
            Some("settings") => GuiTask::Settings,
            Some("recover") => GuiTask::Recover,
            Some(other) => return Err(format!("--gui desconhecido: {other}")),
            None => return Err("use --gui select|settings".to_owned()),
        };
        Ok(Mode::Gui(GuiRequest { parent, task }))
    }
}

#[cfg(test)]
mod tests {
    use super::cli::{parse, GuiTask, Mode};
    use crate::overlay::Purpose;

    fn args(line: &str) -> impl Iterator<Item = String> + '_ {
        line.split_whitespace().map(str::to_owned)
    }

    #[test]
    fn no_arguments_is_the_resident() {
        assert!(matches!(parse(args("")).unwrap(), Mode::Resident));
    }

    #[test]
    fn ocr_takes_a_path() {
        let Mode::OcrFile(path) = parse(args("--ocr captura.png")).unwrap() else {
            panic!("--ocr devia pedir OCR do arquivo")
        };
        assert_eq!(path.to_str(), Some("captura.png"));
    }

    #[test]
    fn ocr_rejects_a_second_source() {
        for linha in ["--ocr a.png --clipboard", "--ocr a.png --file b.png"] {
            assert!(parse(args(linha)).is_err(), "{linha} devia ser recusado");
        }
    }

    #[test]
    fn help_and_version_just_print() {
        for flag in ["--help", "-h"] {
            let Mode::Print(text) = parse(args(flag)).unwrap() else {
                panic!("{flag} devia só imprimir")
            };
            assert!(text.contains("USO:"), "a ajuda precisa dizer como usar");
        }
        let Mode::Print(text) = parse(args("--version")).unwrap() else {
            panic!("esperado texto")
        };
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn an_image_path_opens_the_editor() {
        // Tanto explícito quanto solto — este último é o arquivo arrastado
        // sobre o executável.
        for line in ["--file C:\\fotos\\tela.png", "C:\\fotos\\tela.png"] {
            let Mode::EditFile(path) = parse(args(line)).unwrap() else {
                panic!("{line} devia abrir o editor")
            };
            assert!(path.to_string_lossy().ends_with("tela.png"));
        }
    }

    #[test]
    fn the_clipboard_is_its_own_source() {
        assert!(matches!(parse(args("--clipboard")).unwrap(), Mode::EditClipboard));
        assert!(
            parse(args("--clipboard --file foto.png")).is_err(),
            "duas origens de imagem ao mesmo tempo não fazem sentido"
        );
    }

    #[test]
    fn an_image_does_not_mix_with_the_internal_gui_mode() {
        assert!(parse(args("--gui settings --parent 1 foto.png")).is_err());
    }

    #[test]
    fn parses_the_select_request() {
        let mode = parse(args(
            "--gui select --shots Local\\rustshot-shots-7-1 --len 128 --purpose edit --parent 4242",
        ))
        .unwrap();
        let Mode::Gui(request) = mode else { panic!("esperado modo GUI") };
        assert_eq!(request.parent, 4242);
        match request.task {
            GuiTask::Select { shots, len, purpose } => {
                assert_eq!(shots, "Local\\rustshot-shots-7-1");
                assert_eq!(len, 128);
                assert_eq!(purpose, Purpose::Edit);
            }
            other => panic!("esperado select, veio {}", std::any::type_name_of_val(&other)),
        }
    }

    #[test]
    fn a_url_vira_o_mesmo_modo_da_linha_de_comando() {
        let Mode::QuickCapture { copy, save } = parse(args("rustshot://fullscreen")).unwrap()
        else {
            panic!("esperava captura de tela cheia");
        };
        assert!(!copy && save, "sem pedido explícito, a URL salva como a CLI");

        let Mode::QuickCapture { copy, save } =
            parse(args("rustshot://fullscreen?copy=1")).unwrap()
        else {
            panic!("esperava captura de tela cheia");
        };
        assert!(copy && !save);

        let Mode::QuickCapture { copy, save } =
            parse(args("rustshot://capture?copy=1&save=1")).unwrap()
        else {
            panic!("esperava captura de tela cheia");
        };
        assert!(copy && save);

        assert!(matches!(
            parse(args("rustshot://clipboard")).unwrap(),
            Mode::EditClipboard
        ));
        assert!(matches!(
            parse(args("rustshot://open")).unwrap(),
            Mode::EditClipboard
        ));
    }

    #[test]
    fn a_url_decodifica_o_caminho_do_arquivo() {
        // O shell entrega a URL como o autor a escreveu: um caminho com
        // espaço chega percent-encoded.
        let Mode::EditFile(path) =
            parse(args("rustshot://open?file=C%3A%5CFotos%5Cminha%20foto.png")).unwrap()
        else {
            panic!("esperava abrir um arquivo");
        };
        assert_eq!(path, std::path::PathBuf::from(r"C:\Fotos\minha foto.png"));

        let Mode::OcrFile(path) = parse(args("rustshot://ocr?file=a+b.png")).unwrap() else {
            panic!("esperava OCR de arquivo");
        };
        assert_eq!(path, std::path::PathBuf::from("a b.png"));
    }

    #[test]
    fn uma_url_desconhecida_e_recusada() {
        // Melhor um código de erro de uso que capturar a tela por engano.
        assert!(parse(args("rustshot://banana")).is_err());
        assert!(parse(args("rustshot://ocr")).is_err(), "ocr sem arquivo");
    }

    #[test]
    fn quick_capture_defaults_to_saving() {
        let Mode::QuickCapture { copy, save } = parse(args("--capture-fullscreen")).unwrap()
        else {
            panic!("esperada captura rápida")
        };
        assert!(!copy && save, "sem pedido explícito, salvar é o padrão do atalho");

        let Mode::QuickCapture { copy, save } =
            parse(args("--capture-fullscreen --copy")).unwrap()
        else {
            panic!("esperada captura rápida")
        };
        assert!(copy && !save, "--copy sozinho não salva também");

        let Mode::QuickCapture { copy, save } =
            parse(args("--capture-fullscreen --copy --save")).unwrap()
        else {
            panic!("esperada captura rápida")
        };
        assert!(copy && save, "os dois juntos fazem os dois");
    }

    #[test]
    fn quick_output_needs_something_to_capture() {
        assert!(parse(args("--copy")).is_err());
        assert!(parse(args("--capture-fullscreen --clipboard")).is_err());
    }

    #[test]
    fn parses_the_recover_request() {
        let mode = parse(args("--recover --parent 5")).unwrap();
        let Mode::Gui(request) = mode else { panic!("esperado modo GUI") };
        assert!(matches!(request.task, GuiTask::Recover));
    }

    #[test]
    fn parses_the_settings_request() {
        let mode = parse(args("--gui settings --parent 9")).unwrap();
        let Mode::Gui(request) = mode else { panic!("esperado modo GUI") };
        assert!(matches!(request.task, GuiTask::Settings));
        assert_eq!(request.parent, 9);
    }

    #[test]
    fn rejects_incomplete_or_unknown_lines() {
        // Sem os dados da captura o filho não teria o que exibir.
        assert!(parse(args("--gui select --parent 1")).is_err());
        assert!(parse(args("--gui select --shots x --len 1 --parent 1")).is_err());
        assert!(parse(args("--gui bananas")).is_err());
        assert!(parse(args("--parent")).is_err(), "valor faltando");
        assert!(parse(args("--parent abc")).is_err(), "hwnd não numérico");
        assert!(parse(args("--purpose region")).is_err(), "sem --gui");
    }
}
