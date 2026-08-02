//! Rasterização final para exportação (§8): formas vetoriais com `tiny-skia`
//! (stroke com espessura real e anti-aliasing) e texto com `ab_glyph`, sobre
//! a captura em resolução nativa.
//!
//! A fonte usada aqui é a mesma TTF embutida carregada no egui
//! (`crate::editor::FONT_BYTES`), e o cálculo de escala/baseline espelha o do
//! epaint (`as_scaled(px)`, baseline = topo + ascent) — garantindo WYSIWYG
//! entre o editor e o PNG final (CA-04).

use ab_glyph::{Font as _, FontRef, ScaleFont as _};
use anyhow::{anyhow, Context as _, Result};
use image::RgbaImage;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, PixmapMut, Stroke, Transform,
};

use super::shapes::{arrow_geometry, Point, Shape};
use super::FONT_BYTES;

/// Rasteriza `captura + formas` e retorna a imagem final RGBA.
pub fn render(base: &RgbaImage, shapes: &[Shape]) -> Result<RgbaImage> {
    let (w, h) = (base.width(), base.height());
    let mut buffer = base.clone();
    if shapes.is_empty() {
        return Ok(buffer);
    }

    let font = FontRef::try_from_slice(FONT_BYTES).context("fonte embutida inválida")?;

    for shape in shapes {
        match shape {
            Shape::Line { a, b, style } => {
                let mut pb = PathBuilder::new();
                pb.move_to(a.x, a.y);
                pb.line_to(b.x, b.y);
                if let Some(path) = pb.finish() {
                    stroke_path(&mut buffer, w, h, &path, style.color, style.stroke_width)?;
                }
            }
            Shape::Arrow { a, b, style } => {
                let geo = arrow_geometry(*a, *b, style.stroke_width);
                let mut pb = PathBuilder::new();
                pb.move_to(geo.shaft_a.x, geo.shaft_a.y);
                pb.line_to(geo.shaft_b.x, geo.shaft_b.y);
                if let Some(path) = pb.finish() {
                    stroke_path(&mut buffer, w, h, &path, style.color, style.stroke_width)?;
                }

                let mut tri = PathBuilder::new();
                tri.move_to(geo.head[0].x, geo.head[0].y);
                tri.line_to(geo.head[1].x, geo.head[1].y);
                tri.line_to(geo.head[2].x, geo.head[2].y);
                tri.close();
                if let Some(path) = tri.finish() {
                    fill_path(&mut buffer, w, h, &path, style.color)?;
                }
            }
            Shape::Rect { min, max, style } => {
                if let Some(rect) = tiny_skia::Rect::from_ltrb(
                    min.x,
                    min.y,
                    max.x.max(min.x + 0.1),
                    max.y.max(min.y + 0.1),
                ) {
                    let path = PathBuilder::from_rect(rect);
                    stroke_path(&mut buffer, w, h, &path, style.color, style.stroke_width)?;
                }
            }
            Shape::Ellipse { center, rx, ry, style } => {
                if let Some(oval) = tiny_skia::Rect::from_ltrb(
                    center.x - rx,
                    center.y - ry,
                    center.x + rx.max(0.1),
                    center.y + ry.max(0.1),
                ) {
                    if let Some(path) = PathBuilder::from_oval(oval) {
                        stroke_path(&mut buffer, w, h, &path, style.color, style.stroke_width)?;
                    }
                }
            }
            Shape::Text { anchor, content, style } => {
                draw_text(&mut buffer, &font, *anchor, content, style.color, style.font_size);
            }
        }
    }
    Ok(buffer)
}

fn pixmap_of<'a>(buffer: &'a mut RgbaImage, w: u32, h: u32) -> Result<PixmapMut<'a>> {
    // A captura é opaca (alfa 255) e as cores das formas também; com fundo
    // opaco, RGBA "straight" e pré-multiplicado coincidem, então o buffer
    // pode ser usado direto como pixmap.
    PixmapMut::from_bytes(buffer.as_mut(), w, h).ok_or_else(|| anyhow!("pixmap inválido"))
}

fn paint_for(color: [u8; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint.anti_alias = true;
    paint
}

fn stroke_path(
    buffer: &mut RgbaImage,
    w: u32,
    h: u32,
    path: &Path,
    color: [u8; 4],
    width: f32,
) -> Result<()> {
    let mut pixmap = pixmap_of(buffer, w, h)?;
    let stroke = Stroke {
        width: width.max(0.5),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(path, &paint_for(color), &stroke, Transform::identity(), None);
    Ok(())
}

fn fill_path(
    buffer: &mut RgbaImage,
    w: u32,
    h: u32,
    path: &Path,
    color: [u8; 4],
) -> Result<()> {
    let mut pixmap = pixmap_of(buffer, w, h)?;
    pixmap.fill_path(
        path,
        &paint_for(color),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    Ok(())
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
    let pixel = buffer.get_pixel_mut(x as u32, y as u32);
    for (channel, src) in pixel.0.iter_mut().zip(color).take(3) {
        let dst = *channel as f32;
        *channel = (src as f32 * alpha + dst * (1.0 - alpha)).round() as u8;
    }
    let dst_a = pixel.0[3] as f32 / 255.0;
    pixel.0[3] = ((alpha + dst_a * (1.0 - alpha)) * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::shapes::{shape_from_drag, Style, Tool};

    fn base() -> RgbaImage {
        RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]))
    }

    fn style() -> Style {
        Style { color: [255, 0, 0, 255], stroke_width: 3.0, font_size: 16.0 }
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
        let line = shape_from_drag(
            Tool::Line,
            Point::new(4.0, 4.0),
            Point::new(60.0, 60.0),
            false,
            style(),
        )
        .unwrap();
        let out = render(&img, &[line]).unwrap();
        assert_ne!(img.as_raw(), out.as_raw());
        // O pixel do meio da diagonal deve ter ficado avermelhado.
        let p = out.get_pixel(32, 32);
        assert!(p.0[0] > 100, "esperava traço vermelho, obtido {:?}", p.0);
    }

    #[test]
    fn render_text_touches_pixels() {
        let img = base();
        let text = Shape::Text {
            anchor: Point::new(2.0, 2.0),
            content: "Ok".into(),
            style: style(),
        };
        let out = render(&img, &[text]).unwrap();
        assert_ne!(img.as_raw(), out.as_raw());
    }

    #[test]
    fn dimensions_preserved() {
        let img = base();
        let rect = shape_from_drag(
            Tool::Rect,
            Point::new(1.0, 1.0),
            Point::new(50.0, 40.0),
            false,
            style(),
        )
        .unwrap();
        let out = render(&img, &[rect]).unwrap();
        assert_eq!((out.width(), out.height()), (64, 64));
    }
}
