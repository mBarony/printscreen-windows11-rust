//! Ícone e menu da bandeja do sistema (RF-06), sobre `platform::shell`.
//!
//! A aplicação não possui janela principal: o menu da bandeja é o ponto de
//! entrada. A janela de shell precisa ser criada na thread do event loop;
//! os cliques chegam pelo handler registrado, que empurra eventos para a
//! fila da aplicação (`app::events`).

use crate::error::Result;
use crate::platform::shell::{self, MenuEntry, ShellEvent};

// Ids numéricos dos itens de menu (WM_COMMAND range de usuário).
pub const MENU_CAPTURE_FULLSCREEN: u16 = 0x1001;
pub const MENU_CAPTURE_REGION: u16 = 0x1002;
pub const MENU_CAPTURE_EDIT: u16 = 0x1003;
pub const MENU_OPEN_FOLDER: u16 = 0x1004;
pub const MENU_SETTINGS: u16 = 0x1005;
pub const MENU_AUTOSTART: u16 = 0x1006;
pub const MENU_QUIT: u16 = 0x1007;
pub const MENU_RECOVER: u16 = 0x1008;
pub const MENU_CAPTURE_DELAYED: u16 = 0x1009;
pub const MENU_REPEAT_REGION: u16 = 0x100A;

/// Ícone RGBA cru embutido (gerado do mesmo desenho do icon.ico).
static ICON_32: &[u8] = include_bytes!("../assets/icon-32.rgba");
static ICON_64: &[u8] = include_bytes!("../assets/icon-64.rgba");

pub struct Tray {
    _private: (),
}

impl Tray {
    /// Cria a janela de shell + ícone da bandeja com o menu completo
    /// (RF-06) e registra o handler de eventos.
    pub fn new(
        autostart_checked: bool,
        recoverable: bool,
        repeatable: bool,
        handler: impl Fn(ShellEvent) + Send + Sync + 'static,
    ) -> Result<Self> {
        let mut menu = vec![
            MenuEntry::Item { id: MENU_CAPTURE_FULLSCREEN, label: "Capturar tela cheia" },
            MenuEntry::Item { id: MENU_CAPTURE_REGION, label: "Capturar região" },
            MenuEntry::Item { id: MENU_CAPTURE_EDIT, label: "Capturar e editar" },
            MenuEntry::Item {
                id: MENU_CAPTURE_DELAYED,
                label: "Capturar tela cheia em 3 s",
            },
        ];
        // Só aparece depois da primeira região: antes dela não há o que
        // repetir, e o item ficaria inerte.
        if repeatable {
            menu.push(MenuEntry::Item {
                id: MENU_REPEAT_REGION,
                label: "Repetir a última região",
            });
        }
        // Só aparece quando há de fato o que recuperar — um item permanente
        // e quase sempre inerte só ocuparia espaço.
        if recoverable {
            menu.push(MenuEntry::Separator);
            menu.push(MenuEntry::Item {
                id: MENU_RECOVER,
                label: "Recuperar edição não salva",
            });
        }
        menu.extend([
            MenuEntry::Item { id: MENU_OPEN_FOLDER, label: "Abrir pasta de capturas" },
            MenuEntry::Item { id: MENU_SETTINGS, label: "Configurações…" },
            MenuEntry::Check {
                id: MENU_AUTOSTART,
                label: "Iniciar com o Windows",
                checked: autostart_checked,
            },
            MenuEntry::Separator,
            MenuEntry::Item { id: MENU_QUIT, label: "Sair" },
        ]);
        shell::init(
            crate::config::APP_NAME,
            (ICON_32, 32, 32),
            &menu,
            Box::new(handler),
        )?;
        Ok(Self { _private: () })
    }

    /// Sincroniza o checkbox "Iniciar com o Windows" com o estado real.
    pub fn set_autostart_checked(&self, checked: bool) {
        shell::set_menu_checked(MENU_AUTOSTART, checked);
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        shell::remove_icon();
    }
}

/// RGBA do ícone da aplicação (janelas do editor/configurações e exe).
pub fn app_icon_rgba(size: u32) -> (Vec<u8>, u32, u32) {
    match size {
        32 => (ICON_32.to_vec(), 32, 32),
        _ => (ICON_64.to_vec(), 64, 64),
    }
}
