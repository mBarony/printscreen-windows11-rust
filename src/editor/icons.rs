//! Ícones da toolbar do editor, desenhados vetorialmente.
//!
//! Cada ícone é uma lista de primitivas em coordenadas normalizadas (0–1
//! dentro do quadrado do ícone), escaladas no desenho: nítidas em qualquer
//! DPI e sempre na cor do tema — sem depender de fontes de símbolos/emoji,
//! cuja cobertura varia por máquina e vira tofu quando o glifo falta.
//!
//! A geometria fica separada da pintura para poder ser inspecionada sem um
//! backend gráfico (ver os testes, que a validam e exportam uma prévia SVG).

use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2};

use super::shapes::Tool;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Move,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Freehand,
    Highlighter,
    Marker,
    Eyedropper,
    Redact,
    Spotlight,
    Ruler,
    LineSolid,
    LineDashed,
    LineDotted,
    Sketch,
    RedactText,
    Gif,
    Fill,
    TextPill,
    Backdrop,
    Ocr,
    Text,
    Crop,
    Cut,
    Undo,
    Redo,
    Copy,
    Save,
    Close,
    Check,
    Pin,
}

impl Icon {
    pub fn of(tool: Tool) -> Self {
        match tool {
            Tool::Select => Self::Move,
            Tool::Line => Self::Line,
            Tool::Arrow => Self::Arrow,
            Tool::Rect => Self::Rect,
            Tool::Ellipse => Self::Ellipse,
            Tool::Freehand => Self::Freehand,
            Tool::Highlighter => Self::Highlighter,
            Tool::Marker => Self::Marker,
            Tool::Eyedropper => Self::Eyedropper,
            Tool::Redact => Self::Redact,
            Tool::Spotlight => Self::Spotlight,
            Tool::Text => Self::Text,
            Tool::Ruler => Self::Ruler,
            Tool::Crop => Self::Crop,
            Tool::Cut => Self::Cut,
        }
    }

    #[cfg(test)]
    pub const ALL: [Icon; 32] = [
        Icon::Move,
        Icon::Line,
        Icon::Arrow,
        Icon::Rect,
        Icon::Ellipse,
        Icon::Freehand,
        Icon::Highlighter,
        Icon::Marker,
        Icon::Eyedropper,
        Icon::Redact,
        Icon::Spotlight,
        Icon::Ruler,
        Icon::LineSolid,
        Icon::LineDashed,
        Icon::LineDotted,
        Icon::Sketch,
        Icon::RedactText,
        Icon::Gif,
        Icon::Fill,
        Icon::TextPill,
        Icon::Backdrop,
        Icon::Ocr,
        Icon::Text,
        Icon::Crop,
        Icon::Cut,
        Icon::Undo,
        Icon::Redo,
        Icon::Copy,
        Icon::Save,
        Icon::Check,
        Icon::Close,
        Icon::Pin,
    ];

    #[cfg(test)]
    pub fn name(self) -> &'static str {
        match self {
            Icon::Move => "move",
            Icon::Line => "line",
            Icon::Arrow => "arrow",
            Icon::Rect => "rect",
            Icon::Ellipse => "ellipse",
            Icon::Freehand => "freehand",
            Icon::Highlighter => "highlighter",
            Icon::Marker => "marker",
            Icon::Eyedropper => "eyedropper",
            Icon::Redact => "redact",
            Icon::Spotlight => "spotlight",
            Icon::Ruler => "ruler",
            Icon::LineSolid => "line_solid",
            Icon::LineDashed => "line_dashed",
            Icon::LineDotted => "line_dotted",
            Icon::Sketch => "sketch",
            Icon::RedactText => "redact_text",
            Icon::Gif => "gif",
            Icon::Fill => "fill",
            Icon::TextPill => "text_pill",
            Icon::Backdrop => "backdrop",
            Icon::Ocr => "ocr",
            Icon::Text => "text",
            Icon::Crop => "crop",
            Icon::Cut => "cut",
            Icon::Undo => "undo",
            Icon::Redo => "redo",
            Icon::Copy => "copy",
            Icon::Save => "save",
            Icon::Close => "close",
            Icon::Check => "check",
            Icon::Pin => "pin",
        }
    }
}

/// Primitiva de um ícone, em coordenadas normalizadas.
pub enum Primitive {
    /// Polilinha aberta ou fechada, desenhada em traço.
    Stroke(Vec<(f32, f32)>),
    /// Polígono convexo preenchido (pontas de seta).
    Fill(Vec<(f32, f32)>),
}

/// Polilinha que aproxima uma elipse centrada em `(cx, cy)`.
fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 28;
    (0..=STEPS)
        .map(|i| {
            let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

fn rectangle(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Vec<(f32, f32)> {
    vec![
        (min_x, min_y),
        (max_x, min_y),
        (max_x, max_y),
        (min_x, max_y),
        (min_x, min_y),
    ]
}

/// Meia-volta usada por desfazer/refazer: arco superior de 180°.
fn half_arc(mirrored: bool) -> Vec<(f32, f32)> {
    let x = |v: f32| if mirrored { 1.0 - v } else { v };
    (0..=18)
        .map(|i| {
            let t = i as f32 / 18.0;
            let angle = std::f32::consts::PI * (1.0 - t);
            (x(0.5 + 0.36 * angle.cos()), 0.6 - 0.32 * angle.sin())
        })
        .collect()
}

/// Geometria completa de um ícone.
pub fn geometry(icon: Icon) -> Vec<Primitive> {
    use Primitive::{Fill, Stroke};
    match icon {
        // Cruz com quatro pontas — reposicionar.
        Icon::Move => vec![
            Stroke(vec![(0.5, 0.14), (0.5, 0.86)]),
            Stroke(vec![(0.14, 0.5), (0.86, 0.5)]),
            Fill(vec![(0.5, 0.04), (0.35, 0.24), (0.65, 0.24)]),
            Fill(vec![(0.5, 0.96), (0.35, 0.76), (0.65, 0.76)]),
            Fill(vec![(0.04, 0.5), (0.24, 0.35), (0.24, 0.65)]),
            Fill(vec![(0.96, 0.5), (0.76, 0.35), (0.76, 0.65)]),
        ],
        Icon::Line => vec![Stroke(vec![(0.14, 0.86), (0.86, 0.14)])],
        Icon::Arrow => vec![
            Stroke(vec![(0.14, 0.86), (0.68, 0.32)]),
            Fill(vec![(0.92, 0.08), (0.54, 0.18), (0.82, 0.46)]),
        ],
        Icon::Rect => vec![Stroke(rectangle(0.12, 0.22, 0.88, 0.78))],
        Icon::Ellipse => vec![Stroke(ellipse(0.5, 0.5, 0.4, 0.32))],
        // Rabisco em "S" deitado — o gesto da mão livre.
        Icon::Freehand => vec![Stroke(vec![
            (0.10, 0.68),
            (0.24, 0.40),
            (0.40, 0.66),
            (0.56, 0.34),
            (0.72, 0.60),
            (0.90, 0.30),
        ])],
        // Disco com um "1" dentro — o contador numerado.
        Icon::Marker => vec![
            Stroke(ellipse(0.5, 0.5, 0.38, 0.38)),
            Stroke(vec![(0.42, 0.38), (0.52, 0.30), (0.52, 0.70)]),
        ],
        // Pipeta em pé: bulbo, ombro e o corpo afunilando até a ponta. Na
        // diagonal, com as três partes soltas, ela lia como caneta.
        Icon::Eyedropper => vec![
            Fill(ellipse(0.50, 0.16, 0.16, 0.10)),
            Stroke(vec![
                (0.34, 0.34),
                (0.66, 0.34),
                (0.56, 0.60),
                (0.50, 0.95),
                (0.44, 0.60),
                (0.34, 0.34),
            ]),
        ],
        // Duas metades separadas por uma costura tracejada.
        Icon::Cut => vec![
            Stroke(rectangle(0.10, 0.08, 0.90, 0.34)),
            Stroke(rectangle(0.10, 0.66, 0.90, 0.92)),
            Stroke(vec![(0.12, 0.50), (0.30, 0.50)]),
            Stroke(vec![(0.42, 0.50), (0.60, 0.50)]),
            Stroke(vec![(0.72, 0.50), (0.90, 0.50)]),
        ],
        // Círculo com raios em volta — a área que fica acesa enquanto o
        // resto escurece. A versão anterior tinha cabo e virava lupa, o que
        // prometia um zoom que o editor não tem.
        Icon::Spotlight => vec![
            Stroke(ellipse(0.50, 0.50, 0.22, 0.22)),
            Stroke(vec![(0.50, 0.05), (0.50, 0.17)]),
            Stroke(vec![(0.50, 0.83), (0.50, 0.95)]),
            Stroke(vec![(0.05, 0.50), (0.17, 0.50)]),
            Stroke(vec![(0.83, 0.50), (0.95, 0.50)]),
            Stroke(vec![(0.18, 0.18), (0.27, 0.27)]),
            Stroke(vec![(0.73, 0.73), (0.82, 0.82)]),
            Stroke(vec![(0.82, 0.18), (0.73, 0.27)]),
            Stroke(vec![(0.27, 0.73), (0.18, 0.82)]),
        ],
        // Retângulo com um xadrez dentro — a área coberta pelo mosaico.
        Icon::Redact => vec![
            Stroke(rectangle(0.10, 0.16, 0.90, 0.84)),
            Fill(rectangle(0.22, 0.28, 0.46, 0.50)),
            Fill(rectangle(0.54, 0.50, 0.78, 0.72)),
            Fill(rectangle(0.54, 0.28, 0.78, 0.38)),
        ],
        // Traço com uma ponta em cada extremidade e os riscos da escala por
        // baixo — mede, não aponta.
        Icon::Ruler => vec![
            Stroke(vec![(0.22, 0.38), (0.78, 0.38)]),
            Fill(vec![(0.06, 0.38), (0.24, 0.29), (0.24, 0.47)]),
            Fill(vec![(0.94, 0.38), (0.76, 0.29), (0.76, 0.47)]),
            Stroke(vec![(0.20, 0.62), (0.20, 0.86)]),
            Stroke(vec![(0.40, 0.62), (0.40, 0.78)]),
            Stroke(vec![(0.60, 0.62), (0.60, 0.78)]),
            Stroke(vec![(0.80, 0.62), (0.80, 0.86)]),
            Stroke(vec![(0.20, 0.62), (0.80, 0.62)]),
        ],
        // Os três padrões de traço, na mesma diagonal do ícone da Linha:
        // cheio, partido em traços, partido em pontos.
        Icon::LineSolid => vec![Stroke(vec![(0.12, 0.72), (0.88, 0.28)])],
        Icon::LineDashed => vec![
            Stroke(vec![(0.12, 0.72), (0.32, 0.60)]),
            Stroke(vec![(0.42, 0.55), (0.62, 0.43)]),
            Stroke(vec![(0.72, 0.38), (0.88, 0.28)]),
        ],
        Icon::LineDotted => (0..5)
            .map(|i| {
                let t = i as f32 / 4.0;
                let (cx, cy) = (0.14 + t * 0.72, 0.70 - t * 0.40);
                Fill(ellipse(cx, cy, 0.075, 0.075))
            })
            .collect(),
        // Duas passadas trêmulas por cima da mesma diagonal — que é
        // exatamente o que o estilo faz com o traço.
        Icon::Sketch => vec![
            Stroke(vec![
                (0.12, 0.74),
                (0.30, 0.60),
                (0.48, 0.52),
                (0.66, 0.36),
                (0.88, 0.26),
            ]),
            Stroke(vec![
                (0.14, 0.80),
                (0.34, 0.68),
                (0.50, 0.58),
                (0.70, 0.44),
                (0.86, 0.32),
            ]),
        ],
        // Linhas de texto com o miolo tapado: só as palavras somem, o
        // contorno da região continua lá.
        Icon::RedactText => vec![
            Stroke(rectangle(0.08, 0.16, 0.92, 0.84)),
            Fill(rectangle(0.20, 0.32, 0.62, 0.42)),
            Fill(rectangle(0.20, 0.53, 0.80, 0.63)),
            Fill(rectangle(0.68, 0.32, 0.80, 0.42)),
        ],
        // Dois quadros empilhados e uma seta de vaivém entre eles — o antes
        // e o depois alternando.
        Icon::Gif => vec![
            Stroke(rectangle(0.06, 0.10, 0.62, 0.52)),
            Stroke(rectangle(0.38, 0.48, 0.94, 0.90)),
            Stroke(vec![(0.30, 0.69), (0.70, 0.69)]),
            Fill(vec![(0.20, 0.69), (0.34, 0.62), (0.34, 0.76)]),
            Fill(vec![(0.80, 0.69), (0.66, 0.62), (0.66, 0.76)]),
        ],
        // Quadrado cheio dentro de um vazado — o par contorno/preenchimento.
        Icon::Fill => vec![
            Stroke(rectangle(0.10, 0.10, 0.90, 0.90)),
            Fill(rectangle(0.30, 0.30, 0.70, 0.70)),
        ],
        // Retângulo pequeno dentro de um grande — a captura sobre o fundo.
        Icon::Backdrop => vec![
            Stroke(rectangle(0.06, 0.06, 0.94, 0.94)),
            Fill(rectangle(0.26, 0.26, 0.74, 0.74)),
        ],
        // Mira de leitura com linhas de texto dentro: reconhecer o que está
        // escrito na imagem. Os cantos soltos dizem "ler daqui", e não
        // "desenhar uma caixa" — moldura fechada seria o Recortar.
        Icon::Ocr => vec![
            Stroke(vec![(0.06, 0.28), (0.06, 0.08), (0.26, 0.08)]),
            Stroke(vec![(0.74, 0.08), (0.94, 0.08), (0.94, 0.28)]),
            Stroke(vec![(0.94, 0.72), (0.94, 0.92), (0.74, 0.92)]),
            Stroke(vec![(0.26, 0.92), (0.06, 0.92), (0.06, 0.72)]),
            Stroke(vec![(0.24, 0.38), (0.76, 0.38)]),
            Stroke(vec![(0.24, 0.54), (0.76, 0.54)]),
            Stroke(vec![(0.24, 0.70), (0.56, 0.70)]),
        ],
        // "A" sobre a pílula que fica atrás do texto.
        Icon::TextPill => vec![
            Stroke(rectangle(0.06, 0.30, 0.94, 0.74)),
            Stroke(vec![(0.30, 0.66), (0.50, 0.38), (0.70, 0.66)]),
            Stroke(vec![(0.37, 0.56), (0.63, 0.56)]),
        ],
        // Ponta chanfrada sobre a faixa que ela deixa. A faixa é cheia, e
        // não um risco fino: é o que separa o marca-texto de uma borracha.
        Icon::Highlighter => vec![
            Fill(vec![(0.24, 0.54), (0.60, 0.14), (0.80, 0.32), (0.44, 0.72)]),
            Fill(rectangle(0.10, 0.79, 0.90, 0.94)),
        ],
        Icon::Text => vec![
            Stroke(vec![(0.16, 0.18), (0.84, 0.18)]), // barra do T
            Stroke(vec![(0.5, 0.18), (0.5, 0.86)]),   // haste
        ],
        // Dois "L" cruzados — símbolo clássico de recorte.
        Icon::Crop => vec![
            Stroke(vec![(0.26, 0.04), (0.26, 0.74), (0.96, 0.74)]),
            Stroke(vec![(0.04, 0.26), (0.74, 0.26), (0.74, 0.96)]),
        ],
        Icon::Undo | Icon::Redo => {
            let mirrored = icon == Icon::Redo;
            let x = |v: f32| if mirrored { 1.0 - v } else { v };
            vec![
                Stroke(half_arc(mirrored)),
                Stroke(vec![(x(0.86), 0.6), (x(0.86), 0.84)]),
                Fill(vec![
                    (x(0.14), 0.68),
                    (x(0.01), 0.44),
                    (x(0.29), 0.42),
                ]),
            ]
        }
        Icon::Copy => vec![
            Stroke(rectangle(0.08, 0.08, 0.6, 0.6)),
            Stroke(rectangle(0.4, 0.4, 0.92, 0.92)),
        ],
        // Seta para baixo sobre a bandeja — gravar em disco.
        Icon::Save => vec![
            Stroke(vec![(0.5, 0.08), (0.5, 0.62)]),
            Stroke(vec![(0.27, 0.39), (0.5, 0.62), (0.73, 0.39)]),
            Stroke(vec![(0.12, 0.88), (0.88, 0.88)]),
        ],
        Icon::Close => vec![
            Stroke(vec![(0.18, 0.18), (0.82, 0.82)]),
            Stroke(vec![(0.82, 0.18), (0.18, 0.82)]),
        ],
        Icon::Check => vec![Stroke(vec![(0.12, 0.52), (0.4, 0.8), (0.88, 0.2)])],
        // Alfinete visto de lado: cabeça larga, colarinho e a agulha descendo
        // até a ponta. Espetado na diagonal, como um alfinete de mural — em
        // pé ele lia como taça.
        Icon::Pin => vec![
            Stroke(vec![(0.30, 0.10), (0.72, 0.34)]),
            Stroke(vec![
                (0.36, 0.22),
                (0.24, 0.44),
                (0.30, 0.58),
                (0.58, 0.44),
                (0.62, 0.30),
                (0.36, 0.22),
            ]),
            Stroke(vec![(0.34, 0.56), (0.12, 0.92)]),
        ],
    }
}

/// Desenha `icon` centralizado em `rect` (o lado menor define a escala).
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(side / 2.0);
    let at = |(x, y): (f32, f32)| Pos2::new(origin.x + x * side, origin.y + y * side);
    let stroke = Stroke::new((side * 0.095).clamp(1.2, 2.2), color);

    for primitive in geometry(icon) {
        match primitive {
            Primitive::Stroke(points) => {
                painter.add(Shape::line(points.into_iter().map(at).collect(), stroke));
            }
            Primitive::Fill(points) => {
                painter.add(Shape::convex_polygon(
                    points.into_iter().map(at).collect(),
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_geometry_inside_its_box() {
        for icon in Icon::ALL {
            let primitives = geometry(icon);
            assert!(!primitives.is_empty(), "{} sem geometria", icon.name());
            for primitive in &primitives {
                let points = match primitive {
                    Primitive::Stroke(p) => p,
                    Primitive::Fill(p) => p,
                };
                assert!(
                    points.len() >= 2,
                    "{}: primitiva com menos de 2 pontos",
                    icon.name()
                );
                if let Primitive::Fill(p) = primitive {
                    assert!(p.len() >= 3, "{}: preenchimento não é polígono", icon.name());
                }
                for &(x, y) in points {
                    assert!(
                        (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y),
                        "{}: ponto ({x}, {y}) fora do quadrado do ícone",
                        icon.name()
                    );
                }
            }
        }
    }

    #[test]
    fn tools_map_to_distinct_icons() {
        let tools = [
            Tool::Select,
            Tool::Line,
            Tool::Arrow,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Text,
            Tool::Crop,
            // A régua é uma reta com pontas, como a linha e a seta: é onde a
            // colisão de ícone teria mais chance de passar despercebida.
            Tool::Ruler,
        ];
        let icons: Vec<Icon> = tools.iter().map(|t| Icon::of(*t)).collect();
        for (i, a) in icons.iter().enumerate() {
            for b in &icons[i + 1..] {
                assert!(a != b, "duas ferramentas compartilham o mesmo ícone");
            }
        }
    }

    /// Prévia visual da toolbar sem precisar do Windows: imprime um SVG com
    /// todos os ícones a partir da mesma geometria que a toolbar desenha.
    ///
    /// `cargo test --bin rustshot icons::tests::svg_preview -- --ignored --nocapture`
    #[test]
    #[ignore = "gera a prévia sob demanda"]
    fn svg_preview() {
        const BOX: f32 = 48.0;
        const PAD: f32 = 14.0;
        let step = BOX + PAD;
        let width = step * Icon::ALL.len() as f32 + PAD;
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{}\" \
             viewBox=\"0 0 {width} {}\"><rect width=\"100%\" height=\"100%\" fill=\"#1f1f22\"/>",
            BOX + PAD * 3.0,
            BOX + PAD * 3.0,
        );
        for (index, icon) in Icon::ALL.into_iter().enumerate() {
            let ox = PAD + index as f32 * step;
            let oy = PAD;
            for primitive in geometry(icon) {
                let (points, fill) = match primitive {
                    Primitive::Stroke(p) => (p, false),
                    Primitive::Fill(p) => (p, true),
                };
                let coords: Vec<String> = points
                    .iter()
                    .map(|(x, y)| format!("{:.2},{:.2}", ox + x * BOX, oy + y * BOX))
                    .collect();
                if fill {
                    svg.push_str(&format!(
                        "<polygon points=\"{}\" fill=\"#e8e8ea\"/>",
                        coords.join(" ")
                    ));
                } else {
                    svg.push_str(&format!(
                        "<polyline points=\"{}\" fill=\"none\" stroke=\"#e8e8ea\" \
                         stroke-width=\"4.5\" stroke-linecap=\"round\" \
                         stroke-linejoin=\"round\"/>",
                        coords.join(" ")
                    ));
                }
            }
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#8a8a90\" font-size=\"9\" \
                 font-family=\"sans-serif\" text-anchor=\"middle\">{}</text>",
                ox + BOX / 2.0,
                oy + BOX + 12.0,
                icon.name()
            ));
        }
        svg.push_str("</svg>");
        println!("{svg}");
    }
}
