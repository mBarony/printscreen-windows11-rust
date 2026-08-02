//! Modelo de dados das formas do editor e pilha de undo (§8).
//!
//! As formas são armazenadas **em coordenadas do espaço da imagem** (pixels
//! físicos da captura) e convertidas para o espaço de tela apenas na
//! exibição — assim zoom e exportação nunca divergem. A geometria derivada
//! (ponta de seta, restrições com Shift) vive aqui e é compartilhada entre a
//! pré-visualização (egui) e a rasterização final (tiny-skia).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Line,
    Arrow,
    Rect,
    Ellipse,
    Text,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Linha",
            Self::Arrow => "Seta",
            Self::Rect => "Retângulo",
            Self::Ellipse => "Elipse",
            Self::Text => "Texto",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// RGBA não pré-multiplicado.
    pub color: [u8; 4],
    /// Espessura do traço, em px no espaço da imagem.
    pub stroke_width: f32,
    /// Tamanho da fonte, em px no espaço da imagem (ferramenta Texto).
    pub font_size: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line { a: Point, b: Point, style: Style },
    Arrow { a: Point, b: Point, style: Style },
    Rect { min: Point, max: Point, style: Style },
    Ellipse { center: Point, rx: f32, ry: f32, style: Style },
    Text { anchor: Point, content: String, style: Style },
}

// ---------------------------------------------------------------------------
// Construção a partir do arrasto (com restrições do Shift)
// ---------------------------------------------------------------------------

/// Constrói a forma resultante de um arrasto `a → b` com a ferramenta dada.
/// `shift` restringe: linha/seta em ângulos de 45°, retângulo em quadrado,
/// elipse em círculo. Retorna `None` para a ferramenta Texto (fluxo próprio).
pub fn shape_from_drag(tool: Tool, a: Point, mut b: Point, shift: bool, style: Style) -> Option<Shape> {
    match tool {
        Tool::Line | Tool::Arrow => {
            if shift {
                b = snap_45(a, b);
            }
            Some(if tool == Tool::Line {
                Shape::Line { a, b, style }
            } else {
                Shape::Arrow { a, b, style }
            })
        }
        Tool::Rect => {
            if shift {
                b = snap_square(a, b);
            }
            let (min, max) = normalize(a, b);
            Some(Shape::Rect { min, max, style })
        }
        Tool::Ellipse => {
            if shift {
                b = snap_square(a, b);
            }
            let (min, max) = normalize(a, b);
            let rx = (max.x - min.x) / 2.0;
            let ry = (max.y - min.y) / 2.0;
            Some(Shape::Ellipse {
                center: Point::new(min.x + rx, min.y + ry),
                rx,
                ry,
                style,
            })
        }
        Tool::Text => None,
    }
}

/// Restringe `b` ao ângulo múltiplo de 45° mais próximo em relação a `a`.
fn snap_45(a: Point, b: Point) -> Point {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < f32::EPSILON {
        return b;
    }
    let angle = dy.atan2(dx);
    let step = std::f32::consts::FRAC_PI_4; // 45°
    let snapped = (angle / step).round() * step;
    Point::new(a.x + len * snapped.cos(), a.y + len * snapped.sin())
}

/// Restringe o canto oposto para formar um quadrado (mantendo a direção).
fn snap_square(a: Point, b: Point) -> Point {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let side = dx.abs().max(dy.abs());
    Point::new(a.x + side * dx.signum(), a.y + side * dy.signum())
}

fn normalize(a: Point, b: Point) -> (Point, Point) {
    (
        Point::new(a.x.min(b.x), a.y.min(b.y)),
        Point::new(a.x.max(b.x), a.y.max(b.y)),
    )
}

// ---------------------------------------------------------------------------
// Geometria da seta (compartilhada entre preview e exportação)
// ---------------------------------------------------------------------------

/// Geometria derivada de uma seta: haste + triângulo preenchido da ponta.
pub struct ArrowGeometry {
    pub shaft_a: Point,
    pub shaft_b: Point,
    /// Vértices do triângulo da ponta: [ponta, base esquerda, base direita].
    pub head: [Point; 3],
}

/// Ponta da seta: triângulo preenchido com comprimento `max(10, 4×espessura)`
/// px e abertura de 30° (§8).
pub fn arrow_geometry(a: Point, b: Point, stroke_width: f32) -> ArrowGeometry {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    let (ux, uy) = (dx / len, dy / len);

    let head_len = (4.0 * stroke_width).max(10.0).min(len);
    let half_angle = 15f32.to_radians(); // abertura total de 30°
    let half_width = head_len * half_angle.tan();

    // Base do triângulo (ponto sobre a haste, atrás da ponta).
    let base = Point::new(b.x - ux * head_len, b.y - uy * head_len);
    // Perpendicular unitária.
    let (px, py) = (-uy, ux);

    ArrowGeometry {
        shaft_a: a,
        // Haste recuada até a base para o traço não vazar além do triângulo.
        shaft_b: base,
        head: [
            b,
            Point::new(base.x + px * half_width, base.y + py * half_width),
            Point::new(base.x - px * half_width, base.y - py * half_width),
        ],
    }
}

// ---------------------------------------------------------------------------
// Undo / redo — pilha simples (formas não são editáveis após criadas, §8)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ShapeStack {
    shapes: Vec<Shape>,
    redo: Vec<Shape>,
}

impl ShapeStack {
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Criar uma forma nova limpa a pilha de refazer (§8).
    pub fn push(&mut self, shape: Shape) {
        self.shapes.push(shape);
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.shapes.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(shape) = self.shapes.pop() {
            self.redo.push(shape);
        }
    }

    pub fn redo(&mut self) {
        if let Some(shape) = self.redo.pop() {
            self.shapes.push(shape);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style { color: [255, 0, 0, 255], stroke_width: 3.0, font_size: 24.0 }
    }

    #[test]
    fn shift_makes_square() {
        let s = shape_from_drag(
            Tool::Rect,
            Point::new(10.0, 10.0),
            Point::new(50.0, 20.0),
            true,
            style(),
        )
        .unwrap();
        match s {
            Shape::Rect { min, max, .. } => {
                assert_eq!(max.x - min.x, max.y - min.y);
            }
            _ => panic!("esperava Rect"),
        }
    }

    #[test]
    fn shift_snaps_line_to_45() {
        let s = shape_from_drag(
            Tool::Line,
            Point::new(0.0, 0.0),
            Point::new(100.0, 8.0),
            true,
            style(),
        )
        .unwrap();
        match s {
            Shape::Line { b, .. } => {
                assert!(b.y.abs() < 0.01, "linha quase horizontal deve virar horizontal");
            }
            _ => panic!("esperava Line"),
        }
    }

    #[test]
    fn arrow_head_minimum_length() {
        let g = arrow_geometry(Point::new(0.0, 0.0), Point::new(100.0, 0.0), 1.0);
        let head_len = 100.0 - g.shaft_b.x;
        assert!((head_len - 10.0).abs() < 0.01, "min 10 px, obtido {head_len}");
    }

    #[test]
    fn undo_redo_cycle() {
        let mut stack = ShapeStack::default();
        let line = shape_from_drag(
            Tool::Line,
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            false,
            style(),
        )
        .unwrap();
        stack.push(line.clone());
        assert!(stack.can_undo());
        stack.undo();
        assert!(stack.is_empty() && stack.can_redo());
        stack.redo();
        assert_eq!(stack.shapes(), std::slice::from_ref(&line));
        // Nova forma limpa o redo.
        stack.undo();
        stack.push(line);
        assert!(!stack.can_redo());
    }
}
