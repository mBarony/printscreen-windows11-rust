//! Editor de anotações (RF-03/RF-04, §8).
//!
//! Submódulos: `shapes` (modelo de dados das formas), `document` (imagem +
//! anotações + histórico), `ui` (janela/canvas/toolbar) e `render`
//! (rasterização final para exportação).

pub mod document;
pub mod icons;
pub mod raster;
pub mod render;
pub mod shapes;
pub mod ui;

use crate::config::{self, EditorConfig};
use crate::imgbuf::RgbaImage;
use document::Document;
use shapes::{Point, Style, Tool};

/// Fonte TTF embutida (Inter, licença SIL OFL), usada tanto no egui quanto na
/// exportação — garantindo WYSIWYG entre editor e JPG final (§8).
pub static FONT_BYTES: &[u8] = include_bytes!("../../assets/Inter-Regular.ttf");

/// Título da janela do editor — compartilhado entre o `ViewportBuilder`
/// (app.rs) e a busca da janela para forçar o foco (`ui.rs`).
pub const WINDOW_TITLE: &str = "RustShot — Editor";

/// Frames em que o editor insiste em tomar o foco após nascer (§ v1.3.1).
/// A janela surge logo depois de o overlay fechar, quando o Windows já
/// devolveu o primeiro plano para outro app; sem isso o usuário teria de
/// clicar antes de `Ctrl+C`/`Ctrl+S` funcionarem.
pub const FOCUS_CLAIM_FRAMES: u8 = 12;

/// Tolerância do hit-test ao clicar numa anotação com a ferramenta Mover,
/// em pontos do egui (convertida para px da imagem pelo zoom).
pub const HIT_TOLERANCE_PTS: f32 = 6.0;

/// Lado mínimo (px da imagem) de um recorte: abaixo disso o arrasto foi
/// engano, não intenção de recortar.
pub const CROP_MIN_SIDE: f32 = 4.0;

pub const STROKE_MIN: f32 = 1.0;
pub const STROKE_MAX: f32 = 12.0;
pub const FONT_MIN: f32 = 12.0;
pub const FONT_MAX: f32 = 72.0;
pub const ZOOM_MIN: f32 = 0.25;
pub const ZOOM_MAX: f32 = 4.0;

/// Paleta fixa de 8 cores da toolbar (§8).
pub const PALETTE: [[u8; 4]; 8] = [
    [0xFF, 0x3B, 0x30, 0xFF], // vermelho
    [0xFF, 0x95, 0x00, 0xFF], // laranja
    [0xFF, 0xCC, 0x00, 0xFF], // amarelo
    [0x34, 0xC7, 0x59, 0xFF], // verde
    [0x00, 0x7A, 0xFF, 0xFF], // azul
    [0xAF, 0x52, 0xDE, 0xFF], // roxo
    [0x00, 0x00, 0x00, 0xFF], // preto
    [0xFF, 0xFF, 0xFF, 0xFF], // branco
];

/// Arrasto de criação de forma em andamento (coordenadas da imagem).
pub struct DragPreview {
    pub start: Point,
    pub current: Point,
    pub shift: bool,
}

/// Caixa de texto inline da ferramenta Texto.
pub struct TextInput {
    pub anchor: Point,
    pub buffer: String,
    pub focus_requested: bool,
}

/// Arrasto de reposicionamento de uma anotação existente (ferramenta Mover,
/// issue #2).
pub struct MoveDrag {
    /// Índice da forma em `stack.shapes()`.
    pub index: usize,
    /// Última posição do ponteiro, em px da imagem.
    pub last: Point,
    /// Distância acumulada do arrasto, em px da imagem — distingue clique
    /// parado de movimento real.
    pub travel: f32,
}

/// Estado de uma sessão do editor (uma captura aberta para anotação).
pub struct EditorSession {
    /// Identificador estável da sessão (diferencia viewports sucessivos).
    pub serial: u64,
    /// Imagem base + anotações + histórico (o recorte muda a imagem).
    pub doc: Document,
    pub texture: Option<egui::TextureHandle>,
    pub tool: Tool,
    pub color: [u8; 4],
    pub stroke_width: f32,
    pub font_size: f32,
    /// Px físicos da tela por px da imagem; `None` = "ajustar à janela" pendente.
    pub zoom: Option<f32>,
    /// Deslocamento da origem da imagem dentro do canvas, em pontos do egui.
    pub pan: egui::Vec2,
    /// Teclas das ferramentas, já resolvidas do config (issue #4); `None` =
    /// ferramenta sem atalho (a tecla estava tomada por outra).
    pub tool_keys: [(Tool, Option<egui::Key>); 7],
    /// `true` = Ctrl+roda dá zoom e a roda pura ajusta o traço/fonte
    /// (papéis trocados em relação ao padrão, issue #4).
    pub ctrl_wheel_zoom: bool,
    pub drag: Option<DragPreview>,
    pub text_input: Option<TextInput>,
    /// Índice da anotação selecionada (ferramenta Mover).
    pub selected: Option<usize>,
    /// Arrasto de reposicionamento em andamento (ferramenta Mover).
    pub move_drag: Option<MoveDrag>,
    /// Região de recorte já desenhada, aguardando confirmação (issue #5),
    /// em px da imagem — `(canto superior-esquerdo, inferior-direito)`.
    pub crop_pending: Option<(Point, Point)>,
    /// Acumulador do Ctrl+roda (ajuste de traço/fonte): converte tanto os
    /// notches da roda quanto os deltas contínuos de touchpad em passos
    /// discretos, sem varrer a faixa inteira num gesto.
    pub wheel_accum: f32,
    /// Diálogo "descartar anotações?" visível.
    pub confirm_discard: bool,
    /// Frames restantes de tentativa de tomar o foco (zera ao conseguir).
    pub focus_frames: u8,
    /// Sessão terminou (salvou ou descartou); a janela fecha no próximo frame.
    pub finished: bool,
}

impl EditorSession {
    pub fn new(serial: u64, image: RgbaImage, defaults: &EditorConfig) -> Self {
        Self {
            serial,
            doc: Document::new(image),
            texture: None,
            tool: Tool::Arrow,
            color: config::parse_color(&defaults.default_color),
            stroke_width: defaults.default_stroke_width.clamp(STROKE_MIN, STROKE_MAX),
            font_size: defaults.default_font_size.clamp(FONT_MIN, FONT_MAX),
            zoom: None,
            pan: egui::Vec2::ZERO,
            tool_keys: resolve_tool_keys(&defaults.tool_keys),
            ctrl_wheel_zoom: defaults.ctrl_wheel == config::CtrlWheel::Zoom,
            drag: None,
            text_input: None,
            selected: None,
            move_drag: None,
            crop_pending: None,
            wheel_accum: 0.0,
            confirm_discard: false,
            focus_frames: FOCUS_CLAIM_FRAMES,
            finished: false,
        }
    }

    pub fn style(&self) -> Style {
        Style {
            color: self.color,
            stroke_width: self.stroke_width,
            font_size: self.font_size,
        }
    }

    /// Há edições que seriam perdidas ao fechar sem salvar? Qualquer ponto
    /// no histórico conta — anotações e também recortes (issue #5); desfazer
    /// tudo devolve a sessão ao estado original e limpa a pendência.
    pub fn dirty(&self) -> bool {
        self.doc.can_undo() || self.text_input.is_some()
    }
}

/// Resolve as teclas configuradas das ferramentas (issue #4): uma letra A–Z
/// em qualquer caixa; valor inválido cai no padrão da ferramenta. Teclas já
/// tomadas por uma ferramenta anterior não são reatribuídas — a ferramenta
/// fica sem atalho (`None`) em vez de disputar a tecla (config editado à
/// mão pode criar duplicatas que a UI de Configurações não deixa salvar).
pub fn resolve_tool_keys(config: &config::ToolKeysConfig) -> [(Tool, Option<egui::Key>); 7] {
    let defaults = config::ToolKeysConfig::default();
    let entries = [
        (Tool::Select, &config.select, &defaults.select),
        (Tool::Line, &config.line, &defaults.line),
        (Tool::Arrow, &config.arrow, &defaults.arrow),
        (Tool::Rect, &config.rect, &defaults.rect),
        (Tool::Ellipse, &config.ellipse, &defaults.ellipse),
        (Tool::Text, &config.text, &defaults.text),
        (Tool::Crop, &config.crop, &defaults.crop),
    ];
    let mut used: Vec<egui::Key> = Vec::new();
    entries.map(|(tool, configured, fallback)| {
        let key = parse_tool_key(configured)
            .filter(|k| !used.contains(k))
            .or_else(|| parse_tool_key(fallback).filter(|k| !used.contains(k)));
        if let Some(k) = key {
            used.push(k);
        }
        (tool, key)
    })
}

/// Uma letra A–Z (qualquer caixa) → tecla do egui.
pub fn parse_tool_key(text: &str) -> Option<egui::Key> {
    let mut chars = text.trim().chars();
    let c = chars.next()?;
    if chars.next().is_some() || !c.is_ascii_alphabetic() {
        return None;
    }
    egui::Key::from_name(&c.to_ascii_uppercase().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ToolKeysConfig;

    #[test]
    fn resolve_tool_keys_defaults() {
        let keys = resolve_tool_keys(&ToolKeysConfig::default());
        assert_eq!(keys[0], (Tool::Select, Some(egui::Key::M)));
        assert_eq!(keys[5], (Tool::Text, Some(egui::Key::T)));
        assert_eq!(keys[6], (Tool::Crop, Some(egui::Key::C)));
    }

    #[test]
    fn resolve_tool_keys_accepts_lowercase_and_rejects_garbage() {
        let config = ToolKeysConfig {
            line: "q".into(),      // caixa baixa vale
            arrow: "F5".into(),    // mais de um caractere → padrão (S)
            rect: String::new(),   // vazio → padrão (R)
            ..ToolKeysConfig::default()
        };
        let keys = resolve_tool_keys(&config);
        assert_eq!(keys[1], (Tool::Line, Some(egui::Key::Q)));
        assert_eq!(keys[2], (Tool::Arrow, Some(egui::Key::S)));
        assert_eq!(keys[3], (Tool::Rect, Some(egui::Key::R)));
    }

    #[test]
    fn resolve_tool_keys_never_duplicates() {
        // Config manual: Mover toma "S"; a Seta ("F5" inválida) cairia no
        // padrão "S", já ocupado → fica sem atalho, sem disputar a tecla.
        let config = ToolKeysConfig {
            select: "S".into(),
            arrow: "F5".into(),
            ..ToolKeysConfig::default()
        };
        let keys = resolve_tool_keys(&config);
        assert_eq!(keys[0], (Tool::Select, Some(egui::Key::S)));
        assert_eq!(keys[2], (Tool::Arrow, None));

        // Tecla configurada tomada, mas o padrão da ferramenta está livre:
        // cai no padrão em vez de ficar sem atalho.
        let config = ToolKeysConfig {
            line: "M".into(), // "M" já é do Mover; o padrão da Linha ("L") está livre
            ..ToolKeysConfig::default()
        };
        let keys = resolve_tool_keys(&config);
        assert_eq!(keys[0], (Tool::Select, Some(egui::Key::M)));
        assert_eq!(keys[1], (Tool::Line, Some(egui::Key::L)));
    }
}
