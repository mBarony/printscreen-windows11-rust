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

use super::backdrop::{self, BackdropStyle};
use super::dash;
use super::raster;
use super::ruler;
use super::shapes::{arrow_path,
    arrow_geometry, ellipse_path, marker_geometry, rect_path, stroke_appearance, text_pill_metrics,
    Layer, LineStyle, Point, Shape, MARKER_INK, TEXT_PILL_COLOR,
};
use super::FONT_BYTES;

/// Rasteriza `captura + anotações`, monta a moldura em volta e devolve a
/// imagem final RGBA.
pub fn render(
    base: &RgbaImage,
    layers: &[Layer],
    backdrop: BackdropStyle,
) -> Result<RgbaImage> {
    let mut buffer = base.clone();
    if layers.is_empty() {
        return Ok(backdrop::compose(&buffer, backdrop));
    }

    let font = FontRef::try_from_slice(FONT_BYTES).context("fonte embutida inválida")?;

    for layer in layers {
        let style = &layer.style;
        match &layer.shape {
            Shape::Line { a, b } => {
                stroke_path(
                    &mut buffer,
                    &[*a, *b],
                    style.line,
                    style.stroke_width,
                    style.color,
                );
            }
            Shape::Ruler { a, b } => {
                let geo = ruler::geometry(*a, *b, style.stroke_width);
                stroke_path(
                    &mut buffer,
                    &geo.shaft,
                    style.line,
                    style.stroke_width,
                    style.color,
                );
                for head in [geo.head_a, geo.head_b] {
                    raster::fill_triangle(
                        &mut buffer,
                        (head[0].x, head[0].y),
                        (head[1].x, head[1].y),
                        (head[2].x, head[2].y),
                        style.color,
                    );
                }
                // Rótulo sobre uma pílula da cor do traço, como no preview.
                let (w, h) = text_extent(&font, &geo.label, geo.font_size);
                let (pad, radius) = text_pill_metrics(h);
                let anchor = Point::new(
                    geo.label_center.x - w / 2.0,
                    geo.label_center.y - h / 2.0,
                );
                raster::fill_rect(
                    &mut buffer,
                    (anchor.x - pad, anchor.y - pad),
                    (anchor.x + w + pad, anchor.y + h + pad),
                    radius,
                    style.color,
                );
                draw_text(
                    &mut buffer,
                    &font,
                    anchor,
                    &geo.label,
                    ruler::label_ink(style.color),
                    geo.font_size,
                );
            }
            Shape::Arrow { a, b, bend } => {
                // A ponta segue a tangente do fim da curva; a haste é a
                // curva amostrada, encurtada para não vazar por dentro dela.
                let path = arrow_path(*a, *b, *bend);
                let penultimo = path[path.len().saturating_sub(2)];
                let geo = arrow_geometry(penultimo, *b, style.stroke_width);
                let mut haste = path;
                if let Some(ultimo) = haste.last_mut() {
                    *ultimo = geo.shaft_b;
                }
                stroke_path(
                    &mut buffer,
                    &haste,
                    style.line,
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
                let lo = (min.x, min.y);
                let hi = (max.x.max(min.x + 0.1), max.y.max(min.y + 0.1));
                match (style.filled, style.line, style.corner_radius > 0.0) {
                    // Cheio não leva contorno: a silhueta é a própria cor.
                    (true, ..) => {
                        raster::fill_rect(&mut buffer, lo, hi, style.corner_radius, style.color)
                    }
                    // O padrão é medido ao longo do contorno, então o
                    // contorno vira caminho — cantos arredondados inclusive.
                    (false, line, _) if line != LineStyle::Solid => stroke_path(
                        &mut buffer,
                        &rect_path(
                            Point::new(lo.0, lo.1),
                            Point::new(hi.0, hi.1),
                            style.corner_radius,
                        ),
                        line,
                        style.stroke_width,
                        style.color,
                    ),
                    (false, _, true) => raster::stroke_round_rect(
                        &mut buffer,
                        lo,
                        hi,
                        style.corner_radius,
                        style.stroke_width,
                        style.color,
                    ),
                    (false, _, false) => {
                        raster::stroke_rect(&mut buffer, lo, hi, style.stroke_width, style.color)
                    }
                }
            }
            Shape::Ellipse { center, rx, ry } => {
                let (cx, cy) = (center.x, center.y);
                let (rx, ry) = (rx.max(0.1), ry.max(0.1));
                if style.filled {
                    raster::fill_ellipse(&mut buffer, (cx, cy), rx, ry, style.color);
                } else if style.line == LineStyle::Solid {
                    raster::stroke_ellipse(
                        &mut buffer,
                        (cx, cy),
                        rx,
                        ry,
                        style.stroke_width,
                        style.color,
                    );
                } else {
                    stroke_path(
                        &mut buffer,
                        &ellipse_path(Point::new(cx, cy), rx, ry),
                        style.line,
                        style.stroke_width,
                        style.color,
                    );
                }
            }
            Shape::Freehand { points, highlight } => {
                let (width, color) = stroke_appearance(style, *highlight);
                stroke_path(&mut buffer, points, style.line, width, color);
            }
            Shape::Marker { center, number } => {
                let geo = marker_geometry(style.stroke_width);
                raster::fill_ellipse(
                    &mut buffer,
                    (center.x, center.y),
                    geo.radius,
                    geo.radius,
                    style.color,
                );
                raster::stroke_ellipse(
                    &mut buffer,
                    (center.x, center.y),
                    geo.radius,
                    geo.radius,
                    geo.ring_width,
                    MARKER_INK,
                );
                let label = number.to_string();
                let (w, h) = text_extent(&font, &label, geo.font_size);
                // Centrado no disco: a caixa medida do número decide a âncora.
                let anchor = Point::new(center.x - w / 2.0, center.y - h / 2.0);
                draw_text(&mut buffer, &font, anchor, &label, MARKER_INK, geo.font_size);
            }
            // Já queimada na imagem de partida (ver `document::replay`), e
            // de propósito: o que for desenhado depois fica por cima dela.
            Shape::Redaction { .. } | Shape::Spotlight { .. } => {}
            Shape::Text { anchor, content } => {
                if style.text_pill {
                    let block = text_block_extent(&font, content, style.font_size);
                    let (pad, radius) = text_pill_metrics(block.line_height);
                    raster::fill_rect(
                        &mut buffer,
                        (anchor.x - pad, anchor.y - pad),
                        (anchor.x + block.width + pad, anchor.y + block.height + pad),
                        radius,
                        TEXT_PILL_COLOR,
                    );
                }
                draw_text(&mut buffer, &font, *anchor, content, style.color, style.font_size);
            }
        }
    }
    // A moldura é a última camada: ela emoldura o resultado de tudo.
    Ok(backdrop::compose(&buffer, backdrop))
}

/// Rasteriza um caminho no padrão de traço do estilo.
///
/// A quebra vem do `dash`, a mesma que o preview usa: é o que garante que o
/// tracejado do arquivo salvo seja o mesmo que estava na tela. As partes vão
/// juntas para o rasterizador, e não uma por vez, para serem compostas de uma
/// vez só — um rabisco de marca-texto que cruza a si mesmo ficaria com o
/// cruzamento mais escuro se cada pedaço se compusesse sozinho.
fn stroke_path(
    buffer: &mut RgbaImage,
    points: &[Point],
    line: LineStyle,
    width: f32,
    color: [u8; 4],
) {
    let partes: Vec<Vec<(f32, f32)>> = dash::split(points, line, width)
        .into_iter()
        .map(|parte| parte.into_iter().map(|p| (p.x, p.y)).collect())
        .collect();
    let caminhos: Vec<&[(f32, f32)]> = partes.iter().map(Vec::as_slice).collect();
    raster::stroke_polylines(buffer, &caminhos, width, color);
}

/// Largura e altura de uma linha de texto, na mesma métrica que `draw_text`
/// usa para posicioná-la — é o que permite centralizar o número do contador
/// no disco sem depender do egui.
fn text_extent(font: &FontRef<'_>, content: &str, font_size: f32) -> (f32, f32) {
    let scaled = font.as_scaled(ab_glyph::PxScale::from(font_size.max(1.0)));
    let mut width = 0.0;
    let mut previous: Option<ab_glyph::GlyphId> = None;
    for ch in content.chars().filter(|c| !c.is_control()) {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = previous {
            width += scaled.kern(prev, id);
        }
        width += scaled.h_advance(id);
        previous = Some(id);
    }
    (width, scaled.height())
}

/// Extensão de um bloco de texto: largura da linha mais larga e altura de
/// todas elas somadas, na mesma métrica que `draw_text` usa para desenhá-lo.
fn text_block_extent(font: &FontRef<'_>, content: &str, font_size: f32) -> TextBlock {
    let scaled = font.as_scaled(ab_glyph::PxScale::from(font_size.max(1.0)));
    let line_height = scaled.height() + scaled.line_gap();
    let mut width: f32 = 0.0;
    let mut lines = 0;
    // `split` e não `lines`: uma linha final vazia continua ocupando espaço,
    // e é assim que `draw_text` a desenha.
    for line in content.split('\n') {
        width = width.max(text_extent(font, line, font_size).0);
        lines += 1;
    }
    TextBlock { width, height: line_height * lines.max(1) as f32, line_height }
}

struct TextBlock {
    width: f32,
    height: f32,
    line_height: f32,
}

/// Desenha texto ancorado pelo canto superior esquerdo (como o
/// `painter.text(.., Align2::LEFT_TOP, ..)` do preview). Suporta múltiplas
/// linhas separadas por `\n`.
pub(crate) fn draw_text(
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
    use crate::editor::shapes::{
        shape_from_drag, RedactionStyle, SpotlightForm, Style, Tool, MAGNIFICATION_DEFAULT,
    };

    fn base() -> RgbaImage {
        RgbaImage::filled(64, 64, [10, 20, 30, 255])
    }

    fn style() -> Style {
        Style {
            color: [255, 0, 0, 255],
            stroke_width: 3.0,
            line: LineStyle::default(),
            font_size: 16.0,
            filled: false,
            corner_radius: 0.0,
            text_pill: false,
            redaction: RedactionStyle::default(),
            spotlight: SpotlightForm::default(),
            magnification: MAGNIFICATION_DEFAULT,
        }
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
        let out = render(&img, &[], BackdropStyle::None).unwrap();
        assert_eq!(img.as_raw(), out.as_raw());
    }

    #[test]
    fn render_line_touches_pixels() {
        let img = base();
        let out = render(&img, &[dragged(Tool::Line, (4.0, 4.0), (60.0, 60.0))], BackdropStyle::None).unwrap();
        assert_ne!(img.as_raw(), out.as_raw());
        // O pixel do meio da diagonal deve ter ficado avermelhado.
        let p = out.pixel(32, 32);
        assert!(p[0] > 100, "esperava traço vermelho, obtido {p:?}");
    }

    /// Quantos pixels a anotação pintou por cima da base.
    fn pintados(out: &RgbaImage) -> usize {
        out.as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| *p != &[10, 20, 30, 255])
            .count()
    }

    #[test]
    fn o_tracejado_pinta_menos_que_o_solido() {
        let img = base();
        let reta = |line| {
            let mut l = dragged(Tool::Line, (4.0, 32.0), (60.0, 32.0));
            l.style.line = line;
            pintados(&render(&img, &[l], BackdropStyle::None).unwrap())
        };
        let solido = reta(LineStyle::Solid);
        let tracejado = reta(LineStyle::Dashed);
        let pontilhado = reta(LineStyle::Dotted);
        assert!(tracejado < solido, "solido={solido} tracejado={tracejado}");
        assert!(pontilhado < tracejado, "tracejado={tracejado} pontilhado={pontilhado}");
        assert!(pontilhado > 0, "o pontilhado ainda tem de pintar algo");
    }

    #[test]
    fn o_cruzamento_de_um_marca_texto_tracejado_nao_escurece() {
        // As partes do padrão vão juntas para o rasterizador justamente por
        // isto: compostas uma a uma, o ponto onde o rabisco passa por cima de
        // si mesmo receberia a cor duas vezes.
        let img = base();
        // Vai e volta pela mesma reta: o padrão continua contando o caminho
        // percorrido, então os traços da volta caem por cima dos da ida em
        // outro compasso e a sobreposição é certa.
        let mut sobreposto = layer(Shape::Freehand {
            points: vec![
                Point::new(8.0, 32.0),
                Point::new(56.0, 32.0),
                Point::new(8.0, 32.0),
            ],
            highlight: true,
        });
        sobreposto.style.line = LineStyle::Dashed;
        let mut simples = sobreposto.clone();
        simples.shape = Shape::Freehand {
            points: vec![Point::new(8.0, 32.0), Point::new(56.0, 32.0)],
            highlight: true,
        };

        let duplo = render(&img, &[sobreposto], BackdropStyle::None).unwrap();
        let unico = render(&img, &[simples], BackdropStyle::None).unwrap();
        let mais_vermelho = |img: &RgbaImage| (10..54).map(|x| img.pixel(x, 32)[0]).max().unwrap();
        assert!(
            mais_vermelho(&duplo) <= mais_vermelho(&unico),
            "a sobreposição escureceu: {} contra {}",
            mais_vermelho(&duplo),
            mais_vermelho(&unico)
        );
    }

    #[test]
    fn a_regua_sai_com_as_pontas_e_o_valor() {
        let img = base();
        let regua = layer(Shape::Ruler {
            a: Point::new(6.0, 32.0),
            b: Point::new(58.0, 32.0),
        });
        let out = render(&img, &[regua], BackdropStyle::None).unwrap();
        // Mesma reta, sem as pontas nem o rótulo: a régua tem de pintar mais.
        let reta = render(
            &img,
            &[dragged(Tool::Line, (6.0, 32.0), (58.0, 32.0))],
            BackdropStyle::None,
        )
        .unwrap();
        assert!(
            pintados(&out) > pintados(&reta),
            "régua={} reta={}",
            pintados(&out),
            pintados(&reta)
        );
        // No meio fica a pílula do rótulo, com o número em cima dela: o
        // branco do texto só existe se o rótulo foi rasterizado.
        let branco = (24..40)
            .flat_map(|x| (24..40).map(move |y| (x, y)))
            .any(|(x, y)| {
                let p = out.pixel(x, y);
                p[1] > 180 && p[2] > 180
            });
        assert!(branco, "o valor medido não apareceu");
    }

    #[test]
    fn render_text_touches_pixels() {
        let img = base();
        let text = layer(Shape::Text { anchor: Point::new(2.0, 2.0), content: "Ok".into() });
        let out = render(&img, &[text], BackdropStyle::None).unwrap();
        assert_ne!(img.as_raw(), out.as_raw());
    }

    #[test]
    fn dimensions_preserved() {
        let img = base();
        let out = render(&img, &[dragged(Tool::Rect, (1.0, 1.0), (50.0, 40.0))], BackdropStyle::None).unwrap();
        assert_eq!((out.width(), out.height()), (64, 64));
    }

    #[test]
    fn arrow_head_is_filled() {
        let img = base();
        let out = render(&img, &[dragged(Tool::Arrow, (8.0, 32.0), (56.0, 32.0))], BackdropStyle::None).unwrap();
        // Perto da ponta (x=54, y=32) deve haver vermelho.
        assert!(out.pixel(53, 32)[0] > 150);
    }

    #[test]
    fn freehand_stroke_is_exported() {
        let img = base();
        let points: Vec<Point> = (0..6).map(|i| Point::new(8.0 + i as f32 * 9.0, 32.0)).collect();
        let out = render(&img, &[layer(Shape::Freehand { points, highlight: false })], BackdropStyle::None).unwrap();
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

        let opaque = render(&img, &[plain], BackdropStyle::None).unwrap().pixel(32, 32);
        let translucent = render(&img, &[mark], BackdropStyle::None).unwrap().pixel(32, 32);
        assert_eq!(opaque[0], 255, "mão livre cobre");
        assert!(translucent[0] < opaque[0], "marca-texto não cobre");
        assert!(translucent[2] > opaque[2], "o azul do fundo sobrevive");
    }

    fn styled(shape: Shape, style: Style) -> Layer {
        Layer { id: 1, shape, style }
    }

    #[test]
    fn a_filled_rect_paints_its_interior() {
        let img = base();
        let shape = Shape::Rect { min: Point::new(16.0, 16.0), max: Point::new(48.0, 48.0) };
        let hollow = render(&img, &[layer(shape.clone())], BackdropStyle::None).unwrap();
        let solid = render(
            &img,
            &[styled(shape, Style { filled: true, ..style() })], BackdropStyle::None)
        .unwrap();
        assert_eq!(hollow.pixel(32, 32), [10, 20, 30, 255], "vazado deixa o miolo");
        assert_eq!(solid.pixel(32, 32)[0], 255, "cheio pinta o miolo");
    }

    #[test]
    fn the_corner_radius_rounds_a_filled_rect() {
        let img = base();
        let shape = Shape::Rect { min: Point::new(16.0, 16.0), max: Point::new(48.0, 48.0) };
        let out = render(
            &img,
            &[styled(shape, Style { filled: true, corner_radius: 12.0, ..style() })], BackdropStyle::None)
        .unwrap();
        assert_eq!(out.pixel(32, 32)[0], 255, "miolo cheio");
        assert_eq!(out.pixel(17, 17), [10, 20, 30, 255], "canto recuado pelo raio");
        assert_eq!(out.pixel(32, 17)[0], 255, "meio da aresta continua cheio");
    }

    #[test]
    fn a_filled_ellipse_paints_its_interior() {
        let img = base();
        let shape = Shape::Ellipse { center: Point::new(32.0, 32.0), rx: 16.0, ry: 10.0 };
        let out = render(&img, &[styled(shape, Style { filled: true, ..style() })], BackdropStyle::None).unwrap();
        assert_eq!(out.pixel(32, 32)[0], 255, "centro cheio");
        assert_eq!(out.pixel(32, 4), [10, 20, 30, 255], "fora da elipse");
    }

    #[test]
    fn a_marker_draws_a_disc_with_a_light_ring() {
        let img = base();
        let shape = Shape::Marker { center: Point::new(32.0, 32.0), number: 7 };
        let out = render(&img, &[layer(shape)], BackdropStyle::None).unwrap();
        // Traço 3 → diâmetro mínimo 24, raio 12.
        assert_eq!(out.pixel(32, 44 - 1)[0], 255, "borda do disco na cor ativa");
        assert_eq!(out.pixel(32, 8), [10, 20, 30, 255], "fora do disco");
        // O número claro aparece no meio, sobre a cor cheia.
        let center = out.pixel(32, 32);
        assert!(center[1] > 100 && center[2] > 100, "tinta clara no miolo, veio {center:?}");
    }

    #[test]
    fn marker_text_is_centred_regardless_of_the_digit_count() {
        // Um número de dois dígitos tem de ficar tão centrado quanto um de um
        // só — é o que a medida do texto na exportação garante.
        let img = base();
        let at = Point::new(32.0, 32.0);
        let one = render(&img, &[layer(Shape::Marker { center: at, number: 1 })], BackdropStyle::None).unwrap();
        let twelve = render(&img, &[layer(Shape::Marker { center: at, number: 12 })], BackdropStyle::None).unwrap();

        let ink_bounds = |img: &RgbaImage| {
            let (mut lo, mut hi) = (u32::MAX, 0);
            for x in 20..45 {
                // Pixels claros do número, distinguidos do vermelho do disco.
                if (20..45).any(|y| img.pixel(x, y)[2] > 120) {
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
            (lo + hi) / 2
        };
        let (a, b) = (ink_bounds(&one), ink_bounds(&twelve));
        assert!(a.abs_diff(b) <= 1, "centros em {a} e {b}");
    }

    #[test]
    fn the_reading_pill_sits_behind_the_text() {
        let img = base();
        let shape = Shape::Text { anchor: Point::new(20.0, 20.0), content: "Ok".into() };
        let plain = render(&img, &[layer(shape.clone())], BackdropStyle::None).unwrap();
        let on_pill = render(
            &img,
            &[styled(shape, Style { text_pill: true, ..style() })], BackdropStyle::None)
        .unwrap();
        // Logo acima e à esquerda da âncora cai o recuo da pílula.
        assert_eq!(plain.pixel(18, 18), [10, 20, 30, 255], "sem pílula, o fundo aparece");
        let pill = on_pill.pixel(18, 18);
        assert!(pill[0] > 200 && pill[1] > 200 && pill[2] > 200, "creme claro, veio {pill:?}");
    }

    #[test]
    fn a_multiline_text_grows_downwards() {
        let img = RgbaImage::filled(120, 120, [10, 20, 30, 255]);
        let one = Shape::Text { anchor: Point::new(8.0, 8.0), content: "Ok".into() };
        let two = Shape::Text { anchor: Point::new(8.0, 8.0), content: "Ok\nOk".into() };

        let lowest_ink = |img: &RgbaImage| {
            (0..120).rev().find(|&y| (0..120).any(|x| img.pixel(x, y)[0] > 100))
        };
        let a = lowest_ink(&render(&img, &[layer(one)], BackdropStyle::None).unwrap()).unwrap();
        let b = lowest_ink(&render(&img, &[layer(two)], BackdropStyle::None).unwrap()).unwrap();
        assert!(b > a, "a segunda linha desce ({a} → {b})");
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
        let a = render(&img, &[red], BackdropStyle::None).unwrap();
        let b = render(&img, &[green], BackdropStyle::None).unwrap();
        assert!(a.pixel(8, 32)[0] > 150, "primeira camada vermelha");
        assert!(b.pixel(8, 32)[1] > 150, "segunda camada verde");
    }
}
