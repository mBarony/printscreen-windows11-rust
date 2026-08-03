//! Carga, validação e persistência do `config.json` (§6 da especificação).
//!
//! Localização: todo o estado (`config.json` + `rustshot.log`) fica **na
//! mesma pasta do executável** — a aplicação é portátil por definição;
//! desinstalar = apagar a pasta.
//!
//! Leitura tolerante: campos ausentes assumem o padrão e campos desconhecidos
//! (ex.: `image_format` de versões antigas) são ignorados; JSON corrompido é
//! renomeado para `config.json.bak` e um novo arquivo é criado com padrões.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "RustShot";

// ---------------------------------------------------------------------------
// Modelo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// Pasta de destino das capturas. Vazio = padrão (`Imagens\RustShot`).
    pub output_dir: String,
    /// Template do nome do arquivo; tokens `{date}` e `{time}`.
    pub filename_template: String,
    pub fullscreen_scope: FullscreenScope,
    pub hotkeys: HotkeysConfig,
    pub editor: EditorConfig,
    pub start_with_windows: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            output_dir: String::new(),
            filename_template: "screenshot_{date}_{time}".into(),
            fullscreen_scope: FullscreenScope::AllMonitors,
            hotkeys: HotkeysConfig::default(),
            editor: EditorConfig::default(),
            start_with_windows: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullscreenScope {
    /// Área virtual completa: todos os monitores compostos em uma imagem.
    AllMonitors,
    /// Apenas o monitor principal.
    Primary,
    /// O monitor onde o cursor está.
    MonitorUnderCursor,
}

impl FullscreenScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::AllMonitors => "Todos os monitores",
            Self::Primary => "Monitor principal",
            Self::MonitorUnderCursor => "Monitor sob o cursor",
        }
    }
}

/// Um atalho serializado; `modifiers`/`code` mapeiam diretamente para os
/// tipos do crate `global-hotkey` (`Modifiers`, `Code`), evitando parsing
/// próprio de strings de atalho (§6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyDef {
    pub modifiers: Vec<String>,
    pub code: String,
}

impl HotkeyDef {
    pub fn new(modifiers: &[&str], code: &str) -> Self {
        Self {
            modifiers: modifiers.iter().map(|s| s.to_string()).collect(),
            code: code.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeysConfig {
    pub fullscreen: HotkeyDef,
    pub region: HotkeyDef,
    pub edit: HotkeyDef,
}

impl Default for HotkeysConfig {
    fn default() -> Self {
        // `PrtScr` sem modificador não é usado: desde o Windows 11 22H2 a
        // tecla abre a Ferramenta de Captura nativa (§11).
        Self {
            fullscreen: HotkeyDef::new(&["CTRL"], "PrintScreen"),
            region: HotkeyDef::new(&["SHIFT"], "PrintScreen"),
            edit: HotkeyDef::new(&["CTRL", "SHIFT"], "PrintScreen"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    /// Cor padrão das anotações, `#RRGGBB` ou `#RRGGBBAA`.
    pub default_color: String,
    pub default_stroke_width: f32,
    pub default_font_size: f32,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            default_color: "#FF3B30".into(),
            default_stroke_width: 3.0,
            default_font_size: 24.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Caminhos
// ---------------------------------------------------------------------------

/// Pasta de estado da aplicação: a mesma pasta do executável (portátil).
/// Fallback improvável (exe sem pasta-pai resolvível): subpasta própria no
/// temp — nunca a raiz do temp, onde um `config.json` alheio poderia colidir.
pub fn state_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::temp_dir().join(APP_NAME))
}

/// `true` se a pasta de estado aceita escrita (sonda criada e removida).
/// Exe em pasta somente-leitura (ex.: `Program Files` sem elevação) faria
/// config/log falharem em silêncio — o chamador avisa o usuário.
pub fn state_dir_writable() -> bool {
    let probe = state_dir().join(".rustshot-write-probe");
    match std::fs::OpenOptions::new().write(true).create(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Caminho do `config.json`, ao lado do executável.
pub fn config_path() -> PathBuf {
    state_dir().join("config.json")
}

/// Pasta de destino padrão das capturas: `Imagens\RustShot`.
pub fn default_output_dir() -> PathBuf {
    dirs::picture_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(std::env::temp_dir))
        .join(APP_NAME)
}

impl Config {
    pub fn effective_output_dir(&self) -> PathBuf {
        if self.output_dir.trim().is_empty() {
            default_output_dir()
        } else {
            PathBuf::from(self.output_dir.trim())
        }
    }
}

// ---------------------------------------------------------------------------
// Load / save
// ---------------------------------------------------------------------------

/// Resultado do carregamento inicial.
pub struct LoadedConfig {
    pub config: Config,
    /// `true` quando o arquivo não existia (primeira execução).
    pub created: bool,
    /// `true` quando um JSON corrompido foi movido para `config.json.bak`.
    pub recovered: bool,
}

pub fn load() -> LoadedConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) if text.trim().is_empty() => {
            // Arquivo vazio (criado manualmente ou truncado): recria padrões.
            let config = Config::default();
            let _ = save(&config);
            LoadedConfig { config, created: true, recovered: false }
        }
        Ok(text) => match serde_json::from_str::<Config>(&text) {
            Ok(config) => LoadedConfig { config, created: false, recovered: false },
            Err(err) => {
                log::warn!("config.json inválido ({err}); recriando com padrões");
                let bak = path.with_extension("json.bak");
                let _ = std::fs::rename(&path, &bak);
                let config = Config::default();
                let _ = save(&config);
                LoadedConfig { config, created: false, recovered: true }
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let config = Config::default();
            let _ = save(&config);
            LoadedConfig { config, created: true, recovered: false }
        }
        Err(err) => {
            // Erro transitório (lock de antivírus/sync, ACL): NÃO sobrescreve
            // o arquivo do usuário; usa padrões apenas nesta sessão.
            log::warn!("config.json ilegível ({err}); usando padrões nesta sessão");
            LoadedConfig { config: Config::default(), created: false, recovered: false }
        }
    }
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path();
    save_to(config, &path)
}

fn save_to(config: &Config, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("criando pasta {}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(config)?;
    // Escrita atômica: grava em arquivo temporário e renomeia por cima.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes())
        .with_context(|| format!("gravando {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("renomeando para {}", path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Cores
// ---------------------------------------------------------------------------

/// Converte `#RRGGBB`/`#RRGGBBAA` em RGBA; inválido → cor padrão (vermelho).
pub fn parse_color(hex: &str) -> [u8; 4] {
    fn byte(s: &str) -> Option<u8> {
        u8::from_str_radix(s, 16).ok()
    }
    let h = hex.trim().trim_start_matches('#');
    let parsed = match h.len() {
        6 => Some([byte(&h[0..2]), byte(&h[2..4]), byte(&h[4..6]), Some(255)]),
        8 => Some([byte(&h[0..2]), byte(&h[2..4]), byte(&h[4..6]), byte(&h[6..8])]),
        _ => None,
    };
    if let Some([Some(r), Some(g), Some(b), Some(a)]) = parsed {
        [r, g, b, a]
    } else {
        [0xFF, 0x3B, 0x30, 0xFF]
    }
}

pub fn format_color(rgba: [u8; 4]) -> String {
    let [r, g, b, a] = rgba;
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_roundtrip() {
        assert_eq!(parse_color("#FF3B30"), [0xFF, 0x3B, 0x30, 0xFF]);
        assert_eq!(parse_color("ff3b30"), [0xFF, 0x3B, 0x30, 0xFF]);
        assert_eq!(parse_color("#11223344"), [0x11, 0x22, 0x33, 0x44]);
        assert_eq!(parse_color("banana"), [0xFF, 0x3B, 0x30, 0xFF]);
        assert_eq!(format_color([0xFF, 0x3B, 0x30, 0xFF]), "#FF3B30");
    }

    #[test]
    fn config_defaults_survive_partial_json() {
        let cfg: Config = serde_json::from_str(r#"{ "output_dir": "C:\\x" }"#).unwrap();
        assert_eq!(cfg.output_dir, "C:\\x");
        assert_eq!(cfg.filename_template, "screenshot_{date}_{time}");
        assert_eq!(cfg.hotkeys.fullscreen.code, "PrintScreen");
    }

    #[test]
    fn old_config_with_image_format_still_loads() {
        // Configs da v1.0 traziam "image_format"; o campo é ignorado hoje.
        let cfg: Config =
            serde_json::from_str(r#"{ "image_format": "png", "output_dir": "C:\\y" }"#).unwrap();
        assert_eq!(cfg.output_dir, "C:\\y");
    }
}
