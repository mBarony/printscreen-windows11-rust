//! Rasterizador vetorial próprio para a exportação (substitui `tiny-skia`).
//!
//! Cobertura por superamostragem 4×4 dentro do bounding box de cada forma:
//! qualidade de anti-aliasing equivalente à do preview do egui para os
//! traços de 1–12 px usados nas anotações. Todas as formas do editor se
//! reduzem a quatro primitivas: segmento com pontas redondas, triângulo
//! preenchido, retângulo em traço (união de 4 segmentos → juntas redondas)
//! e elipse em traço (anel entre duas elipses ±½ traço).

use crate::imgbuf::RgbaImage;

const SS: u32 = 4; // subamostras por eixo (4×4 = 16 por pixel)

type P = (f32, f32);

#[inline]
fn blend(pixel: &mut [u8], color: [u8; 4], coverage: f32) {
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

/// Varre o bounding box avaliando `inside` em 16 subamostras por pixel.
fn rasterize(
    img: &mut RgbaImage,
    bbox: (f32, f32, f32, f32),
    color: [u8; 4],
    inside: impl Fn(f32, f32) -> bool,
) {
    let (min_x, min_y, max_x, max_y) = bbox;
    let x0 = min_x.floor().max(0.0) as u32;
    let y0 = min_y.floor().max(0.0) as u32;
    let x1 = (max_x.ceil() as i64).clamp(0, img.width() as i64) as u32;
    let y1 = (max_y.ceil() as i64).clamp(0, img.height() as i64) as u32;

    let step = 1.0 / SS as f32;
    let offset = step / 2.0;
    for py in y0..y1 {
        for px in x0..x1 {
            let mut hits = 0u32;
            for sy in 0..SS {
                let y = py as f32 + offset + sy as f32 * step;
                for sx in 0..SS {
                    let x = px as f32 + offset + sx as f32 * step;
                    if inside(x, y) {
                        hits += 1;
                    }
                }
            }
            if hits > 0 {
                let coverage = hits as f32 / (SS * SS) as f32;
                blend(img.pixel_mut(px, py), color, coverage);
            }
        }
    }
}

/// Distância² de um ponto ao segmento `a`–`b`.
#[inline]
fn dist_sq_to_segment(p: P, a: P, b: P) -> f32 {
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

/// Segmento com espessura `width` e pontas redondas.
pub fn stroke_line(img: &mut RgbaImage, a: P, b: P, width: f32, color: [u8; 4]) {
    let r = (width.max(0.5)) / 2.0;
    let r_sq = r * r;
    let bbox = (
        a.0.min(b.0) - r - 1.0,
        a.1.min(b.1) - r - 1.0,
        a.0.max(b.0) + r + 1.0,
        a.1.max(b.1) + r + 1.0,
    );
    rasterize(img, bbox, color, |x, y| dist_sq_to_segment((x, y), a, b) <= r_sq);
}

/// Triângulo preenchido (ponta da seta).
pub fn fill_triangle(img: &mut RgbaImage, p0: P, p1: P, p2: P, color: [u8; 4]) {
    #[inline]
    fn cross(o: P, a: P, b: P) -> f32 {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    }
    let bbox = (
        p0.0.min(p1.0).min(p2.0) - 1.0,
        p0.1.min(p1.1).min(p2.1) - 1.0,
        p0.0.max(p1.0).max(p2.0) + 1.0,
        p0.1.max(p1.1).max(p2.1) + 1.0,
    );
    // Normaliza o sentido para os três produtos vetoriais terem o mesmo sinal.
    let area = cross(p0, p1, p2);
    if area.abs() <= f32::EPSILON {
        return;
    }
    let flip = area.signum();
    rasterize(img, bbox, color, |x, y| {
        let p = (x, y);
        cross(p0, p1, p) * flip >= 0.0
            && cross(p1, p2, p) * flip >= 0.0
            && cross(p2, p0, p) * flip >= 0.0
    });
}

/// Retângulo em traço: união dos 4 lados como segmentos (juntas redondas).
pub fn stroke_rect(img: &mut RgbaImage, min: P, max: P, width: f32, color: [u8; 4]) {
    let r = (width.max(0.5)) / 2.0;
    let r_sq = r * r;
    let corners = [min, (max.0, min.1), max, (min.0, max.1)];
    let bbox = (min.0 - r - 1.0, min.1 - r - 1.0, max.0 + r + 1.0, max.1 + r + 1.0);
    rasterize(img, bbox, color, |x, y| {
        let p = (x, y);
        (0..4).any(|i| dist_sq_to_segment(p, corners[i], corners[(i + 1) % 4]) <= r_sq)
    });
}

/// Elipse em traço: anel entre as elipses de raios `±width/2`.
pub fn stroke_ellipse(img: &mut RgbaImage, center: P, rx: f32, ry: f32, width: f32, color: [u8; 4]) {
    let half = (width.max(0.5)) / 2.0;
    let (outer_rx, outer_ry) = (rx + half, ry + half);
    let (inner_rx, inner_ry) = ((rx - half).max(0.0), (ry - half).max(0.0));
    let bbox = (
        center.0 - outer_rx - 1.0,
        center.1 - outer_ry - 1.0,
        center.0 + outer_rx + 1.0,
        center.1 + outer_ry + 1.0,
    );
    let inside_ellipse = |dx: f32, dy: f32, ex: f32, ey: f32| -> bool {
        if ex <= f32::EPSILON || ey <= f32::EPSILON {
            return false;
        }
        let nx = dx / ex;
        let ny = dy / ey;
        nx * nx + ny * ny <= 1.0
    };
    rasterize(img, bbox, color, |x, y| {
        let dx = x - center.0;
        let dy = y - center.1;
        inside_ellipse(dx, dy, outer_rx, outer_ry) && !inside_ellipse(dx, dy, inner_rx, inner_ry)
    });
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
        // Centro do traço: cobertura total.
        assert_eq!(img.pixel(32, 32)[0], 255);
        // Longe do traço: intocado.
        assert_eq!(img.pixel(32, 8), [0, 0, 0, 255]);
    }

    #[test]
    fn triangle_fills_interior_only() {
        let mut img = canvas();
        fill_triangle(&mut img, (32.0, 8.0), (56.0, 56.0), (8.0, 56.0), red());
        assert_eq!(img.pixel(32, 40)[0], 255, "interior");
        assert_eq!(img.pixel(8, 8), [0, 0, 0, 255], "exterior");
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
    fn clipping_at_image_borders_does_not_panic() {
        let mut img = canvas();
        stroke_line(&mut img, (-20.0, -20.0), (100.0, 100.0), 6.0, red());
        stroke_ellipse(&mut img, (0.0, 0.0), 50.0, 50.0, 4.0, red());
        stroke_rect(&mut img, (-10.0, -10.0), (80.0, 80.0), 5.0, red());
        fill_triangle(&mut img, (-5.0, 70.0), (70.0, -5.0), (70.0, 70.0), red());
    }
}
