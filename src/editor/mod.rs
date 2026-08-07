//! Editor de anotações (RF-03/RF-04, §8).
//!
//! Submódulos: `shapes` (modelo de dados + undo), `ui` (janela/canvas/
//! toolbar) e `render` (rasterização final para exportação).

pub mod raster;
pub mod render;
pub mod shapes;
pub mod ui;

use std::sync::Arc;

use crate::config::{self, EditorConfig};
use crate::imgbuf::RgbaImage;
use shapes::{Point, ShapeStack, Style, Tool};

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
    /// Índice da anotação selecionada (ferramenta Mover).
    pub selected: Option<usize>,
    /// Arrasto de reposicionamento em andamento (ferramenta Mover).
    pub move_drag: Option<MoveDrag>,
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
            selected: None,
            move_drag: None,
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

    /// Há anotações que seriam perdidas ao fechar sem salvar?
    pub fn dirty(&self) -> bool {
        !self.stack.is_empty() || self.text_input.is_some()
    }
}
