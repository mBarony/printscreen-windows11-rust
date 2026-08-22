//! Primitivas de preenchimento do rasterizador.

use super::{inside_ellipse, inside_round_rect, rasterize, P};
use crate::imgbuf::RgbaImage;

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

/// Retângulo preenchido; `radius` acima de zero arredonda os cantos.
pub fn fill_rect(img: &mut RgbaImage, min: P, max: P, radius: f32, color: [u8; 4]) {
    if min.0 >= max.0 || min.1 >= max.1 {
        return;
    }
    let bbox = (min.0 - 1.0, min.1 - 1.0, max.0 + 1.0, max.1 + 1.0);
    rasterize(img, bbox, color, |x, y| {
        inside_round_rect((x, y), min, max, radius)
    });
}

/// Elipse preenchida (também serve de disco, com `rx == ry`).
pub fn fill_ellipse(img: &mut RgbaImage, center: P, rx: f32, ry: f32, color: [u8; 4]) {
    let bbox = (
        center.0 - rx - 1.0,
        center.1 - ry - 1.0,
        center.0 + rx + 1.0,
        center.1 + ry + 1.0,
    );
    rasterize(img, bbox, color, |x, y| inside_ellipse((x, y), center, rx, ry));
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
    fn triangle_fills_interior_only() {
        let mut img = canvas();
        fill_triangle(&mut img, (32.0, 8.0), (56.0, 56.0), (8.0, 56.0), red());
        assert_eq!(img.pixel(32, 40)[0], 255, "interior");
        assert_eq!(img.pixel(8, 8), [0, 0, 0, 255], "exterior");
    }

    #[test]
    fn rect_fills_the_whole_interior() {
        let mut img = canvas();
        fill_rect(&mut img, (16.0, 16.0), (48.0, 48.0), 0.0, red());
        assert_eq!(img.pixel(32, 32)[0], 255, "miolo preenchido");
        assert_eq!(img.pixel(20, 20)[0], 255, "perto da borda");
        assert_eq!(img.pixel(8, 8), [0, 0, 0, 255], "fora");
    }

    #[test]
    fn rounded_rect_cuts_the_corners() {
        let mut img = canvas();
        fill_rect(&mut img, (16.0, 16.0), (48.0, 48.0), 12.0, red());
        assert_eq!(img.pixel(32, 32)[0], 255, "miolo preenchido");
        assert_eq!(img.pixel(17, 17), [0, 0, 0, 255], "canto recuado pelo raio");
        assert_eq!(img.pixel(32, 17)[0], 255, "meio da aresta continua cheio");
    }

    #[test]
    fn degenerate_rect_draws_nothing() {
        let mut img = canvas();
        fill_rect(&mut img, (30.0, 30.0), (30.0, 40.0), 0.0, red());
        assert_eq!(img.pixel(30, 35), [0, 0, 0, 255]);
    }

    #[test]
    fn ellipse_fills_center_and_spares_corners() {
        let mut img = canvas();
        fill_ellipse(&mut img, (32.0, 32.0), 20.0, 12.0, red());
        assert_eq!(img.pixel(32, 32)[0], 255, "centro");
        assert!(img.pixel(50, 32)[0] > 200, "dentro no eixo maior");
        assert_eq!(img.pixel(32, 4), [0, 0, 0, 255], "fora no eixo menor");
    }

    #[test]
    fn translucent_fill_composites_over_the_background() {
        let mut img = RgbaImage::filled(16, 16, [0, 0, 0, 255]);
        fill_rect(&mut img, (0.0, 0.0), (16.0, 16.0), 0.0, [255, 0, 0, 128]);
        let r = img.pixel(8, 8)[0];
        assert!(r > 100 && r < 160, "meio-termo entre fundo e cor, veio {r}");
    }

    #[test]
    fn clipping_at_image_borders_does_not_panic() {
        let mut img = canvas();
        fill_rect(&mut img, (-10.0, -10.0), (80.0, 80.0), 6.0, red());
        fill_ellipse(&mut img, (0.0, 0.0), 50.0, 50.0, red());
        fill_triangle(&mut img, (-5.0, 70.0), (70.0, -5.0), (70.0, 70.0), red());
    }
}
