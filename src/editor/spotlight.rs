//! Holofote: escurece a imagem inteira e devolve, ampliado, só o pedaço que
//! interessa.
//!
//! É uma operação sobre pixels, não uma forma vetorial, por dois motivos: o
//! escurecimento é global (a área de fora de *todos* os holofotes) e o miolo
//! mostra o conteúdo reamostrado. Por isso ela roda no mesmo lugar que a
//! redação — e **depois** dela, para que a lupa nunca amplie o que foi
//! censurado.

use crate::imgbuf::RgbaImage;

use super::shapes::Point;

/// Opacidade do véu sobre o que fica de fora.
const DIM_ALPHA: f32 = 154.0 / 255.0;

/// Limites da ampliação.
pub const MAGNIFICATION_MIN: f32 = 1.0;
pub const MAGNIFICATION_MAX: f32 = 4.0;
pub const MAGNIFICATION_DEFAULT: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpotlightForm {
    #[default]
    Ellipse,
    Rect,
    RoundedRect,
}

impl SpotlightForm {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ellipse => "Elipse",
            Self::Rect => "Retângulo",
            Self::RoundedRect => "Arredondado",
        }
    }

    /// Cicla entre as três formas.
    pub fn next(self) -> Self {
        match self {
            Self::Ellipse => Self::Rect,
            Self::Rect => Self::RoundedRect,
            Self::RoundedRect => Self::Ellipse,
        }
    }
}

/// Um holofote pronto para ser aplicado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spotlight {
    pub center: Point,
    pub rx: f32,
    pub ry: f32,
    pub form: SpotlightForm,
    pub magnification: f32,
    /// Espessura do anel; zero deixa o holofote sem borda.
    pub border: f32,
    pub border_color: [u8; 4],
}

impl Spotlight {
    /// O ponto está dentro da lente?
    fn contains(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.center.x, y - self.center.y);
        if self.rx <= f32::EPSILON || self.ry <= f32::EPSILON {
            return false;
        }
        match self.form {
            SpotlightForm::Ellipse => {
                let (nx, ny) = (dx / self.rx, dy / self.ry);
                nx * nx + ny * ny <= 1.0
            }
            SpotlightForm::Rect => dx.abs() <= self.rx && dy.abs() <= self.ry,
            SpotlightForm::RoundedRect => {
                let radius = (self.rx.min(self.ry) * 0.12).clamp(3.0, 28.0);
                let (ax, ay) = (dx.abs(), dy.abs());
                if ax > self.rx || ay > self.ry {
                    return false;
                }
                let (ox, oy) = (ax - (self.rx - radius), ay - (self.ry - radius));
                if ox <= 0.0 || oy <= 0.0 {
                    return true;
                }
                ox * ox + oy * oy <= radius * radius
            }
        }
    }

    /// Distância normalizada até a borda, usada para desenhar o anel: valores
    /// perto de 1 estão junto do contorno.
    fn edge_distance(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = (x - self.center.x, y - self.center.y);
        match self.form {
            SpotlightForm::Ellipse => {
                let (nx, ny) = (dx / self.rx.max(0.001), dy / self.ry.max(0.001));
                (nx * nx + ny * ny).sqrt()
            }
            _ => (dx.abs() / self.rx.max(0.001)).max(dy.abs() / self.ry.max(0.001)),
        }
    }
}

/// Amostra bilinear — a lupa amplia, e vizinho-mais-próximo deixaria degraus.
fn sample(src: &RgbaImage, x: f32, y: f32) -> [u8; 4] {
    let (w, h) = (src.width(), src.height());
    if w == 0 || h == 0 {
        return [0, 0, 0, 255];
    }
    let fx = x.clamp(0.0, (w - 1) as f32);
    let fy = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);

    let (p00, p10, p01, p11) =
        (src.pixel(x0, y0), src.pixel(x1, y0), src.pixel(x0, y1), src.pixel(x1, y1));
    let mut out = [255u8; 4];
    for c in 0..4 {
        let top = p00[c] as f32 * (1.0 - tx) + p10[c] as f32 * tx;
        let bottom = p01[c] as f32 * (1.0 - tx) + p11[c] as f32 * tx;
        out[c] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Aplica todos os holofotes de uma vez.
///
/// Precisa ser de uma vez porque o véu cobre o que está fora de **todos**
/// eles: aplicar um de cada vez escureceria duas vezes a área entre dois
/// holofotes.
pub fn apply(img: &mut RgbaImage, lights: &[Spotlight]) {
    if lights.is_empty() {
        return;
    }
    let source = img.clone();

    for y in 0..img.height() {
        for x in 0..img.width() {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let Some(light) = lights.iter().find(|l| l.contains(fx, fy)) else {
                // Fora de todos: escurece.
                let px = img.pixel_mut(x, y);
                for channel in px.iter_mut().take(3) {
                    *channel = (*channel as f32 * (1.0 - DIM_ALPHA)).round() as u8;
                }
                continue;
            };

            // Dentro: o conteúdo em volta do centro, ampliado.
            let magnification = light.magnification.clamp(MAGNIFICATION_MIN, MAGNIFICATION_MAX);
            let sx = light.center.x + (fx - light.center.x) / magnification;
            let sy = light.center.y + (fy - light.center.y) / magnification;
            let mut color = sample(&source, sx - 0.5, sy - 0.5);

            // Anel na borda, se a espessura pedir.
            if light.border > 0.0 {
                let reach = light.border / light.rx.min(light.ry).max(0.001);
                if light.edge_distance(fx, fy) >= 1.0 - reach {
                    color = light.border_color;
                }
            }
            img.pixel_mut(x, y).copy_from_slice(&color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkers() -> RgbaImage {
        let mut img = RgbaImage::filled(64, 64, [0, 0, 0, 255]);
        for y in 0..64 {
            for x in 0..64 {
                if (x / 4 + y / 4) % 2 == 0 {
                    img.pixel_mut(x, y).copy_from_slice(&[200, 210, 220, 255]);
                }
            }
        }
        img
    }

    fn light(form: SpotlightForm) -> Spotlight {
        Spotlight {
            center: Point::new(32.0, 32.0),
            rx: 12.0,
            ry: 12.0,
            form,
            magnification: 2.0,
            border: 0.0,
            border_color: [255, 255, 255, 255],
        }
    }

    #[test]
    fn no_spotlight_is_a_noop() {
        let mut img = checkers();
        let before = img.clone();
        apply(&mut img, &[]);
        assert_eq!(img.as_raw(), before.as_raw());
    }

    /// Brilho médio de uma faixa horizontal.
    fn brightness(img: &RgbaImage, y: u32, xs: std::ops::Range<u32>) -> f32 {
        let n = xs.len() as f32;
        xs.map(|x| img.pixel(x, y)[0] as f32).sum::<f32>() / n
    }

    /// Quantas vezes a cor muda ao longo de uma faixa — a "frequência" do
    /// padrão. Ampliar reduz esse número.
    fn transitions(img: &RgbaImage, y: u32, xs: std::ops::Range<u32>) -> usize {
        let mut count = 0;
        let mut previous: Option<u8> = None;
        for x in xs {
            let bright = img.pixel(x, y)[0] > 100;
            if previous.is_some_and(|p| (p > 100) != bright) {
                count += 1;
            }
            previous = Some(img.pixel(x, y)[0]);
        }
        count
    }

    #[test]
    fn outside_gets_darker_and_inside_stays_bright() {
        let original = checkers();
        let mut img = original.clone();
        apply(&mut img, &[light(SpotlightForm::Ellipse)]);

        let outside_before = brightness(&original, 2, 0..64);
        let outside_after = brightness(&img, 2, 0..64);
        assert!(outside_after < outside_before * 0.6, "o que ficou de fora escureceu");

        // Dentro da lente o conteúdo continua com o brilho de origem.
        let inside = brightness(&img, 32, 26..38);
        assert!(inside > outside_after * 1.5, "o miolo não levou véu");
    }

    #[test]
    fn the_lens_magnifies_what_is_around_the_centre() {
        // Ampliar espalha o padrão: dentro da lente ele muda de cor menos
        // vezes do que na imagem de origem, na mesma faixa.
        let original = checkers();
        let mut img = original.clone();
        apply(&mut img, &[light(SpotlightForm::Ellipse)]);

        let before = transitions(&original, 32, 24..40);
        let after = transitions(&img, 32, 24..40);
        assert!(after < before, "padrão mais largo dentro da lupa ({before} → {after})");
    }

    #[test]
    fn a_rect_spotlight_keeps_its_corners() {
        // O canto do retângulo fica dentro; o da elipse, fora.
        let original = checkers();
        let mut as_rect = original.clone();
        let mut as_ellipse = original.clone();
        apply(&mut as_rect, &[light(SpotlightForm::Rect)]);
        apply(&mut as_ellipse, &[light(SpotlightForm::Ellipse)]);

        let corner = (42, 42); // dentro do quadrado, fora do círculo
        let rect_px = as_rect.pixel(corner.0, corner.1);
        let ellipse_px = as_ellipse.pixel(corner.0, corner.1);
        assert_ne!(rect_px, ellipse_px, "as formas recortam diferente");
    }

    #[test]
    fn two_spotlights_do_not_dim_the_gap_twice() {
        // O véu é o complemento da união: a área entre dois holofotes só pode
        // escurecer uma vez.
        let original = checkers();
        let mut one = original.clone();
        let mut two = original.clone();
        let a = Spotlight { center: Point::new(16.0, 32.0), ..light(SpotlightForm::Ellipse) };
        let b = Spotlight { center: Point::new(48.0, 32.0), ..light(SpotlightForm::Ellipse) };
        apply(&mut one, &[a]);
        apply(&mut two, &[a, b]);
        // O ponto no meio está fora dos dois nos dois casos.
        assert_eq!(one.pixel(32, 2), two.pixel(32, 2));
    }

    #[test]
    fn the_border_paints_a_ring() {
        let mut img = checkers();
        let ringed = Spotlight { border: 3.0, ..light(SpotlightForm::Ellipse) };
        apply(&mut img, &[ringed]);
        // Junto da borda direita da lente (centro 32, raio 12).
        assert_eq!(img.pixel(43, 32), [255, 255, 255, 255], "anel desenhado");
        assert_ne!(img.pixel(32, 32), [255, 255, 255, 255], "o miolo não é anel");
    }

    #[test]
    fn a_degenerate_spotlight_does_not_panic() {
        let mut img = checkers();
        let flat = Spotlight { rx: 0.0, ry: 0.0, ..light(SpotlightForm::RoundedRect) };
        apply(&mut img, &[flat]);
    }
}
