//! Modelo de dados das formas do editor e sua geometria (§8).
//!
//! As formas são armazenadas **em coordenadas do espaço da imagem** (pixels
//! físicos da captura) e convertidas para o espaço de tela apenas na
//! exibição — assim zoom e exportação nunca divergem. A geometria derivada
//! (ponta de seta, restrições com Shift) vive aqui e é compartilhada entre a
//! pré-visualização (egui) e a rasterização final (`raster`). O conjunto de
//! formas e seu histórico ficam em [`super::document`].

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
    /// Recorta a imagem para a região arrastada (issue #5).
    Crop,
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
            Self::Crop => "Recortar",
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
    Line { a: Point, b: Point },
    Arrow { a: Point, b: Point },
    Rect { min: Point, max: Point },
    Ellipse { center: Point, rx: f32, ry: f32 },
    Text { anchor: Point, content: String },
}

/// Uma anotação no documento: geometria, aparência e uma identidade estável.
///
/// O `id` é o que permite ao histórico referir-se a uma anotação depois que
/// os índices mudaram (uma exclusão no meio da lista desloca todo o resto),
/// e o `style` vive aqui — fora das variantes de [`Shape`] — para que trocar
/// a cor ou a espessura de uma anotação já criada não dependa de saber qual
/// é a forma dela.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub id: u64,
    pub shape: Shape,
    pub style: Style,
}

impl Layer {
    /// `p` está a até `tol` px (espaço da imagem) do traço da anotação?
    ///
    /// `text_size` é o tamanho `(largura, altura)` do texto renderizado —
    /// medido pelo chamador, que tem acesso à fonte — e só é usado na
    /// variante `Text` (caixa `anchor..anchor+text_size` expandida por `tol`).
    pub fn hit_test(&self, p: Point, tol: f32, text_size: (f32, f32)) -> bool {
        let reach = tol + self.style.stroke_width / 2.0;
        match &self.shape {
            Shape::Line { a, b } => dist_to_segment(p, *a, *b) <= reach,
            Shape::Arrow { a, b } => {
                let geo = arrow_geometry(*a, *b, self.style.stroke_width);
                dist_to_segment(p, geo.shaft_a, geo.shaft_b) <= reach
                    || point_in_triangle(p, geo.head)
                    || (0..3).any(|i| dist_to_segment(p, geo.head[i], geo.head[(i + 1) % 3]) <= tol)
            }
            Shape::Rect { min, max } => {
                let corners = [*min, Point::new(max.x, min.y), *max, Point::new(min.x, max.y)];
                (0..4).any(|i| dist_to_segment(p, corners[i], corners[(i + 1) % 4]) <= reach)
            }
            Shape::Ellipse { center, rx, ry } => {
                // Contorno amostrado em segmentos: robusto inclusive para
                // elipses degeneradas (um arrasto quase-linear cria rx≈0 ou
                // ry≈0, cujo traço é um segmento).
                const N: usize = 32;
                let pt = |i: usize| {
                    let a = i as f32 / N as f32 * std::f32::consts::TAU;
                    Point::new(center.x + rx * a.cos(), center.y + ry * a.sin())
                };
                (0..N).any(|i| dist_to_segment(p, pt(i), pt(i + 1)) <= reach)
            }
            Shape::Text { anchor, .. } => {
                let (w, h) = text_size;
                p.x >= anchor.x - tol
                    && p.x <= anchor.x + w + tol
                    && p.y >= anchor.y - tol
                    && p.y <= anchor.y + h + tol
            }
        }
    }

    /// Caixa envolvente da geometria, em px da imagem (sem a espessura do
    /// traço). `None` para texto, cuja extensão só é conhecida por quem tem
    /// a fonte em mãos.
    pub fn bbox(&self) -> Option<(Point, Point)> {
        match &self.shape {
            Shape::Line { a, b } | Shape::Arrow { a, b } => Some(normalize(*a, *b)),
            Shape::Rect { min, max } => Some((*min, *max)),
            Shape::Ellipse { center, rx, ry } => Some((
                Point::new(center.x - rx, center.y - ry),
                Point::new(center.x + rx, center.y + ry),
            )),
            Shape::Text { .. } => None,
        }
    }

    /// Alças de redimensionamento, em px da imagem.
    ///
    /// Linha e seta expõem só as duas pontas. As formas com área expõem os
    /// quatro cantos, mais as alças de aresta cujo lado tenha ao menos
    /// `min_side` — sem esse piso elas se sobreporiam aos cantos e virariam
    /// um borrão de pontos numa anotação pequena. O texto não tem alça: seu
    /// tamanho se ajusta pela roda.
    pub fn handles(&self, min_side: f32) -> Vec<(Handle, Point)> {
        if let Shape::Line { a, b } | Shape::Arrow { a, b } = &self.shape {
            return vec![(Handle::Start, *a), (Handle::End, *b)];
        }
        let Some((min, max)) = self.bbox() else {
            return Vec::new();
        };
        let (wide, tall) = (max.x - min.x >= min_side, max.y - min.y >= min_side);
        Handle::BOX
            .into_iter()
            .filter(|h| match h {
                Handle::Top | Handle::Bottom => wide,
                Handle::Left | Handle::Right => tall,
                _ => true,
            })
            .map(|h| (h, h.position(min, max)))
            .collect()
    }

    /// Arrasta a alça `handle` até `to`.
    ///
    /// Com `constrain`, cantos preservam a proporção original e pontas de
    /// linha/seta prendem em múltiplos de 45°. Puxar uma aresta para além da
    /// oposta vira a forma do avesso e ela continua acompanhando o ponteiro,
    /// que é o comportamento que não trava o gesto no meio.
    pub fn resize(&mut self, handle: Handle, to: Point, constrain: bool) {
        match handle {
            Handle::Start | Handle::End => self.move_endpoint(handle, to, constrain),
            _ => self.resize_box(handle, to, constrain),
        }
    }

    fn move_endpoint(&mut self, handle: Handle, to: Point, constrain: bool) {
        let (Shape::Line { a, b } | Shape::Arrow { a, b }) = &mut self.shape else {
            return;
        };
        let (moving, fixed) = if handle == Handle::Start { (&mut *a, *b) } else { (&mut *b, *a) };
        *moving = if constrain { snap_45(fixed, to) } else { to };
    }

    fn resize_box(&mut self, handle: Handle, to: Point, constrain: bool) {
        let Some((mut min, mut max)) = self.bbox() else {
            return;
        };
        let target = if constrain && handle.is_corner() {
            aspect_locked(handle, min, max, to)
        } else {
            to
        };
        let (left, top, right, bottom) = handle.edges();
        if left {
            min.x = target.x;
        }
        if right {
            max.x = target.x;
        }
        if top {
            min.y = target.y;
        }
        if bottom {
            max.y = target.y;
        }
        let (min, max) = normalize(min, max);
        self.set_bbox(min, max);
    }

    fn set_bbox(&mut self, min: Point, max: Point) {
        match &mut self.shape {
            Shape::Rect { min: lo, max: hi } => {
                *lo = min;
                *hi = max;
            }
            Shape::Ellipse { center, rx, ry } => {
                *rx = (max.x - min.x) / 2.0;
                *ry = (max.y - min.y) / 2.0;
                *center = Point::new(min.x + *rx, min.y + *ry);
            }
            // Linha e seta são redimensionadas pelas pontas; texto, pela roda.
            Shape::Line { .. } | Shape::Arrow { .. } | Shape::Text { .. } => {}
        }
    }
}

/// Canto oposto ao da alça — o que fica parado durante o arrasto. A alça que
/// puxa a aresta esquerda gira em torno da direita, e assim por diante.
fn opposite_corner(handle: Handle, min: Point, max: Point) -> Point {
    let (left, top, ..) = handle.edges();
    Point::new(
        if left { max.x } else { min.x },
        if top { max.y } else { min.y },
    )
}

/// Projeta `to` de modo a preservar a proporção original da caixa, medindo a
/// partir do canto que não se move. O eixo mais esticado é o que manda.
fn aspect_locked(handle: Handle, min: Point, max: Point, to: Point) -> Point {
    let fixed = opposite_corner(handle, min, max);
    let (w, h) = (max.x - min.x, max.y - min.y);
    if w.abs() <= f32::EPSILON || h.abs() <= f32::EPSILON {
        return to;
    }
    let (dx, dy) = (to.x - fixed.x, to.y - fixed.y);
    let scale = (dx.abs() / w).max(dy.abs() / h);
    Point::new(
        fixed.x + w * scale * if dx < 0.0 { -1.0 } else { 1.0 },
        fixed.y + h * scale * if dy < 0.0 { -1.0 } else { 1.0 },
    )
}

/// Alça de redimensionamento de uma anotação selecionada.
///
/// Formas com área usam as oito do retângulo envolvente; linha e seta usam
/// as duas pontas, que é o que dá sentido a arrastá-las.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Start,
    End,
}

impl Handle {
    /// As oito alças da caixa, em ordem horária a partir do canto superior
    /// esquerdo — a mesma ordem usada pelo recorte.
    pub const BOX: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    /// Quais arestas da caixa esta alça arrasta: `(esquerda, topo, direita, base)`.
    pub fn edges(self) -> (bool, bool, bool, bool) {
        match self {
            Handle::TopLeft => (true, true, false, false),
            Handle::Top => (false, true, false, false),
            Handle::TopRight => (false, true, true, false),
            Handle::Right => (false, false, true, false),
            Handle::BottomRight => (false, false, true, true),
            Handle::Bottom => (false, false, false, true),
            Handle::BottomLeft => (true, false, false, true),
            Handle::Left => (true, false, false, false),
            Handle::Start | Handle::End => (false, false, false, false),
        }
    }

    /// Alça de canto move dois eixos — é onde a trava de proporção faz sentido.
    pub fn is_corner(self) -> bool {
        let (l, t, r, b) = self.edges();
        (l || r) && (t || b)
    }

    /// Posição da alça na caixa `min..max`.
    pub fn position(self, min: Point, max: Point) -> Point {
        let cx = (min.x + max.x) / 2.0;
        let cy = (min.y + max.y) / 2.0;
        match self {
            Handle::TopLeft => min,
            Handle::Top => Point::new(cx, min.y),
            Handle::TopRight => Point::new(max.x, min.y),
            Handle::Right => Point::new(max.x, cy),
            Handle::BottomRight => max,
            Handle::Bottom => Point::new(cx, max.y),
            Handle::BottomLeft => Point::new(min.x, max.y),
            Handle::Left => Point::new(min.x, cy),
            Handle::Start | Handle::End => min,
        }
    }
}

impl Shape {
    /// Desloca a forma inteira por `(dx, dy)`, em px da imagem (issue #2).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        let mv = |p: &mut Point| {
            p.x += dx;
            p.y += dy;
        };
        match self {
            Self::Line { a, b } | Self::Arrow { a, b } => {
                mv(a);
                mv(b);
            }
            Self::Rect { min, max } => {
                mv(min);
                mv(max);
            }
            Self::Ellipse { center, .. } => mv(center),
            Self::Text { anchor, .. } => mv(anchor),
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
/// elipse em círculo. Retorna `None` para as ferramentas Texto, Mover e
/// Recortar (fluxos próprios).
pub fn shape_from_drag(tool: Tool, a: Point, mut b: Point, shift: bool) -> Option<Shape> {
    match tool {
        Tool::Line | Tool::Arrow => {
            if shift {
                b = snap_45(a, b);
            }
            Some(if tool == Tool::Line {
                Shape::Line { a, b }
            } else {
                Shape::Arrow { a, b }
            })
        }
        Tool::Rect => {
            if shift {
                b = snap_square(a, b);
            }
            let (min, max) = normalize(a, b);
            Some(Shape::Rect { min, max })
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
            })
        }
        Tool::Text | Tool::Select | Tool::Crop => None,
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

/// Canto superior-esquerdo e inferior-direito do retângulo `a`–`b`.
pub fn normalize(a: Point, b: Point) -> (Point, Point) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style { color: [255, 0, 0, 255], stroke_width: 3.0, font_size: 24.0 }
    }

    fn layer(shape: Shape) -> Layer {
        Layer { id: 1, shape, style: style() }
    }

    #[test]
    fn shift_makes_square() {
        let s = shape_from_drag(Tool::Rect, Point::new(10.0, 10.0), Point::new(50.0, 20.0), true)
            .unwrap();
        match s {
            Shape::Rect { min, max } => assert_eq!(max.x - min.x, max.y - min.y),
            _ => panic!("esperava Rect"),
        }
    }

    #[test]
    fn shift_snaps_line_to_45() {
        let s = shape_from_drag(Tool::Line, Point::new(0.0, 0.0), Point::new(100.0, 8.0), true)
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
        let mut rect = Shape::Rect { min: Point::new(10.0, 10.0), max: Point::new(20.0, 30.0) };
        rect.translate(5.0, -3.0);
        match rect {
            Shape::Rect { min, max } => {
                assert_eq!((min.x, min.y), (15.0, 7.0));
                assert_eq!((max.x, max.y), (25.0, 27.0));
            }
            _ => unreachable!(),
        }

        let mut text = Shape::Text { anchor: Point::new(1.0, 2.0), content: "oi".into() };
        text.translate(-1.0, -2.0);
        match text {
            Shape::Text { anchor, .. } => assert_eq!((anchor.x, anchor.y), (0.0, 0.0)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn hit_test_line_edge_and_miss() {
        // Traço 3 → meia-espessura 1,5.
        let line = layer(Shape::Line { a: Point::new(0.0, 0.0), b: Point::new(100.0, 0.0) });
        assert!(line.hit_test(Point::new(50.0, 3.0), 2.0, (0.0, 0.0)));
        assert!(!line.hit_test(Point::new(50.0, 10.0), 2.0, (0.0, 0.0)));
    }

    #[test]
    fn hit_test_rect_border_not_interior() {
        let rect = layer(Shape::Rect { min: Point::new(10.0, 10.0), max: Point::new(50.0, 50.0) });
        assert!(rect.hit_test(Point::new(10.0, 30.0), 2.0, (0.0, 0.0)), "borda esquerda");
        assert!(!rect.hit_test(Point::new(30.0, 30.0), 2.0, (0.0, 0.0)), "miolo vazio");
    }

    #[test]
    fn hit_test_ellipse_ring_not_center() {
        let ellipse = layer(Shape::Ellipse { center: Point::new(50.0, 50.0), rx: 20.0, ry: 10.0 });
        assert!(ellipse.hit_test(Point::new(70.0, 50.0), 2.0, (0.0, 0.0)), "contorno em rx");
        assert!(!ellipse.hit_test(Point::new(50.0, 50.0), 2.0, (0.0, 0.0)), "centro vazio");
    }

    #[test]
    fn hit_test_arrow_head() {
        let arrow = layer(Shape::Arrow { a: Point::new(0.0, 50.0), b: Point::new(100.0, 50.0) });
        // Dentro do triângulo da ponta (comprimento 12 = 4×3).
        assert!(arrow.hit_test(Point::new(95.0, 50.0), 0.0, (0.0, 0.0)));
        assert!(!arrow.hit_test(Point::new(95.0, 60.0), 2.0, (0.0, 0.0)));
    }

    #[test]
    fn hit_test_text_uses_measured_box() {
        let text = layer(Shape::Text { anchor: Point::new(10.0, 10.0), content: "abc".into() });
        assert!(text.hit_test(Point::new(30.0, 20.0), 2.0, (40.0, 16.0)));
        assert!(!text.hit_test(Point::new(60.0, 40.0), 2.0, (40.0, 16.0)));
    }

    #[test]
    fn hit_test_degenerate_ellipse_is_reachable() {
        // Arrasto vertical puro cria rx = 0: o traço é um segmento vertical.
        let ellipse = layer(Shape::Ellipse { center: Point::new(50.0, 50.0), rx: 0.0, ry: 30.0 });
        assert!(ellipse.hit_test(Point::new(50.0, 65.0), 2.0, (0.0, 0.0)), "sobre o traço");
        assert!(!ellipse.hit_test(Point::new(60.0, 50.0), 2.0, (0.0, 0.0)), "longe do traço");
    }

    #[test]
    fn hit_test_follows_the_layer_stroke_width() {
        // O alcance sai do estilo da camada, não da forma: engrossar o traço
        // aumenta a área agarrável.
        let shape = Shape::Line { a: Point::new(0.0, 0.0), b: Point::new(100.0, 0.0) };
        let thin = Layer { id: 1, shape: shape.clone(), style: style() };
        let thick = Layer { id: 2, shape, style: Style { stroke_width: 20.0, ..style() } };
        let p = Point::new(50.0, 9.0);
        assert!(!thin.hit_test(p, 1.0, (0.0, 0.0)));
        assert!(thick.hit_test(p, 1.0, (0.0, 0.0)));
    }

    #[test]
    fn crop_and_select_have_no_drag_shape() {
        for tool in [Tool::Crop, Tool::Select, Tool::Text] {
            assert!(
                shape_from_drag(tool, Point::new(0.0, 0.0), Point::new(10.0, 10.0), false)
                    .is_none(),
                "{} tem fluxo próprio",
                tool.label()
            );
        }
    }

    #[test]
    fn normalize_orders_corners() {
        let (min, max) = normalize(Point::new(50.0, 8.0), Point::new(10.0, 30.0));
        assert_eq!((min.x, min.y), (10.0, 8.0));
        assert_eq!((max.x, max.y), (50.0, 30.0));
    }

    // -----------------------------------------------------------------------
    // Alças e redimensionamento
    // -----------------------------------------------------------------------

    fn rect(min: (f32, f32), max: (f32, f32)) -> Layer {
        layer(Shape::Rect {
            min: Point::new(min.0, min.1),
            max: Point::new(max.0, max.1),
        })
    }

    fn bbox_of(l: &Layer) -> ((f32, f32), (f32, f32)) {
        let (min, max) = l.bbox().unwrap();
        ((min.x, min.y), (max.x, max.y))
    }

    #[test]
    fn a_line_is_resized_by_its_two_endpoints() {
        let line = layer(Shape::Line { a: Point::new(0.0, 0.0), b: Point::new(10.0, 10.0) });
        let handles = line.handles(1.0);
        assert_eq!(handles.len(), 2, "linha não tem caixa, tem pontas");
        assert_eq!(handles[0].0, Handle::Start);
        assert_eq!(handles[1].0, Handle::End);
    }

    #[test]
    fn edge_handles_only_appear_when_the_side_is_long_enough() {
        // Fina na vertical: sem alças de topo/base à esquerda e à direita.
        let wide = rect((0.0, 0.0), (100.0, 4.0));
        let kinds: Vec<Handle> = wide.handles(20.0).into_iter().map(|(h, _)| h).collect();
        assert!(kinds.contains(&Handle::Top), "o lado largo comporta a alça");
        assert!(!kinds.contains(&Handle::Left), "o lado curto não comporta");

        // Pequena nos dois eixos: só os quatro cantos.
        let tiny = rect((0.0, 0.0), (5.0, 5.0));
        assert_eq!(tiny.handles(20.0).len(), 4);
    }

    #[test]
    fn resizing_moves_only_the_edges_the_handle_owns() {
        let mut l = rect((10.0, 10.0), (50.0, 50.0));
        l.resize(Handle::Right, Point::new(80.0, 999.0), false);
        assert_eq!(bbox_of(&l), ((10.0, 10.0), (80.0, 50.0)), "só a direita cede");

        let mut l = rect((10.0, 10.0), (50.0, 50.0));
        l.resize(Handle::TopLeft, Point::new(0.0, 4.0), false);
        assert_eq!(bbox_of(&l), ((0.0, 4.0), (50.0, 50.0)), "canto move dois eixos");
    }

    #[test]
    fn dragging_past_the_opposite_edge_flips_the_shape() {
        // Arrastar a direita para além da esquerda não pode travar o gesto:
        // a forma vira do avesso e continua acompanhando o ponteiro.
        let mut l = rect((10.0, 10.0), (50.0, 50.0));
        l.resize(Handle::Right, Point::new(-30.0, 0.0), false);
        assert_eq!(bbox_of(&l), ((-30.0, 10.0), (10.0, 50.0)));
    }

    #[test]
    fn aspect_lock_keeps_the_original_ratio() {
        // Caixa 40×20 (2:1): puxar o canto mantém a proporção.
        let mut l = rect((0.0, 0.0), (40.0, 20.0));
        l.resize(Handle::BottomRight, Point::new(80.0, 25.0), true);
        let ((x0, y0), (x1, y1)) = bbox_of(&l);
        let (w, h) = (x1 - x0, y1 - y0);
        assert!((w / h - 2.0).abs() < 0.001, "proporção 2:1 preservada, veio {w}×{h}");
    }

    #[test]
    fn aspect_lock_is_ignored_on_edge_handles() {
        // Só cantos movem dois eixos; numa aresta a trava não faria sentido.
        let mut l = rect((0.0, 0.0), (40.0, 20.0));
        l.resize(Handle::Right, Point::new(100.0, 0.0), true);
        assert_eq!(bbox_of(&l), ((0.0, 0.0), (100.0, 20.0)));
    }

    #[test]
    fn endpoint_resize_snaps_to_45_when_constrained() {
        let mut l = layer(Shape::Line { a: Point::new(0.0, 0.0), b: Point::new(10.0, 0.0) });
        l.resize(Handle::End, Point::new(100.0, 8.0), true);
        match l.shape {
            Shape::Line { b, .. } => assert!(b.y.abs() < 0.01, "prende na horizontal"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn ellipse_resize_recomputes_center_and_radii() {
        let mut l = layer(Shape::Ellipse { center: Point::new(50.0, 50.0), rx: 10.0, ry: 10.0 });
        l.resize(Handle::BottomRight, Point::new(80.0, 70.0), false);
        match l.shape {
            Shape::Ellipse { center, rx, ry } => {
                // Caixa 40..80 × 40..70 → centro (60, 55), raios (20, 15).
                assert_eq!((center.x, center.y), (60.0, 55.0));
                assert_eq!((rx, ry), (20.0, 15.0));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn text_has_no_bbox_or_handles() {
        // A extensão do texto depende da fonte, que shapes.rs não conhece.
        let t = layer(Shape::Text { anchor: Point::new(0.0, 0.0), content: "oi".into() });
        assert!(t.bbox().is_none());
        assert!(t.handles(1.0).is_empty());
    }
}

