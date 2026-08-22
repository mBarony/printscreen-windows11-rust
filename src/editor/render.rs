//! Rasterização final para exportação (§8): formas vetoriais com o
//! rasterizador próprio (`raster`) e texto com `ab_glyph`, sobre a captura
//! em resolução nativa.
//!
//! A fonte usada aqui é a mesma TTF embutida carregada no egui
//! (`crate::editor::FONT_BYTES`), e o cálculo de escala/baseline espelha o do
//! epaint (`as_scaled(px)`, baseline = topo + ascent) — garantindo WYSIWYG
//! entre o editor e o JPG final (CA-04). O `ab_glyph` já é parte do epaint
//! (egui); usá-lo aqui não adiciona dependência nova à árvore.

use ab_glyph::{Font as _, FontRef, ScaleFont as _};

use crate::error::{Context as _, Result};
use crate::imgbuf::RgbaImage;

use super::raster;
use super::shapes::{arrow_geometry, stroke_appearance, Layer, Point, Shape};
use super::FONT_BYTES;

/// Rasteriza `captura + anotações` e retorna a imagem final RGBA.
pub fn render(base: &RgbaImage, layers: &[Layer]) -> Result<RgbaImage> {
    let mut buffer = base.clone();
    if layers.is_empty() {
        return Ok(buffer);
    }

    let font = FontRef::try_from_slice(FONT_BYTES).context("fonte embutida inválida")?;

    for layer in layers {
        let style = &layer.style;
        match &layer.shape {
            Shape::Line { a, b } => {
                raster::stroke_line(
                    &mut buffer,
                    (a.x, a.y),
                    (b.x, b.y),
                    style.stroke_width,
                    style.color,
                );
            }
            Shape::Arrow { a, b } => {
                let geo = arrow_geometry(*a, *b, style.stroke_width);
                raster::stroke_line(
                    &mut buffer,
                    (geo.shaft_a.x, geo.shaft_a.y),
                    (geo.shaft_b.x, geo.shaft_b.y),
                    style.stroke_width,
                    style.color,
                );
                raster::fill_triangle(
                    &mut buffer,
                    (geo.head[0].x, geo.head[0].y),
                    (geo.head[1].x, geo.head[1].y),
                    (geo.head[2].x, geo.head[2].y),
                    style.color,
                );
            }
            Shape::Rect { min, max } => {
                raster::stroke_rect(
                    &mut buffer,
                    (min.x, min.y),
                    (max.x.max(min.x + 0.1), max.y.max(min.y + 0.1)),
                    style.stroke_width,
                    style.color,
                );
            }
            Shape::Ellipse { center, rx, ry } => {
                raster::stroke_ellipse(
                    &mut buffer,
                    (center.x, center.y),
                    rx.max(0.1),
                    ry.max(0.1),
                    style.stroke_width,
                    style.color,
                );
            }
            Shape::Freehand { points, highlight } => {
                let (width, color) = stroke_appearance(style, *highlight);
                let path: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
                raster::stroke_polyline(&mut buffer, &path, width, color);
            }
            Shape::Text { anchor, content } => {
                draw_text(&mut buffer, &font, *anchor, content, style.color, style.font_size);
            }
        }
    }
    Ok(buffer)
}

/// Desenha texto ancorado pelo canto superior esquerdo (como o
/// `painter.text(.., Align2::LEFT_TOP, ..)` do preview). Suporta múltiplas
/// linhas separadas por `\n`.
fn draw_text(
    buffer: &mut RgbaImage,
    font: &FontRef<'_>,
    anchor: Point,
    content: &str,
    color: [u8; 4],
    font_size: f32,
) {
    let scale = ab_glyph::PxScale::from(font_size.max(1.0));
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let line_height = scaled.height() + scaled.line_gap();

    let mut baseline_y = anchor.y + ascent;
    for line in content.split('\n') {
        let mut caret_x = anchor.x;
        let mut previous: Option<ab_glyph::GlyphId> = None;
        for ch in line.chars() {
            if ch.is_control() {
                continue;
            }
            let id = scaled.glyph_id(ch);
            if let Some(prev) = previous {
                caret_x += scaled.kern(prev, id);
            }
            let glyph = id.with_scale_and_position(scale, ab_glyph::point(caret_x, baseline_y));
            caret_x += scaled.h_advance(id);
            previous = Some(id);

            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i64 + gx as i64;
                    let py = bounds.min.y as i64 + gy as i64;
                    blend_pixel(buffer, px, py, color, coverage);
                });
            }
        }
        baseline_y += line_height;
    }
}

/// Mistura src-over de um pixel com cobertura do anti-aliasing.
fn blend_pixel(buffer: &mut RgbaImage, x: i64, y: i64, color: [u8; 4], coverage: f32) {
    if x < 0 || y < 0 || x >= buffer.width() as i64 || y >= buffer.height() as i64 {
        return;
    }
    let alpha = (color[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let pixel = buffer.pixel_mut(x as u32, y as u32);
    for (channel, src) in pixel.iter_mut().zip(color).take(3) {
        let dst = *channel as f32;
        *channel = (src as f32 * alpha + dst * (1.0 - alpha)).round() as u8;
    }
    let dst_a = pixel[3] as f32 / 255.0;
    pixel[3] = ((alpha + dst_a * (1.0 - alpha)) * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::shapes::{shape_from_drag, Style, Tool};

    fn base() -> RgbaImage {
        RgbaImage::filled(64, 64, [10, 20, 30, 255])
    }

    fn style() -> Style {
        Style { color: [255, 0, 0, 255], stroke_width: 3.0, font_size: 16.0 }
    }

    fn layer(shape: Shape) -> Layer {
        Layer { id: 1, shape, style: style() }
    }

    fn dragged(tool: Tool, a: (f32, f32), b: (f32, f32)) -> Layer {
        let shape =
            shape_from_drag(tool, Point::new(a.0, a.1), Point::new(b.0, b.1), false, false).unwrap();
        layer(shape)
    }

    #[test]
    fn render_without_shapes_is_identity() {
        let img = base();
        let out = render(&img, &[]).unwrap();
        assert_eq!(img.as_raw(), out.as_raw());
    }

    #[test]
    fn render_line_touches_pixels() {
        let img = base();
        let out = render(&img, &[dragged(Tool::Line, (4.0, 4.0), (60.0, 60.0))]).unwrap();
        assert_ne!(img.as_raw(), out.as_raw());
        // O pixel do meio da diagonal deve ter ficado avermelhado.
        let p = out.pixel(32, 32);
        assert!(p[0] > 100, "esperava traço vermelho, obtido {p:?}");
    }

    #[test]
    fn render_text_touches_pixels() {
        let img = base();
        let text = layer(Shape::Text { anchor: Point::new(2.0, 2.0), content: "Ok".into() });
        let out = render(&img, &[text]).unwrap();
        assert_ne!(img.as_raw(), out.as_raw());
    }

    #[test]
    fn dimensions_preserved() {
        let img = base();
        let out = render(&img, &[dragged(Tool::Rect, (1.0, 1.0), (50.0, 40.0))]).unwrap();
        assert_eq!((out.width(), out.height()), (64, 64));
    }

    #[test]
    fn arrow_head_is_filled() {
        let img = base();
        let out = render(&img, &[dragged(Tool::Arrow, (8.0, 32.0), (56.0, 32.0))]).unwrap();
        // Perto da ponta (x=54, y=32) deve haver vermelho.
        assert!(out.pixel(53, 32)[0] > 150);
    }

    #[test]
    fn freehand_stroke_is_exported() {
        let img = base();
        let points: Vec<Point> = (0..6).map(|i| Point::new(8.0 + i as f32 * 9.0, 32.0)).collect();
        let out = render(&img, &[layer(Shape::Freehand { points, highlight: false })]).unwrap();
        assert!(out.pixel(30, 32)[0] > 150, "traço no meio do caminho");
        assert_eq!(out.pixel(30, 8), [10, 20, 30, 255], "fora do traço");
    }

    #[test]
    fn the_highlighter_lets_the_image_show_through() {
        // Sobre o mesmo fundo, o marca-texto tem de deixar a base aparecer —
        // é o que separa marcar de tapar.
        let img = base();
        let points = vec![Point::new(8.0, 32.0), Point::new(56.0, 32.0)];
        let plain = layer(Shape::Freehand { points: points.clone(), highlight: false });
        let mark = layer(Shape::Freehand { points, highlight: true });

        let opaque = render(&img, &[plain]).unwrap().pixel(32, 32);
        let translucent = render(&img, &[mark]).unwrap().pixel(32, 32);
        assert_eq!(opaque[0], 255, "mão livre cobre");
        assert!(translucent[0] < opaque[0], "marca-texto não cobre");
        assert!(translucent[2] > opaque[2], "o azul do fundo sobrevive");
    }

    #[test]
    fn each_layer_uses_its_own_style() {
        // O estilo deixou de morar na forma: duas camadas com a mesma
        // geometria e cores diferentes têm de sair diferentes.
        let img = base();
        let shape = Shape::Rect { min: Point::new(8.0, 8.0), max: Point::new(56.0, 56.0) };
        let red = Layer { id: 1, shape: shape.clone(), style: style() };
        let green = Layer {
            id: 2,
            shape,
            style: Style { color: [0, 255, 0, 255], ..style() },
        };
        let a = render(&img, &[red]).unwrap();
        let b = render(&img, &[green]).unwrap();
        assert!(a.pixel(8, 32)[0] > 150, "primeira camada vermelha");
        assert!(b.pixel(8, 32)[1] > 150, "segunda camada verde");
    }
}
