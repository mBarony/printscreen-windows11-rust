//! Registro e re-registro dos atalhos globais (§11) via
//! `RegisterHotKey`/`WM_HOTKEY` (`platform::shell`), substituindo o crate
//! `global-hotkey`.
//!
//! O registro acontece na thread do event loop (janela de shell). Se o
//! registro de um atalho falhar (já tomado por outro app), a aplicação
//! continua com os demais e o chamador é informado. A configuração continua
//! usando os nomes de tecla do padrão W3C (`PrintScreen`, `KeyA`, `F5`…) —
//! os mesmos arquivos `config.json` da v1.x seguem válidos.

use crate::config::{HotkeyDef, HotkeysConfig};
use crate::platform::shell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Fullscreen,
    Region,
    Edit,
}

impl HotkeyAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fullscreen => "Capturar tela cheia",
            Self::Region => "Capturar região",
            Self::Edit => "Capturar e editar",
        }
    }
}

// Modificadores do RegisterHotKey.
const MOD_ALT: u32 = 0x1;
const MOD_CONTROL: u32 = 0x2;
const MOD_SHIFT: u32 = 0x4;
const MOD_WIN: u32 = 0x8;

/// Converte a definição serializada em `(modificadores, virtual-key)`.
pub fn parse_hotkey(def: &HotkeyDef) -> Result<(u32, u32), String> {
    let mut mods = 0u32;
    for m in &def.modifiers {
        match m.trim().to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => mods |= MOD_CONTROL,
            "SHIFT" => mods |= MOD_SHIFT,
            "ALT" => mods |= MOD_ALT,
            "WIN" | "SUPER" | "META" | "CMD" => mods |= MOD_WIN,
            other => return Err(format!("modificador desconhecido: {other:?}")),
        }
    }
    let vk = code_to_vk(def.code.trim()).ok_or_else(|| format!("tecla desconhecida: {:?}", def.code))?;
    Ok((mods, vk))
}

/// Nome de tecla W3C (`keyboard-types`) → virtual-key do Windows.
fn code_to_vk(code: &str) -> Option<u32> {
    // Teclas de letra/dígito/função seguem padrões simples.
    if let Some(letter) = code.strip_prefix("Key") {
        let mut chars = letter.chars();
        if let (Some(c @ 'A'..='Z'), None) = (chars.next(), chars.next()) {
            return Some(c as u32); // VK_A..VK_Z == 'A'..'Z'
        }
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        let mut chars = digit.chars();
        if let (Some(c @ '0'..='9'), None) = (chars.next(), chars.next()) {
            return Some(c as u32); // VK_0..VK_9 == '0'..'9'
        }
    }
    if let Some(n) = code.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x70 + n - 1); // VK_F1..VK_F24
        }
    }
    Some(match code {
        "PrintScreen" => 0x2C,
        "ScrollLock" => 0x91,
        "Pause" => 0x13,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "ArrowUp" => 0x26,
        "ArrowDown" => 0x28,
        "ArrowLeft" => 0x25,
        "ArrowRight" => 0x27,
        "Space" => 0x20,
        "Enter" => 0x0D,
        "Tab" => 0x09,
        "Backquote" => 0xC0, // VK_OEM_3
        "Minus" => 0xBD,     // VK_OEM_MINUS
        "Equal" => 0xBB,     // VK_OEM_PLUS
        _ => return None,
    })
}

/// Texto amigável, ex.: `Ctrl + Shift + PrintScreen`.
pub fn pretty(def: &HotkeyDef) -> String {
    let mut parts: Vec<String> = Vec::new();
    for m in &def.modifiers {
        parts.push(match m.trim().to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => "Ctrl".into(),
            "SHIFT" => "Shift".into(),
            "ALT" => "Alt".into(),
            "WIN" | "SUPER" | "META" | "CMD" => "Win".into(),
            other => other.to_string(),
        });
    }
    parts.push(def.code.trim().to_string());
    parts.join(" + ")
}

/// Ids fixos por ação no `RegisterHotKey` (1 a 3).
fn hotkey_id(action: HotkeyAction) -> i32 {
    match action {
        HotkeyAction::Fullscreen => 1,
        HotkeyAction::Region => 2,
        HotkeyAction::Edit => 3,
    }
}

/// Estado dos atalhos registrados no sistema.
#[derive(Default)]
pub struct Hotkeys {
    registered: Vec<HotkeyAction>,
}

/// Falha de registro de um atalho individual (a aplicação segue com os demais).
pub struct HotkeyFailure {
    pub action: HotkeyAction,
    pub pretty: String,
    pub reason: String,
}

impl Hotkeys {
    pub fn new() -> Self {
        Self::default()
    }

    /// (Re)registra os três atalhos conforme a configuração; alterações têm
    /// efeito imediato, sem reiniciar (RF-05). Retorna as falhas individuais.
    /// Deve rodar na thread do event loop (dona da janela de shell).
    pub fn apply(&mut self, config: &HotkeysConfig) -> Vec<HotkeyFailure> {
        for action in self.registered.drain(..) {
            shell::unregister_hotkey(hotkey_id(action));
        }

        let mut failures = Vec::new();
        let wanted = [
            (HotkeyAction::Fullscreen, &config.fullscreen),
            (HotkeyAction::Region, &config.region),
            (HotkeyAction::Edit, &config.edit),
        ];
        for (action, def) in wanted {
            let pretty = pretty(def);
            match parse_hotkey(def) {
                Ok((mods, vk)) => match shell::register_hotkey(hotkey_id(action), mods, vk) {
                    Ok(()) => {
                        log::info!("atalho {pretty} registrado para {action:?}");
                        self.registered.push(action);
                    }
                    Err(error) => failures.push(HotkeyFailure {
                        action,
                        pretty,
                        reason: error.to_string(),
                    }),
                },
                Err(reason) => failures.push(HotkeyFailure { action, pretty, reason }),
            }
        }
        failures
    }

    /// Mapeia o id do `WM_HOTKEY` para a ação.
    pub fn action_for(&self, id: i32) -> Option<HotkeyAction> {
        [HotkeyAction::Fullscreen, HotkeyAction::Region, HotkeyAction::Edit]
            .into_iter()
            .find(|&a| hotkey_id(a) == id && self.registered.contains(&a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_hotkeys() {
        let cfg = HotkeysConfig::default();
        assert_eq!(parse_hotkey(&cfg.fullscreen), Ok((MOD_CONTROL, 0x2C)));
        assert_eq!(parse_hotkey(&cfg.region), Ok((MOD_SHIFT, 0x2C)));
        assert_eq!(parse_hotkey(&cfg.edit), Ok((MOD_CONTROL | MOD_SHIFT, 0x2C)));
    }

    #[test]
    fn rejects_unknown_key() {
        let def = HotkeyDef::new(&["CTRL"], "TeclaInexistente");
        assert!(parse_hotkey(&def).is_err());
    }

    #[test]
    fn pretty_formats() {
        let def = HotkeyDef::new(&["CTRL", "SHIFT"], "PrintScreen");
        assert_eq!(pretty(&def), "Ctrl + Shift + PrintScreen");
    }

    #[test]
    fn vk_mapping_covers_choice_list() {
        for code in [
            "PrintScreen", "ScrollLock", "Pause", "Insert", "Home", "End", "PageUp",
            "PageDown", "F1", "F12", "F24", "KeyA", "KeyZ", "Digit0", "Digit9", "Space",
            "Enter", "Backquote", "Minus", "Equal", "ArrowLeft", "Delete", "Tab",
        ] {
            assert!(code_to_vk(code).is_some(), "sem VK para {code}");
        }
        assert_eq!(code_to_vk("KeyA"), Some(0x41));
        assert_eq!(code_to_vk("Digit0"), Some(0x30));
        assert_eq!(code_to_vk("F1"), Some(0x70));
        assert!(code_to_vk("Banana").is_none());
        assert!(code_to_vk("F25").is_none());
    }
}
