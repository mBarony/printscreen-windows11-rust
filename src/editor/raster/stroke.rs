//! Primitivas de contorno do rasterizador.

use super::{
    blend, clamp_radius, clip_bbox, dist_sq_to_segment, inside_ellipse, inside_round_rect,
    rasterize, subsample_at, P, SUBSAMPLES,
};
use crate::imgbuf::RgbaImage;

/// Segmento com espessura `width` e pontas redondas.
pub fn stroke_line(img: &mut RgbaImage, a: P, b: P, width: f32, color: [u8; 4]) {
    let r = width.max(0.5) / 2.0;
    let r_sq = r * r;
    let bbox = (
        a.0.min(b.0) - r - 1.0,
        a.1.min(b.1) - r - 1.0,
        a.0.max(b.0) + r + 1.0,
        a.1.max(b.1) + r + 1.0,
    );
    rasterize(img, bbox, color, |x, y| dist_sq_to_segment((x, y), a, b) <= r_sq);
}

/// Retângulo em traço: união dos 4 lados como segmentos (juntas redondas).
pub fn stroke_rect(img: &mut RgbaImage, min: P, max: P, width: f32, color: [u8; 4]) {
    let r = width.max(0.5) / 2.0;
    let r_sq = r * r;
    let corners = [min, (max.0, min.1), max, (min.0, max.1)];
    let bbox = (min.0 - r - 1.0, min.1 - r - 1.0, max.0 + r + 1.0, max.1 + r + 1.0);
    rasterize(img, bbox, color, |x, y| {
        let p = (x, y);
        (0..4).any(|i| dist_sq_to_segment(p, corners[i], corners[(i + 1) % 4]) <= r_sq)
    });
}

/// Retângulo de cantos arredondados em traço: anel entre o retângulo externo
/// (`+½ traço`) e o interno (`−½ traço`).
pub fn stroke_round_rect(img: &mut RgbaImage, min: P, max: P, radius: f32, width: f32, color: [u8; 4]) {
    let half = width.max(0.5) / 2.0;
    let r = clamp_radius(min, max, radius);
    let outer = ((min.0 - half, min.1 - half), (max.0 + half, max.1 + half));
    let inner = ((min.0 + half, min.1 + half), (max.0 - half, max.1 - half));
    // Traço mais grosso que o retângulo: vira preenchimento sólido.
    let inner_empty = inner.0 .0 >= inner.1 .0 || inner.0 .1 >= inner.1 .1;
    let bbox = (
        outer.0 .0 - 1.0,
        outer.0 .1 - 1.0,
        outer.1 .0 + 1.0,
        outer.1 .1 + 1.0,
    );
    rasterize(img, bbox, color, |x, y| {
        let p = (x, y);
        inside_round_rect(p, outer.0, outer.1, r + half)
            && (inner_empty || !inside_round_rect(p, inner.0, inner.1, (r - half).max(0.0)))
    });
}

/// Elipse em traço: anel entre as elipses de raios `±width/2`.
pub fn stroke_ellipse(img: &mut RgbaImage, center: P, rx: f32, ry: f32, width: f32, color: [u8; 4]) {
    let half = width.max(0.5) / 2.0;
    let (outer_rx, outer_ry) = (rx + half, ry + half);
    let (inner_rx, inner_ry) = ((rx - half).max(0.0), (ry - half).max(0.0));
    let bbox = (
        center.0 - outer_rx - 1.0,
        center.1 - outer_ry - 1.0,
        center.0 + outer_rx + 1.0,
        center.1 + outer_ry + 1.0,
    );
    rasterize(img, bbox, color, |x, y| {
        let p = (x, y);
        inside_ellipse(p, center, outer_rx, outer_ry) && !inside_ellipse(p, center, inner_rx, inner_ry)
    });
}

/// Traço contínuo por uma sequência de pontos (mão livre e marca-texto).
///
/// A cobertura de todos os segmentos é acumulada num bitmask de 16 bits por
/// pixel — um bit por subamostra — e composta **uma única vez** no fim. Sem
/// isso, cada segmento faria sua própria composição e as junções ficariam
/// mais escuras que o resto do traço, o que é justamente onde o marca-texto
/// (translúcido, alfa 120) denunciaria o problema.
pub fn stroke_polyline(img: &mut RgbaImage, points: &[P], width: f32, color: [u8; 4]) {
    let (first, rest) = match points.split_first() {
        Some(split) => split,
        None => return,
    };
    if rest.is_empty() {
        // Um toque sem arrasto ainda deixa a marca redonda da ponta.
        stroke_line(img, *first, *first, width, color);
        return;
    }

    let r = width.max(0.5) / 2.0;
    let Some(area) = clip_bbox(img, polyline_bbox(points, r)) else {
        return;
    };
    let (x0, y0, x1, y1) = area;
    let stride = (x1 - x0) as usize;
    let mut mask = vec![0u16; stride * (y1 - y0) as usize];

    for segment in points.windows(2) {
        accumulate_segment(&mut mask, area, segment[0], segment[1], r);
    }

    for py in y0..y1 {
        for px in x0..x1 {
            let bits = mask[(py - y0) as usize * stride + (px - x0) as usize];
            if bits != 0 {
                blend(img.pixel_mut(px, py), color, bits.count_ones() as f32 / SUBSAMPLES as f32);
            }
        }
    }
}

fn polyline_bbox(points: &[P], r: f32) -> (f32, f32, f32, f32) {
    let mut bbox = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in points {
        bbox.0 = bbox.0.min(x);
        bbox.1 = bbox.1.min(y);
        bbox.2 = bbox.2.max(x);
        bbox.3 = bbox.3.max(y);
    }
    (bbox.0 - r - 1.0, bbox.1 - r - 1.0, bbox.2 + r + 1.0, bbox.3 + r + 1.0)
}

/// Marca na máscara as subamostras cobertas por um segmento, varrendo apenas
/// o pedaço da área que ele pode alcançar.
fn accumulate_segment(mask: &mut [u16], area: (u32, u32, u32, u32), a: P, b: P, r: f32) {
    let (x0, y0, x1, y1) = area;
    let stride = (x1 - x0) as usize;
    let r_sq = r * r;

    let clamp_x = |v: f32| (v.max(x0 as f32) as u32).clamp(x0, x1);
    let clamp_y = |v: f32| (v.max(y0 as f32) as u32).clamp(y0, y1);
    let sx0 = clamp_x((a.0.min(b.0) - r - 1.0).floor());
    let sy0 = clamp_y((a.1.min(b.1) - r - 1.0).floor());
    let sx1 = clamp_x((a.0.max(b.0) + r + 1.0).ceil());
    let sy1 = clamp_y((a.1.max(b.1) + r + 1.0).ceil());

    for py in sy0..sy1 {
        for px in sx0..sx1 {
            let cell = &mut mask[(py - y0) as usize * stride + (px - x0) as usize];
            if *cell == u16::MAX {
                continue; // pixel já totalmente coberto por um segmento anterior
            }
            for s in 0..SUBSAMPLES {
                let bit = 1u16 << s;
                if *cell & bit != 0 {
                    continue;
                }
                if dist_sq_to_segment(subsample_at(px, py, s), a, b) <= r_sq {
                    *cell |= bit;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> RgbaImage {
        RgbaImage::filled(64, 64, [0, 0, 0, 255])
    }

    fn red() -> [u8; 4] {
        [255, 0, 0, 255]
    }

    #[test]
    fn line_covers_center_and_antialiases_edges() {
        let mut img = canvas();
        stroke_line(&mut img, (8.0, 32.0), (56.0, 32.0), 4.0, red());
        assert_eq!(img.pixel(32, 32)[0], 255, "centro do traço");
        assert_eq!(img.pixel(32, 8), [0, 0, 0, 255], "longe do traço");
    }

    #[test]
    fn rect_stroke_leaves_hole() {
        let mut img = canvas();
        stroke_rect(&mut img, (16.0, 16.0), (48.0, 48.0), 3.0, red());
        assert!(img.pixel(16, 32)[0] > 200, "borda esquerda pintada");
        assert_eq!(img.pixel(32, 32), [0, 0, 0, 255], "miolo vazio");
    }

    #[test]
    fn ellipse_ring() {
        let mut img = canvas();
        stroke_ellipse(&mut img, (32.0, 32.0), 20.0, 12.0, 3.0, red());
        assert!(img.pixel(52, 32)[0] > 200, "borda direita (rx)");
        assert_eq!(img.pixel(32, 32), [0, 0, 0, 255], "centro vazio");
        assert_eq!(img.pixel(32, 4), [0, 0, 0, 255], "fora");
    }

    #[test]
    fn round_rect_stroke_keeps_hole_and_rounds_corners() {
        let mut img = canvas();
        stroke_round_rect(&mut img, (12.0, 12.0), (52.0, 52.0), 10.0, 3.0, red());
        assert!(img.pixel(32, 12)[0] > 200, "aresta superior pintada");
        assert_eq!(img.pixel(32, 32), [0, 0, 0, 255], "miolo vazio");
        assert_eq!(img.pixel(12, 12), [0, 0, 0, 255], "canto recuado pelo raio");
    }

    #[test]
    fn round_rect_stroke_thicker_than_the_box_fills_it() {
        let mut img = canvas();
        stroke_round_rect(&mut img, (30.0, 30.0), (34.0, 34.0), 2.0, 20.0, red());
        assert!(img.pixel(32, 32)[0] > 200, "traço grosso preenche o miolo");
    }

    #[test]
    fn polyline_joints_do_not_darken_a_translucent_stroke() {
        // Cobertura acumulada em máscara: a junção deve ficar igual ao meio
        // de um segmento reto, não mais opaca.
        let translucent = [255, 0, 0, 120];
        let mut bent = canvas();
        stroke_polyline(
            &mut bent,
            &[(10.0, 10.0), (32.0, 32.0), (54.0, 10.0)],
            8.0,
            translucent,
        );
        let mut straight = canvas();
        stroke_polyline(&mut straight, &[(10.0, 32.0), (54.0, 32.0)], 8.0, translucent);
        assert_eq!(
            bent.pixel(32, 32)[0],
            straight.pixel(32, 32)[0],
            "a junção não pode escurecer"
        );
    }

    #[test]
    fn polyline_draws_a_continuous_stroke() {
        let mut img = canvas();
        stroke_polyline(&mut img, &[(8.0, 32.0), (32.0, 32.0), (56.0, 32.0)], 4.0, red());
        for x in [10, 20, 32, 44, 54] {
            assert!(img.pixel(x, 32)[0] > 200, "traço contínuo em x={x}");
        }
        assert_eq!(img.pixel(32, 8), [0, 0, 0, 255], "fora do traço");
    }

    #[test]
    fn polyline_with_a_single_point_marks_the_cap() {
        let mut img = canvas();
        stroke_polyline(&mut img, &[(32.0, 32.0)], 10.0, red());
        assert!(img.pixel(32, 32)[0] > 200, "ponta redonda desenhada");
    }

    #[test]
    fn polyline_with_no_points_is_a_noop() {
        let mut img = canvas();
        stroke_polyline(&mut img, &[], 4.0, red());
        assert_eq!(img.pixel(32, 32), [0, 0, 0, 255]);
    }

    #[test]
    fn clipping_at_image_borders_does_not_panic() {
        let mut img = canvas();
        stroke_line(&mut img, (-20.0, -20.0), (100.0, 100.0), 6.0, red());
        stroke_ellipse(&mut img, (0.0, 0.0), 50.0, 50.0, 4.0, red());
        stroke_rect(&mut img, (-10.0, -10.0), (80.0, 80.0), 5.0, red());
        stroke_round_rect(&mut img, (-10.0, -10.0), (80.0, 80.0), 8.0, 5.0, red());
        stroke_polyline(&mut img, &[(-30.0, -30.0), (100.0, 100.0)], 6.0, red());
        // Inteiramente fora: não pinta nada e não estoura.
        stroke_polyline(&mut img, &[(-90.0, -90.0), (-80.0, -80.0)], 4.0, red());
    }
}
