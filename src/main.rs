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
mod hotkeys;
mod notify;
mod overlay;
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
    let _instance = match single_instance::SingleInstance::new(SINGLE_INSTANCE_NAME) {
        Ok(instance) => {
            if !instance.is_single() {
                notify::toast_blocking(
                    "RustShot já está em execução",
                    "Use o ícone na bandeja do sistema.",
                );
                return;
            }
            Some(instance)
        }
        // Sem o mutex seguimos mesmo assim — pior caso, duas instâncias.
        Err(_) => None,
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
        let config = simplelog::ConfigBuilder::new()
            .set_time_format_rfc3339()
            .build();
        // Build debug loga em nível Debug (diagnóstico de geometria/DPI).
        let level = if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        };
        let _ = simplelog::WriteLogger::init(level, config, file);
    }
}
