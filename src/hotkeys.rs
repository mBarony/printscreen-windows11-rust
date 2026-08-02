//! Registro e re-registro dos atalhos globais (`global-hotkey`), §11.
//!
//! O `GlobalHotKeyManager` precisa viver na thread que bombeia mensagens
//! Win32 (a thread do event loop do eframe); toda a manipulação acontece em
//! `App::update`/`App::new`. Se o registro de um atalho falhar (já tomado por
//! outro app), a aplicação continua com os demais e o chamador é informado.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;

use crate::config::{HotkeyDef, HotkeysConfig};

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

/// Converte a definição serializada em `HotKey` do `global-hotkey`.
pub fn parse_hotkey(def: &HotkeyDef) -> Result<HotKey, String> {
    let mut mods = Modifiers::empty();
    for m in &def.modifiers {
        match m.trim().to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => mods |= Modifiers::CONTROL,
            "SHIFT" => mods |= Modifiers::SHIFT,
            "ALT" => mods |= Modifiers::ALT,
            "WIN" | "SUPER" | "META" | "CMD" => mods |= Modifiers::SUPER,
            other => return Err(format!("modificador desconhecido: {other:?}")),
        }
    }
    let code: Code = def
        .code
        .trim()
        .parse()
        .map_err(|_| format!("tecla desconhecida: {:?}", def.code))?;
    let mods = (!mods.is_empty()).then_some(mods);
    Ok(HotKey::new(mods, code))
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

struct Registered {
    action: HotkeyAction,
    hotkey: HotKey,
}

/// Estado dos atalhos registrados no sistema.
pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    registered: Vec<Registered>,
}

/// Falha de registro de um atalho individual (a aplicação segue com os demais).
pub struct HotkeyFailure {
    pub action: HotkeyAction,
    pub pretty: String,
    pub reason: String,
}

impl Hotkeys {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            manager: GlobalHotKeyManager::new()?,
            registered: Vec::new(),
        })
    }

    /// (Re)registra os três atalhos conforme a configuração; alterações têm
    /// efeito imediato, sem reiniciar (RF-05). Retorna as falhas individuais.
    pub fn apply(&mut self, config: &HotkeysConfig) -> Vec<HotkeyFailure> {
        for reg in self.registered.drain(..) {
            if let Err(err) = self.manager.unregister(reg.hotkey) {
                log::warn!("falha ao remover atalho de {:?}: {err}", reg.action);
            }
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
                Ok(hotkey) => match self.manager.register(hotkey) {
                    Ok(()) => {
                        log::info!("atalho {pretty} registrado para {action:?}");
                        self.registered.push(Registered { action, hotkey });
                    }
                    Err(err) => failures.push(HotkeyFailure {
                        action,
                        pretty,
                        reason: err.to_string(),
                    }),
                },
                Err(reason) => failures.push(HotkeyFailure { action, pretty, reason }),
            }
        }
        failures
    }

    /// Mapeia o id do evento (`GlobalHotKeyEvent::id`) para a ação.
    pub fn action_for(&self, id: u32) -> Option<HotkeyAction> {
        self.registered
            .iter()
            .find(|r| r.hotkey.id() == id)
            .map(|r| r.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_hotkeys() {
        let cfg = HotkeysConfig::default();
        assert!(parse_hotkey(&cfg.fullscreen).is_ok());
        assert!(parse_hotkey(&cfg.region).is_ok());
        assert!(parse_hotkey(&cfg.edit).is_ok());
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
}
