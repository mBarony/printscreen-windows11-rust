//! Ícone e menu da bandeja do sistema (RF-06).
//!
//! A aplicação não possui janela principal: o menu da bandeja é o ponto de
//! entrada. O `TrayIcon` precisa ser criado (e mantido vivo) na thread do
//! event loop; os cliques chegam via `MenuEvent::set_event_handler`, tratado
//! em `main.rs`, que empurra eventos para a fila da aplicação.

use anyhow::{Context as _, Result};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::config::APP_NAME;

pub const MENU_CAPTURE_FULLSCREEN: &str = "capture_fullscreen";
pub const MENU_CAPTURE_REGION: &str = "capture_region";
pub const MENU_CAPTURE_EDIT: &str = "capture_edit";
pub const MENU_OPEN_FOLDER: &str = "open_folder";
pub const MENU_SETTINGS: &str = "settings";
pub const MENU_AUTOSTART: &str = "autostart";
pub const MENU_QUIT: &str = "quit";

pub struct Tray {
    // Mantidos vivos: soltar qualquer um deles remove o ícone/menu.
    _tray: TrayIcon,
    autostart_item: CheckMenuItem,
}

impl Tray {
    /// Cria o ícone da bandeja com o menu completo (RF-06).
    pub fn new(autostart_checked: bool) -> Result<Self> {
        let menu = Menu::new();

        let capture_fullscreen =
            MenuItem::with_id(MENU_CAPTURE_FULLSCREEN, "Capturar tela cheia", true, None);
        let capture_region =
            MenuItem::with_id(MENU_CAPTURE_REGION, "Capturar região", true, None);
        let capture_edit =
            MenuItem::with_id(MENU_CAPTURE_EDIT, "Capturar e editar", true, None);
        let open_folder =
            MenuItem::with_id(MENU_OPEN_FOLDER, "Abrir pasta de capturas", true, None);
        let settings = MenuItem::with_id(MENU_SETTINGS, "Configurações…", true, None);
        let autostart_item = CheckMenuItem::with_id(
            MENU_AUTOSTART,
            "Iniciar com o Windows",
            true,
            autostart_checked,
            None,
        );
        let quit = MenuItem::with_id(MENU_QUIT, "Sair", true, None);

        menu.append_items(&[
            &capture_fullscreen,
            &capture_region,
            &capture_edit,
            &PredefinedMenuItem::separator(),
            &open_folder,
            &settings,
            &autostart_item,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .context("montando menu da bandeja")?;

        let (rgba, w, h) = tray_icon_rgba();
        let icon = tray_icon::Icon::from_rgba(rgba, w, h).context("ícone da bandeja")?;

        let tray = TrayIconBuilder::new()
            .with_id(APP_NAME)
            .with_menu(Box::new(menu))
            .with_tooltip(APP_NAME)
            .with_icon(icon)
            .build()
            .context("criando ícone da bandeja")?;

        Ok(Self { _tray: tray, autostart_item })
    }

    /// Sincroniza o checkbox "Iniciar com o Windows" com o estado real.
    pub fn set_autostart_checked(&self, checked: bool) {
        self.autostart_item.set_checked(checked);
    }
}

/// Decodifica o `icon.ico` embutido e redimensiona para o tamanho da bandeja.
pub fn tray_icon_rgba() -> (Vec<u8>, u32, u32) {
    app_icon_rgba(32)
}

/// RGBA do ícone da aplicação no tamanho pedido (bandeja, janelas, exe).
pub fn app_icon_rgba(size: u32) -> (Vec<u8>, u32, u32) {
    const ICO: &[u8] = include_bytes!("../assets/icon.ico");
    match image::load_from_memory_with_format(ICO, image::ImageFormat::Ico) {
        Ok(img) => {
            let img = img.to_rgba8();
            let img = if img.width() != size {
                image::imageops::resize(&img, size, size, image::imageops::FilterType::Lanczos3)
            } else {
                img
            };
            let (w, h) = (img.width(), img.height());
            (img.into_raw(), w, h)
        }
        Err(err) => {
            // Nunca deve acontecer (o .ico é gerado no repositório); um
            // quadrado laranja mantém a bandeja funcional mesmo assim.
            log::error!("falha ao decodificar icon.ico embutido: {err}");
            let px = [0xE0u8, 0x5A, 0x2B, 0xFF];
            let data: Vec<u8> = std::iter::repeat_n(px, (size * size) as usize)
                .flatten()
                .collect();
            (data, size, size)
        }
    }
}
