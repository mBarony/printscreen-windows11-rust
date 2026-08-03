//! Editor de anotações (RF-03/RF-04, §8).
//!
//! Submódulos: `shapes` (modelo de dados + undo), `ui` (janela/canvas/
//! toolbar) e `render` (rasterização final para exportação).

pub mod render;
pub mod shapes;
pub mod ui;

use std::sync::Arc;

use image::RgbaImage;

use crate::config::{self, EditorConfig};
use shapes::{Point, ShapeStack, Style, Tool};

/// Fonte TTF embutida (Inter, licença SIL OFL), usada tanto no egui quanto na
/// exportação — garantindo WYSIWYG entre editor e JPG final (§8).
pub static FONT_BYTES: &[u8] = include_bytes!("../../assets/Inter-Regular.ttf");

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

/// Estado de uma sessão do editor (uma captura aberta para anotação).
pub struct EditorSession {
    /// Identificador estável da sessão (diferencia viewports sucessivos).
    pub serial: u64,
    pub image: Arc<RgbaImage>,
    pub texture: Option<egui::TextureHandle>,
    pub stack: ShapeStack,
    pub tool: Tool,
    pub color: [u8; 4],
    pub stroke_width: f32,
    pub font_size: f32,
    /// Px físicos da tela por px da imagem; `None` = "ajustar à janela" pendente.
    pub zoom: Option<f32>,
    /// Deslocamento da origem da imagem dentro do canvas, em pontos do egui.
    pub pan: egui::Vec2,
    pub drag: Option<DragPreview>,
    pub text_input: Option<TextInput>,
    /// Diálogo "descartar anotações?" visível.
    pub confirm_discard: bool,
    /// Sessão terminou (salvou ou descartou); a janela fecha no próximo frame.
    pub finished: bool,
}

impl EditorSession {
    pub fn new(serial: u64, image: RgbaImage, defaults: &EditorConfig) -> Self {
        Self {
            serial,
            image: Arc::new(image),
            texture: None,
            stack: ShapeStack::default(),
            tool: Tool::Arrow,
            color: config::parse_color(&defaults.default_color),
            stroke_width: defaults.default_stroke_width.clamp(STROKE_MIN, STROKE_MAX),
            font_size: defaults.default_font_size.clamp(FONT_MIN, FONT_MAX),
            zoom: None,
            pan: egui::Vec2::ZERO,
            drag: None,
            text_input: None,
            confirm_discard: false,
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

    /// Há anotações que seriam perdidas ao fechar sem salvar?
    pub fn dirty(&self) -> bool {
        !self.stack.is_empty() || self.text_input.is_some()
    }
}
