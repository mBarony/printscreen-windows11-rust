//! RustShot — captura de tela para Windows 11 (bootstrap).
//!
//! Responsável por: instância única (RF-08), logging em
//! `%APPDATA%\RustShot\rustshot.log`, carga do `config.json` e inicialização
//! do event loop do eframe com o viewport principal **oculto** — a aplicação
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
mod tray;

use config::APP_NAME;

const SINGLE_INSTANCE_NAME: &str = "rustshot-single-instance-mutex";
/// Acima disso o log é rotacionado para `rustshot.log.old`.
const LOG_ROTATE_BYTES: u64 = 2 * 1024 * 1024;

fn main() {
    init_logging();
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));

    // RF-08: apenas uma instância. A segunda notifica e encerra.
    let _instance = match single_instance::SingleInstance::new(SINGLE_INSTANCE_NAME) {
        Ok(instance) => {
            if !instance.is_single() {
                log::info!("segunda instância detectada; encerrando");
                notify::toast_blocking(
                    "RustShot já está em execução",
                    "Use o ícone na bandeja do sistema.",
                );
                return;
            }
            Some(instance)
        }
        Err(err) => {
            // Sem o mutex seguimos mesmo assim — pior caso, duas instâncias.
            log::warn!("verificação de instância única falhou: {err}");
            None
        }
    };

    let loaded = config::load();
    log::info!(
        "RustShot {} iniciando; config em {}",
        env!("CARGO_PKG_VERSION"),
        config::config_path().display()
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_visible(false)
            .with_decorations(false)
            .with_taskbar(false)
            .with_inner_size(egui::Vec2::new(320.0, 220.0))
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
    let dir = config::app_data_dir();
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
        let _ = simplelog::WriteLogger::init(log::LevelFilter::Info, config, file);
    }
}
