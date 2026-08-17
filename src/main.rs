//! RustShot — captura de tela para Windows 11 (bootstrap).
//!
//! Responsável por: instância única (RF-08), logging em `rustshot.log` (na
//! pasta do executável), carga do `config.json` e inicialização do event loop
//! do eframe com o viewport principal fora da área visível — a aplicação
//! vive na bandeja do sistema (RF-06).

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod capture;
mod clipboard;
mod config;
mod editor;
mod error;
mod hotkeys;
mod imgbuf;
mod jpeg;
mod json;
mod notify;
mod overlay;
mod platform;
mod settings;
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
    // RF-08 primeiro: a segunda instância não deve tocar (nem rotacionar) o
    // log que a instância em execução mantém aberto.
    let Some(_instance) = platform::instance::acquire(SINGLE_INSTANCE_NAME) else {
        notify::toast_blocking(
            "RustShot já está em execução",
            "Use o ícone na bandeja do sistema.",
        );
        return;
    };

    init_logging();
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));

    if !config::state_dir_writable() {
        log::warn!("pasta do executável sem permissão de escrita");
        notify::toast_error(
            "Pasta do executável sem permissão de escrita",
            "Configurações e log não serão gravados. Mova o RustShot para uma pasta gravável (ex.: Documentos).",
        );
    }

    let loaded = config::load();
    log::info!(
        "RustShot {} iniciando; config em {}",
        env!("CARGO_PKG_VERSION"),
        config::config_path().display()
    );

    // O viewport-raiz precisa ficar VISÍVEL para o SO (janela oculta não
    // recebe WM_PAINT, o `update` nunca roda e atalhos/bandeja morrem), mas
    // imperceptível para o usuário: 1×1 px, sem decoração, fora da área da
    // tela, sem ativação e sem redimensionar/maximizar (Win+Seta em uma
    // janela alcançada por engano maximizaria um retângulo vazio).
    // `visible(false)` era a causa do retângulo preto: em máquinas onde a
    // flag não segurava a janela, ela surgia no canto do monitor.
    // O App ainda a remove do Alt-Tab (WS_EX_TOOLWINDOW) e ignora Alt+F4.
    let options = eframe::NativeOptions {
        // wgpu (D3D12/DXGI): apresentação composta pelo DWM como qualquer
        // app moderno — o glow/OpenGL sofria unredirection em tela cheia.
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
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

    let result = eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::RustShotApp::new(cc, loaded)))),
    );
    if let Err(err) = result {
        log::error!("falha fatal no event loop: {err}");
    }
    log::info!("RustShot encerrado");
}

/// Ajustes de memória do backend wgpu: os padrões do eframe são de aplicativo
/// de tela cheia, e este vive na bandeja com um viewport-raiz de 1×1 px — o
/// device D3D12 fica de pé a sessão inteira, então o que ele reserva no boot é
/// consumo permanente.
///
/// * `MemoryUsage`: o alocador do dx12 passa de blocos de 256 MiB (device) e
///   64 MiB (host-visível, que é RAM do sistema) para 8 MiB e 4 MiB. Sem isso o
///   primeiro upload — o atlas de fontes, já no boot — reserva um heap UPLOAD
///   de 64 MiB que fica preso até o app encerrar.
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
        setup.power_preference =
            wgpu::PowerPreference::from_env().unwrap_or(wgpu::PowerPreference::LowPower);
        setup.device_descriptor = std::sync::Arc::new(|_adapter| wgpu::DeviceDescriptor {
            label: Some("rustshot"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                // Igual ao padrão do eframe: textura grande o bastante para a
                // captura inteira de um monitor 4K/5K.
                max_texture_dimension_2d: 8192,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        });
    }
    options
}

/// Logger em arquivo com rotação simples (`.log` → `.log.old` acima de 2 MB).
fn init_logging() {
    let dir = config::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("rustshot.log");

    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > LOG_ROTATE_BYTES {
            let _ = std::fs::rename(&path, dir.join("rustshot.log.old"));
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
