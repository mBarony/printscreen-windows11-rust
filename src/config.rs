//! Carga, validação e persistência do `config.json` (§6 da especificação),
//! serializado com o módulo próprio `json` (sem `serde`).
//!
//! Localização: todo o estado (`config.json` + `rustshot.log`) fica **na
//! mesma pasta do executável** — a aplicação é portátil por definição;
//! desinstalar = apagar a pasta.
//!
//! Leitura tolerante: campos ausentes assumem o padrão e campos desconhecidos
//! (ex.: `image_format` de versões antigas) são ignorados; JSON corrompido é
//! renomeado para `config.json.bak` e um novo arquivo é criado com padrões.

use std::path::{Path, PathBuf};

use crate::error::{Context as _, Result};
use crate::json::{self, Value};
use crate::platform::folders;

pub const APP_NAME: &str = "RustShot";

// ---------------------------------------------------------------------------
// Modelo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    fn as_str(self) -> &'static str {
        match self {
            Self::AllMonitors => "all_monitors",
            Self::Primary => "primary",
            Self::MonitorUnderCursor => "monitor_under_cursor",
        }
    }

    fn from_str(text: &str) -> Option<Self> {
        match text {
            "all_monitors" => Some(Self::AllMonitors),
            "primary" => Some(Self::Primary),
            "monitor_under_cursor" => Some(Self::MonitorUnderCursor),
            _ => None,
        }
    }
}

/// Um atalho serializado; `modifiers` usa `CTRL`/`SHIFT`/`ALT`/`WIN` e
/// `code` os nomes de tecla do padrão W3C (`PrintScreen`, `KeyA`, `F5`…) —
/// formato idêntico ao das versões anteriores (§6).
#[derive(Debug, Clone, PartialEq, Eq)]
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

    fn from_json(value: &Value, default: &HotkeyDef) -> HotkeyDef {
        let modifiers = value
            .get("modifiers")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| default.modifiers.clone());
        let code = value
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default.code.clone());
        HotkeyDef { modifiers, code }
    }

    fn to_json(&self) -> Value {
        json::obj(vec![
            (
                "modifiers",
                json::arr(self.modifiers.iter().map(|m| json::s(m)).collect()),
            ),
            ("code", json::s(&self.code)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct EditorConfig {
    /// Cor padrão das anotações, `#RRGGBB` ou `#RRGGBBAA`.
    pub default_color: String,
    pub default_stroke_width: f32,
    pub default_font_size: f32,
    /// Teclas das ferramentas do editor (issue #4).
    pub tool_keys: ToolKeysConfig,
    /// O que o Ctrl+roda ajusta no canvas (issue #4).
    pub ctrl_wheel: CtrlWheel,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            default_color: "#FF3B30".into(),
            default_stroke_width: 3.0,
            default_font_size: 24.0,
            tool_keys: ToolKeysConfig::default(),
            ctrl_wheel: CtrlWheel::default(),
        }
    }
}

/// Uma letra (A–Z) por ferramenta do editor; inválida cai no padrão da
/// ferramenta ao abrir o editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolKeysConfig {
    pub select: String,
    pub line: String,
    pub arrow: String,
    pub rect: String,
    pub ellipse: String,
    pub freehand: String,
    pub highlighter: String,
    pub marker: String,
    pub eyedropper: String,
    pub text: String,
    pub crop: String,
}

impl Default for ToolKeysConfig {
    fn default() -> Self {
        Self {
            select: "M".into(),
            line: "L".into(),
            arrow: "S".into(),
            rect: "R".into(),
            ellipse: "E".into(),
            freehand: "F".into(),
            highlighter: "H".into(),
            marker: "N".into(),
            eyedropper: "I".into(),
            text: "T".into(),
            crop: "C".into(),
        }
    }
}

/// Papel da roda do mouse no canvas do editor: o Ctrl+roda ajusta uma das
/// funções e a roda pura fica com a outra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CtrlWheel {
    /// Ctrl+roda ajusta o traço/fonte; a roda pura dá zoom.
    #[default]
    StrokeFont,
    /// Ctrl+roda dá zoom; a roda pura ajusta o traço/fonte.
    Zoom,
}

impl CtrlWheel {
    pub fn label(self) -> &'static str {
        match self {
            Self::StrokeFont => "Traço/fonte (roda pura dá zoom)",
            Self::Zoom => "Zoom (roda pura ajusta o traço/fonte)",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::StrokeFont => "stroke_font",
            Self::Zoom => "zoom",
        }
    }

    fn from_str(text: &str) -> Option<Self> {
        match text {
            "stroke_font" => Some(Self::StrokeFont),
            "zoom" => Some(Self::Zoom),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// (De)serialização tolerante
// ---------------------------------------------------------------------------

impl Config {
    /// Constrói a partir do JSON; o texto precisa ser um objeto JSON válido
    /// (qualquer outra coisa conta como corrompido), mas cada campo ausente
    /// ou com tipo errado cai no padrão individualmente.
    pub fn from_json_text(text: &str) -> std::result::Result<Config, String> {
        let root = json::parse(text)?;
        if !matches!(root, Value::Object(_)) {
            return Err("raiz do config.json não é um objeto".into());
        }
        let defaults = Config::default();

        let str_field = |key: &str, fallback: &str| -> String {
            root.get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| fallback.to_string())
        };
        let f32_field = |value: Option<&Value>, fallback: f32| -> f32 {
            value.and_then(Value::as_f64).map(|n| n as f32).unwrap_or(fallback)
        };

        let hotkeys_value = root.get("hotkeys");
        let hotkeys = HotkeysConfig {
            fullscreen: hotkeys_value
                .and_then(|h| h.get("fullscreen"))
                .map(|v| HotkeyDef::from_json(v, &defaults.hotkeys.fullscreen))
                .unwrap_or_else(|| defaults.hotkeys.fullscreen.clone()),
            region: hotkeys_value
                .and_then(|h| h.get("region"))
                .map(|v| HotkeyDef::from_json(v, &defaults.hotkeys.region))
                .unwrap_or_else(|| defaults.hotkeys.region.clone()),
            edit: hotkeys_value
                .and_then(|h| h.get("edit"))
                .map(|v| HotkeyDef::from_json(v, &defaults.hotkeys.edit))
                .unwrap_or_else(|| defaults.hotkeys.edit.clone()),
        };

        let editor_value = root.get("editor");
        let tool_keys_value = editor_value.and_then(|e| e.get("tool_keys"));
        let tool_key = |key: &str, fallback: &str| -> String {
            tool_keys_value
                .and_then(|t| t.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| fallback.to_string())
        };
        let editor = EditorConfig {
            default_color: editor_value
                .and_then(|e| e.get("default_color"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| defaults.editor.default_color.clone()),
            default_stroke_width: f32_field(
                editor_value.and_then(|e| e.get("default_stroke_width")),
                defaults.editor.default_stroke_width,
            ),
            default_font_size: f32_field(
                editor_value.and_then(|e| e.get("default_font_size")),
                defaults.editor.default_font_size,
            ),
            tool_keys: ToolKeysConfig {
                select: tool_key("select", &defaults.editor.tool_keys.select),
                line: tool_key("line", &defaults.editor.tool_keys.line),
                arrow: tool_key("arrow", &defaults.editor.tool_keys.arrow),
                rect: tool_key("rect", &defaults.editor.tool_keys.rect),
                ellipse: tool_key("ellipse", &defaults.editor.tool_keys.ellipse),
                freehand: tool_key("freehand", &defaults.editor.tool_keys.freehand),
                highlighter: tool_key("highlighter", &defaults.editor.tool_keys.highlighter),
                marker: tool_key("marker", &defaults.editor.tool_keys.marker),
                eyedropper: tool_key("eyedropper", &defaults.editor.tool_keys.eyedropper),
                text: tool_key("text", &defaults.editor.tool_keys.text),
                crop: tool_key("crop", &defaults.editor.tool_keys.crop),
            },
            ctrl_wheel: editor_value
                .and_then(|e| e.get("ctrl_wheel"))
                .and_then(Value::as_str)
                .and_then(CtrlWheel::from_str)
                .unwrap_or(defaults.editor.ctrl_wheel),
        };

        Ok(Config {
            version: root
                .get("version")
                .and_then(Value::as_f64)
                .map(|n| n as u32)
                .unwrap_or(defaults.version),
            output_dir: str_field("output_dir", &defaults.output_dir),
            filename_template: str_field("filename_template", &defaults.filename_template),
            fullscreen_scope: root
                .get("fullscreen_scope")
                .and_then(Value::as_str)
                .and_then(FullscreenScope::from_str)
                .unwrap_or(defaults.fullscreen_scope),
            hotkeys,
            editor,
            start_with_windows: root
                .get("start_with_windows")
                .and_then(Value::as_bool)
                .unwrap_or(defaults.start_with_windows),
        })
    }

    pub fn to_json_text(&self) -> String {
        let value = json::obj(vec![
            ("version", json::n(self.version as f64)),
            ("output_dir", json::s(&self.output_dir)),
            ("filename_template", json::s(&self.filename_template)),
            ("fullscreen_scope", json::s(self.fullscreen_scope.as_str())),
            (
                "hotkeys",
                json::obj(vec![
                    ("fullscreen", self.hotkeys.fullscreen.to_json()),
                    ("region", self.hotkeys.region.to_json()),
                    ("edit", self.hotkeys.edit.to_json()),
                ]),
            ),
            (
                "editor",
                json::obj(vec![
                    ("default_color", json::s(&self.editor.default_color)),
                    (
                        "default_stroke_width",
                        json::n(self.editor.default_stroke_width as f64),
                    ),
                    ("default_font_size", json::n(self.editor.default_font_size as f64)),
                    (
                        "tool_keys",
                        json::obj(vec![
                            ("select", json::s(&self.editor.tool_keys.select)),
                            ("line", json::s(&self.editor.tool_keys.line)),
                            ("arrow", json::s(&self.editor.tool_keys.arrow)),
                            ("rect", json::s(&self.editor.tool_keys.rect)),
                            ("ellipse", json::s(&self.editor.tool_keys.ellipse)),
                            ("freehand", json::s(&self.editor.tool_keys.freehand)),
                            ("highlighter", json::s(&self.editor.tool_keys.highlighter)),
                            ("marker", json::s(&self.editor.tool_keys.marker)),
                            ("eyedropper", json::s(&self.editor.tool_keys.eyedropper)),
                            ("text", json::s(&self.editor.tool_keys.text)),
                            ("crop", json::s(&self.editor.tool_keys.crop)),
                        ]),
                    ),
                    ("ctrl_wheel", json::s(self.editor.ctrl_wheel.as_str())),
                ]),
            ),
            ("start_with_windows", json::b(self.start_with_windows)),
        ]);
        json::to_string_pretty(&value)
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
    match std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&probe) {
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
    folders::pictures_dir()
        .unwrap_or_else(std::env::temp_dir)
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
        Ok(text) => match Config::from_json_text(&text) {
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
    let json = config.to_json_text();
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
        let cfg = Config::from_json_text(r#"{ "output_dir": "C:\\x" }"#).unwrap();
        assert_eq!(cfg.output_dir, "C:\\x");
        assert_eq!(cfg.filename_template, "screenshot_{date}_{time}");
        assert_eq!(cfg.hotkeys.fullscreen.code, "PrintScreen");
        assert_eq!(cfg.editor.default_stroke_width, 3.0);
    }

    #[test]
    fn old_config_with_image_format_still_loads() {
        // Configs da v1.0 traziam "image_format"; o campo é ignorado hoje.
        let cfg = Config::from_json_text(r#"{ "image_format": "png", "output_dir": "C:\\y" }"#)
            .unwrap();
        assert_eq!(cfg.output_dir, "C:\\y");
    }

    #[test]
    fn roundtrip_preserves_everything() {
        let cfg = Config {
            output_dir: "D:\\Capturas".into(),
            fullscreen_scope: FullscreenScope::MonitorUnderCursor,
            hotkeys: HotkeysConfig {
                region: HotkeyDef::new(&["CTRL", "ALT"], "KeyR"),
                ..HotkeysConfig::default()
            },
            editor: EditorConfig {
                default_color: "#00FF00".into(),
                default_stroke_width: 5.0,
                tool_keys: ToolKeysConfig { line: "Q".into(), ..ToolKeysConfig::default() },
                ctrl_wheel: CtrlWheel::Zoom,
                ..EditorConfig::default()
            },
            start_with_windows: true,
            ..Config::default()
        };

        let text = cfg.to_json_text();
        let reparsed = Config::from_json_text(&text).unwrap();
        assert_eq!(reparsed, cfg);
    }

    #[test]
    fn tool_keys_absent_or_partial_fall_back_to_defaults() {
        // Config anterior à issue #4: sem "tool_keys" nem "ctrl_wheel".
        let cfg = Config::from_json_text(r#"{ "editor": { "default_font_size": 30 } }"#).unwrap();
        assert_eq!(cfg.editor.tool_keys, ToolKeysConfig::default());
        assert_eq!(cfg.editor.ctrl_wheel, CtrlWheel::StrokeFont);

        // Parcial: só uma tecla trocada; "ctrl_wheel" com valor desconhecido.
        let cfg = Config::from_json_text(
            r#"{ "editor": { "tool_keys": { "rect": "B" }, "ctrl_wheel": "banana" } }"#,
        )
        .unwrap();
        assert_eq!(cfg.editor.tool_keys.rect, "B");
        assert_eq!(cfg.editor.tool_keys.line, "L");
        assert_eq!(cfg.editor.ctrl_wheel, CtrlWheel::StrokeFont);
    }

    #[test]
    fn garbage_root_is_rejected() {
        assert!(Config::from_json_text("[1, 2, 3]").is_err());
        assert!(Config::from_json_text("{ truncado").is_err());
    }

    #[test]
    fn wrong_types_fall_back_to_defaults() {
        let cfg = Config::from_json_text(
            r#"{ "output_dir": 42, "start_with_windows": "sim", "editor": { "default_font_size": "grande" } }"#,
        )
        .unwrap();
        assert_eq!(cfg.output_dir, "");
        assert!(!cfg.start_with_windows);
        assert_eq!(cfg.editor.default_font_size, 24.0);
    }
}
