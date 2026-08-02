//! Carga, validação e persistência do `config.json` (§6 da especificação).
//!
//! Localização: por padrão o arquivo fica em `%APPDATA%\RustShot\config.json`
//! (todo o estado da aplicação vive nessa pasta, §13). Modo portátil: se já
//! existir um `config.json` ao lado do executável, ele tem precedência — crie
//! um arquivo vazio ao lado do exe para optar por esse modo.
//!
//! Leitura tolerante: campos ausentes assumem o padrão; JSON corrompido é
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
    pub image_format: ImageFormat,
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
            image_format: ImageFormat::Png,
            fullscreen_scope: FullscreenScope::AllMonitors,
            hotkeys: HotkeysConfig::default(),
            editor: EditorConfig::default(),
            start_with_windows: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    /// PNG RGBA 8 bits — formato padrão (RF-07).
    Png,
    /// JPG qualidade 90 (aceito por compatibilidade; RF-01 menciona JPG).
    #[serde(alias = "jpeg")]
    Jpg,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
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

/// Pasta de estado da aplicação: `%APPDATA%\RustShot`.
pub fn app_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_NAME)
}

/// Caminho efetivo do `config.json` (modo portátil tem precedência).
pub fn config_path() -> PathBuf {
    if let Some(portable) = portable_config_path() {
        if portable.exists() {
            return portable;
        }
    }
    app_data_dir().join("config.json")
}

fn portable_config_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("config.json"))
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
            // Arquivo vazio (ex.: marcador de modo portátil): recria padrões.
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
        Err(_) => {
            let config = Config::default();
            let _ = save(&config);
            LoadedConfig { config, created: true, recovered: false }
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
        assert_eq!(cfg.image_format, ImageFormat::Png);
        assert_eq!(cfg.hotkeys.fullscreen.code, "PrintScreen");
    }
}
