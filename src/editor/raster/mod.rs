//! Rasterizador vetorial próprio para a exportação (substitui `tiny-skia`).
//!
//! Cobertura por superamostragem 4×4 dentro do bounding box de cada forma:
//! qualidade de anti-aliasing equivalente à do preview do egui para os
//! traços de 1–12 px usados nas anotações.
//!
//! Este módulo é só o motor de amostragem e composição; as primitivas ficam
//! em [`stroke`] (contornos) e [`fill`] (preenchimentos), reexportadas aqui
//! para que os chamadores continuem escrevendo `raster::stroke_line(…)`.

pub mod fill;
pub mod stroke;

pub use fill::{fill_ellipse, fill_rect, fill_triangle};
pub use stroke::{
    stroke_ellipse, stroke_line, stroke_polyline, stroke_rect, stroke_round_rect,
};

use crate::imgbuf::RgbaImage;

/// Subamostras por eixo (4×4 = 16 por pixel).
pub(super) const SS: u32 = 4;

/// Total de subamostras por pixel — cabe num `u16`, que é o que permite
/// acumular a união exata de várias formas (ver [`stroke_polyline`]).
pub(super) const SUBSAMPLES: u32 = SS * SS;

pub(super) type P = (f32, f32);

/// Composição src-over com cor não pré-multiplicada.
#[inline]
pub(super) fn blend(pixel: &mut [u8], color: [u8; 4], coverage: f32) {
    let alpha = (color[3] as f32 / 255.0) * coverage;
    if alpha <= 0.0 {
        return;
    }
    for i in 0..3 {
        let src = color[i] as f32;
        let dst = pixel[i] as f32;
        pixel[i] = (src * alpha + dst * (1.0 - alpha)).round() as u8;
    }
    let dst_a = pixel[3] as f32 / 255.0;
    pixel[3] = ((alpha + dst_a * (1.0 - alpha)) * 255.0).round() as u8;
}

/// Recorta um bounding box em coordenadas de pixel inteiras dentro da imagem.
/// Devolve `None` quando a forma cai inteiramente fora.
#[inline]
pub(super) fn clip_bbox(img: &RgbaImage, bbox: (f32, f32, f32, f32)) -> Option<(u32, u32, u32, u32)> {
    let (min_x, min_y, max_x, max_y) = bbox;
    let x0 = min_x.floor().max(0.0) as u32;
    let y0 = min_y.floor().max(0.0) as u32;
    let x1 = (max_x.ceil() as i64).clamp(0, img.width() as i64) as u32;
    let y1 = (max_y.ceil() as i64).clamp(0, img.height() as i64) as u32;
    (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
}

/// Posição de uma subamostra dentro do pixel `(px, py)`.
#[inline]
pub(super) fn subsample_at(px: u32, py: u32, index: u32) -> P {
    let step = 1.0 / SS as f32;
    let offset = step / 2.0;
    let sx = index % SS;
    let sy = index / SS;
    (
        px as f32 + offset + sx as f32 * step,
        py as f32 + offset + sy as f32 * step,
    )
}

/// Varre o bounding box avaliando `inside` em 16 subamostras por pixel.
pub(super) fn rasterize(
    img: &mut RgbaImage,
    bbox: (f32, f32, f32, f32),
    color: [u8; 4],
    inside: impl Fn(f32, f32) -> bool,
) {
    let Some((x0, y0, x1, y1)) = clip_bbox(img, bbox) else {
        return;
    };
    for py in y0..y1 {
        for px in x0..x1 {
            let mut hits = 0u32;
            for s in 0..SUBSAMPLES {
                let (x, y) = subsample_at(px, py, s);
                if inside(x, y) {
                    hits += 1;
                }
            }
            if hits > 0 {
                blend(img.pixel_mut(px, py), color, hits as f32 / SUBSAMPLES as f32);
            }
        }
    }
}

/// Distância² de um ponto ao segmento `a`–`b`.
#[inline]
pub(super) fn dist_sq_to_segment(p: P, a: P, b: P) -> f32 {
    let (px, py) = p;
    let (ax, ay) = a;
    let (bx, by) = b;
    let (dx, dy) = (bx - ax, by - ay);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    let (ex, ey) = (px - cx, py - cy);
    ex * ex + ey * ey
}

/// Ponto dentro de um retângulo de cantos arredondados.
///
/// O truque é projetar o ponto no centro do arco mais próximo: na faixa
/// central `clamp` devolve o próprio ponto (distância zero, dentro), e só
/// nos quatro cantos o teste vira o do círculo.
#[inline]
pub(super) fn inside_round_rect(p: P, min: P, max: P, radius: f32) -> bool {
    if p.0 < min.0 || p.0 > max.0 || p.1 < min.1 || p.1 > max.1 {
        return false;
    }
    let r = clamp_radius(min, max, radius);
    if r <= 0.0 {
        return true;
    }
    let cx = p.0.clamp(min.0 + r, max.0 - r);
    let cy = p.1.clamp(min.1 + r, max.1 - r);
    let (dx, dy) = (p.0 - cx, p.1 - cy);
    dx * dx + dy * dy <= r * r
}

/// Limita o raio a metade do menor lado — sem isso o `clamp` de
/// [`inside_round_rect`] receberia um intervalo invertido e entraria em pânico.
#[inline]
pub(super) fn clamp_radius(min: P, max: P, radius: f32) -> f32 {
    let half_w = (max.0 - min.0) / 2.0;
    let half_h = (max.1 - min.1) / 2.0;
    radius.max(0.0).min(half_w.max(0.0)).min(half_h.max(0.0))
}

/// Ponto dentro da elipse de raios `(rx, ry)` centrada em `center`.
#[inline]
pub(super) fn inside_ellipse(p: P, center: P, rx: f32, ry: f32) -> bool {
    if rx <= f32::EPSILON || ry <= f32::EPSILON {
        return false;
    }
    let nx = (p.0 - center.0) / rx;
    let ny = (p.1 - center.1) / ry;
    nx * nx + ny * ny <= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_rect_radius_is_clamped_to_half_the_shorter_side() {
        // Raio maior que o retângulo não pode estourar o `clamp` interno.
        let (min, max) = ((0.0, 0.0), (10.0, 4.0));
        assert_eq!(clamp_radius(min, max, 100.0), 2.0);
        assert!(inside_round_rect((5.0, 2.0), min, max, 100.0), "centro dentro");
        assert!(!inside_round_rect((0.0, 0.0), min, max, 100.0), "canto arredondado fora");
    }

    #[test]
    fn round_rect_with_zero_radius_is_a_plain_rect() {
        let (min, max) = ((0.0, 0.0), (10.0, 10.0));
        assert!(inside_round_rect((0.0, 0.0), min, max, 0.0));
        assert!(!inside_round_rect((10.1, 5.0), min, max, 0.0));
    }

    #[test]
    fn subsamples_cover_the_pixel_without_touching_its_borders() {
        for s in 0..SUBSAMPLES {
            let (x, y) = subsample_at(3, 7, s);
            assert!(x > 3.0 && x < 4.0, "subamostra {s} em x");
            assert!(y > 7.0 && y < 8.0, "subamostra {s} em y");
        }
    }

    #[test]
    fn degenerate_ellipse_is_never_inside() {
        assert!(!inside_ellipse((0.0, 0.0), (0.0, 0.0), 0.0, 5.0));
    }
}
