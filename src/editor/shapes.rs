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
    /// Seleciona e reposiciona anotações já criadas (issue #2).
    Select,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Text,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Mover",
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

impl Shape {
    /// Desloca a forma inteira por `(dx, dy)`, em px da imagem (issue #2).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        let mv = |p: &mut Point| {
            p.x += dx;
            p.y += dy;
        };
        match self {
            Self::Line { a, b, .. } | Self::Arrow { a, b, .. } => {
                mv(a);
                mv(b);
            }
            Self::Rect { min, max, .. } => {
                mv(min);
                mv(max);
            }
            Self::Ellipse { center, .. } => mv(center),
            Self::Text { anchor, .. } => mv(anchor),
        }
    }

    /// `p` está a até `tol` px (espaço da imagem) do traço da forma?
    ///
    /// `text_size` é o tamanho `(largura, altura)` do texto renderizado —
    /// medido pelo chamador, que tem acesso à fonte — e só é usado na
    /// variante `Text` (caixa `anchor..anchor+text_size` expandida por `tol`).
    pub fn hit_test(&self, p: Point, tol: f32, text_size: (f32, f32)) -> bool {
        match self {
            Self::Line { a, b, style } => {
                dist_to_segment(p, *a, *b) <= tol + style.stroke_width / 2.0
            }
            Self::Arrow { a, b, style } => {
                let geo = arrow_geometry(*a, *b, style.stroke_width);
                dist_to_segment(p, geo.shaft_a, geo.shaft_b) <= tol + style.stroke_width / 2.0
                    || point_in_triangle(p, geo.head)
                    || (0..3).any(|i| dist_to_segment(p, geo.head[i], geo.head[(i + 1) % 3]) <= tol)
            }
            Self::Rect { min, max, style } => {
                let corners = [*min, Point::new(max.x, min.y), *max, Point::new(min.x, max.y)];
                let reach = tol + style.stroke_width / 2.0;
                (0..4).any(|i| dist_to_segment(p, corners[i], corners[(i + 1) % 4]) <= reach)
            }
            Self::Ellipse { center, rx, ry, style } => {
                // Contorno amostrado em segmentos: robusto inclusive para
                // elipses degeneradas (um arrasto quase-linear cria rx≈0 ou
                // ry≈0, cujo traço é um segmento).
                const N: usize = 32;
                let reach = tol + style.stroke_width / 2.0;
                let pt = |i: usize| {
                    let a = i as f32 / N as f32 * std::f32::consts::TAU;
                    Point::new(center.x + rx * a.cos(), center.y + ry * a.sin())
                };
                (0..N).any(|i| dist_to_segment(p, pt(i), pt(i + 1)) <= reach)
            }
            Self::Text { anchor, .. } => {
                let (w, h) = text_size;
                p.x >= anchor.x - tol
                    && p.x <= anchor.x + w + tol
                    && p.y >= anchor.y - tol
                    && p.y <= anchor.y + h + tol
            }
        }
    }
}

fn dist(p: Point, q: Point) -> f32 {
    let (dx, dy) = (p.x - q.x, p.y - q.y);
    (dx * dx + dy * dy).sqrt()
}

/// Distância de `p` ao segmento `a`–`b`.
fn dist_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0)
    };
    dist(p, Point::new(a.x + t * dx, a.y + t * dy))
}

fn point_in_triangle(p: Point, t: [Point; 3]) -> bool {
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let area = cross(t[0], t[1], t[2]);
    if area.abs() <= f32::EPSILON {
        return false;
    }
    let flip = area.signum();
    cross(t[0], t[1], p) * flip >= 0.0
        && cross(t[1], t[2], p) * flip >= 0.0
        && cross(t[2], t[0], p) * flip >= 0.0
}

// ---------------------------------------------------------------------------
// Construção a partir do arrasto (com restrições do Shift)
// ---------------------------------------------------------------------------

/// Constrói a forma resultante de um arrasto `a → b` com a ferramenta dada.
/// `shift` restringe: linha/seta em ângulos de 45°, retângulo em quadrado,
/// elipse em círculo. Retorna `None` para as ferramentas Texto e Mover
/// (fluxos próprios).
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
        Tool::Text | Tool::Select => None,
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
// Undo / redo — snapshots do conjunto de formas
// ---------------------------------------------------------------------------

/// Pilha de undo/redo por snapshots: cada edição (criar uma forma ou
/// reposicioná-la com a ferramenta Mover, issue #2) registra o estado
/// anterior inteiro. Snapshots são baratos — uma sessão tem no máximo
/// dezenas de anotações — e cobrem qualquer tipo de edição uniformemente.
#[derive(Default)]
pub struct ShapeStack {
    shapes: Vec<Shape>,
    undo: Vec<Vec<Shape>>,
    redo: Vec<Vec<Shape>>,
    /// Snapshot do movimento em andamento — só entra no histórico quando o
    /// arrasto se confirma (`end_move`); um clique parado (`abort_move`)
    /// não toca nem no undo nem no redo.
    moving: Option<Vec<Shape>>,
}

impl ShapeStack {
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Registra o estado atual como ponto de desfazer (e, como toda edição
    /// nova, limpa a pilha de refazer, §8).
    fn checkpoint(&mut self) {
        self.undo.push(self.shapes.clone());
        self.redo.clear();
    }

    pub fn push(&mut self, shape: Shape) {
        self.checkpoint();
        self.shapes.push(shape);
    }

    /// Início de um arrasto de reposicionamento: guarda o estado atual fora
    /// do histórico — ele só vira ponto de desfazer se o movimento se
    /// confirmar (`end_move`).
    pub fn begin_move(&mut self) {
        self.moving = Some(self.shapes.clone());
    }

    /// Deslocamento incremental da forma `index` durante o arrasto.
    pub fn translate(&mut self, index: usize, dx: f32, dy: f32) {
        if let Some(shape) = self.shapes.get_mut(index) {
            shape.translate(dx, dy);
        }
    }

    /// Confirma o movimento: o estado pré-arrasto entra no undo e, como toda
    /// edição nova, a pilha de refazer é limpa (§8).
    pub fn end_move(&mut self) {
        if let Some(prev) = self.moving.take() {
            self.undo.push(prev);
            self.redo.clear();
        }
    }

    /// Descarta o movimento em andamento (clique parado, troca de ferramenta,
    /// Esc): restaura o estado pré-arrasto sem tocar no undo nem no redo.
    pub fn abort_move(&mut self) {
        if let Some(prev) = self.moving.take() {
            self.shapes = prev;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.shapes, prev));
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.shapes, next));
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
    fn translate_moves_every_variant() {
        let mut rect = Shape::Rect {
            min: Point::new(10.0, 10.0),
            max: Point::new(20.0, 30.0),
            style: style(),
        };
        rect.translate(5.0, -3.0);
        match rect {
            Shape::Rect { min, max, .. } => {
                assert_eq!((min.x, min.y), (15.0, 7.0));
                assert_eq!((max.x, max.y), (25.0, 27.0));
            }
            _ => unreachable!(),
        }

        let mut text = Shape::Text {
            anchor: Point::new(1.0, 2.0),
            content: "oi".into(),
            style: style(),
        };
        text.translate(-1.0, -2.0);
        match text {
            Shape::Text { anchor, .. } => assert_eq!((anchor.x, anchor.y), (0.0, 0.0)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn hit_test_line_edge_and_miss() {
        let line = Shape::Line {
            a: Point::new(0.0, 0.0),
            b: Point::new(100.0, 0.0),
            style: style(), // traço 3 → meia-espessura 1.5
        };
        assert!(line.hit_test(Point::new(50.0, 3.0), 2.0, (0.0, 0.0)));
        assert!(!line.hit_test(Point::new(50.0, 10.0), 2.0, (0.0, 0.0)));
    }

    #[test]
    fn hit_test_rect_border_not_interior() {
        let rect = Shape::Rect {
            min: Point::new(10.0, 10.0),
            max: Point::new(50.0, 50.0),
            style: style(),
        };
        assert!(rect.hit_test(Point::new(10.0, 30.0), 2.0, (0.0, 0.0)), "borda esquerda");
        assert!(!rect.hit_test(Point::new(30.0, 30.0), 2.0, (0.0, 0.0)), "miolo vazio");
    }

    #[test]
    fn hit_test_ellipse_ring_not_center() {
        let ellipse = Shape::Ellipse {
            center: Point::new(50.0, 50.0),
            rx: 20.0,
            ry: 10.0,
            style: style(),
        };
        assert!(ellipse.hit_test(Point::new(70.0, 50.0), 2.0, (0.0, 0.0)), "contorno em rx");
        assert!(!ellipse.hit_test(Point::new(50.0, 50.0), 2.0, (0.0, 0.0)), "centro vazio");
    }

    #[test]
    fn hit_test_arrow_head() {
        let arrow = Shape::Arrow {
            a: Point::new(0.0, 50.0),
            b: Point::new(100.0, 50.0),
            style: style(),
        };
        // Dentro do triângulo da ponta (comprimento 12 = 4×3).
        assert!(arrow.hit_test(Point::new(95.0, 50.0), 0.0, (0.0, 0.0)));
        assert!(!arrow.hit_test(Point::new(95.0, 60.0), 2.0, (0.0, 0.0)));
    }

    #[test]
    fn hit_test_text_uses_measured_box() {
        let text = Shape::Text {
            anchor: Point::new(10.0, 10.0),
            content: "abc".into(),
            style: style(),
        };
        assert!(text.hit_test(Point::new(30.0, 20.0), 2.0, (40.0, 16.0)));
        assert!(!text.hit_test(Point::new(60.0, 40.0), 2.0, (40.0, 16.0)));
    }

    #[test]
    fn hit_test_degenerate_ellipse_is_reachable() {
        // Arrasto vertical puro cria rx = 0: o traço é um segmento vertical.
        let ellipse = Shape::Ellipse {
            center: Point::new(50.0, 50.0),
            rx: 0.0,
            ry: 30.0,
            style: style(),
        };
        assert!(ellipse.hit_test(Point::new(50.0, 65.0), 2.0, (0.0, 0.0)), "sobre o traço");
        assert!(!ellipse.hit_test(Point::new(60.0, 50.0), 2.0, (0.0, 0.0)), "longe do traço");
    }

    #[test]
    fn move_undo_redo_and_abort() {
        let mut stack = ShapeStack::default();
        stack.push(Shape::Rect {
            min: Point::new(0.0, 0.0),
            max: Point::new(10.0, 10.0),
            style: style(),
        });

        // Movimento real: um único ponto de undo para o arrasto inteiro.
        stack.begin_move();
        stack.translate(0, 3.0, 0.0);
        stack.translate(0, 2.0, 4.0);
        stack.end_move();
        let moved = stack.shapes()[0].clone();
        match &moved {
            Shape::Rect { min, .. } => assert_eq!((min.x, min.y), (5.0, 4.0)),
            _ => unreachable!(),
        }

        stack.undo();
        match &stack.shapes()[0] {
            Shape::Rect { min, .. } => assert_eq!((min.x, min.y), (0.0, 0.0)),
            _ => unreachable!(),
        }
        stack.redo();
        assert_eq!(stack.shapes()[0], moved);

        // Movimento abortado no meio: posição restaurada, histórico intacto.
        stack.begin_move();
        stack.translate(0, 100.0, 100.0);
        stack.abort_move();
        assert_eq!(stack.shapes()[0], moved);

        stack.undo(); // desfaz o movimento
        stack.undo(); // desfaz a criação
        assert!(stack.is_empty() && !stack.can_undo());
    }

    #[test]
    fn select_click_preserves_redo() {
        let mut stack = ShapeStack::default();
        let line = shape_from_drag(
            Tool::Line,
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            false,
            style(),
        )
        .unwrap();
        stack.push(line.clone());
        stack.push(line);
        stack.undo(); // segunda forma vai para o redo
        assert!(stack.can_redo());

        // Clique parado com a ferramenta Mover (selecionar sem arrastar):
        // não é edição — o refazer precisa sobreviver.
        stack.begin_move();
        stack.abort_move();
        assert!(stack.can_redo(), "clique de seleção não pode destruir o redo");
        stack.redo();
        assert_eq!(stack.shapes().len(), 2);
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
