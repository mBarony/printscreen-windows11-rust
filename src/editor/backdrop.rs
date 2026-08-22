//! Fundo decorativo: coloca a captura sobre um degradê colorido, com sombra,
//! do jeito que se costuma publicar uma imagem de tela.
//!
//! O degradê é um "mesh": uma cor de base e quatro manchas radiais espalhadas
//! pelos cantos, cada uma somada por cima da anterior. Não há blur em lugar
//! nenhum — a sombra é uma pilha de retângulos arredondados concêntricos com
//! opacidade crescente, que sai mais barato e dá o mesmo efeito nessa escala.
//!
//! Diferente da redação e do holofote, isto não altera a captura: é uma
//! moldura montada em volta dela, e por isso só entra na exportação. As
//! anotações continuam em coordenadas da imagem, sem deslocamento.

use crate::imgbuf::RgbaImage;

use super::raster;

/// Folga entre a borda da captura e a do fundo, em px.
pub const MARGIN: f32 = 64.0;
/// Raio dos cantos da captura sobre o fundo.
const IMAGE_RADIUS: f32 = 14.0;
/// Opacidade do centro de cada mancha do degradê.
const BLOB_ALPHA: f32 = 220.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackdropStyle {
    #[default]
    None,
    Aurora,
    Sunset,
    Lagoon,
    Violet,
}

impl BackdropStyle {
    /// Ordem do ciclo na toolbar.
    pub const ALL: [BackdropStyle; 5] = [
        BackdropStyle::None,
        BackdropStyle::Aurora,
        BackdropStyle::Sunset,
        BackdropStyle::Lagoon,
        BackdropStyle::Violet,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Sem fundo",
            Self::Aurora => "Aurora",
            Self::Sunset => "Poente",
            Self::Lagoon => "Lagoa",
            Self::Violet => "Violeta",
        }
    }

    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// Cor de base e as quatro manchas, em coordenadas relativas ao retângulo
    /// (0..1 em cada eixo; o raio é uma fração da largura).
    fn palette(self) -> Option<([u8; 3], [Blob; 4])> {
        let p = match self {
            Self::None => return None,
            Self::Aurora => (
                [0x10, 0x18, 0x27],
                [
                    Blob::new(0.15, 0.18, 0.75, [0x2d, 0xd4, 0xbf]),
                    Blob::new(1.0, 0.0, 0.78, [0x7c, 0x3a, 0xed]),
                    Blob::new(0.5, 1.0, 0.72, [0x25, 0x63, 0xeb]),
                    Blob::new(0.0, 1.0, 0.55, [0x0f, 0x76, 0x6e]),
                ],
            ),
            Self::Sunset => (
                [0x25, 0x13, 0x28],
                [
                    Blob::new(0.0, 0.0, 0.82, [0xf9, 0x73, 0x16]),
                    Blob::new(1.0, 0.2, 0.70, [0xec, 0x48, 0x99]),
                    Blob::new(0.5, 1.0, 0.75, [0x7c, 0x3a, 0xed]),
                    Blob::new(0.0, 1.0, 0.50, [0xef, 0x44, 0x44]),
                ],
            ),
            Self::Lagoon => (
                [0x07, 0x1c, 0x2a],
                [
                    Blob::new(0.12, 0.0, 0.68, [0x06, 0xb6, 0xd4]),
                    Blob::new(1.0, 0.25, 0.75, [0x1d, 0x4e, 0xd8]),
                    Blob::new(0.5, 1.0, 0.75, [0x0f, 0x76, 0x6e]),
                    Blob::new(0.0, 1.0, 0.50, [0x22, 0xd3, 0xee]),
                ],
            ),
            Self::Violet => (
                [0x17, 0x12, 0x25],
                [
                    Blob::new(0.0, 0.15, 0.70, [0xa8, 0x55, 0xf7]),
                    Blob::new(1.0, 0.0, 0.72, [0x4f, 0x46, 0xe5]),
                    Blob::new(1.0, 1.0, 0.65, [0xdb, 0x27, 0x77]),
                    Blob::new(0.25, 1.0, 0.62, [0x43, 0x38, 0xca]),
                ],
            ),
        };
        Some(p)
    }
}

#[derive(Debug, Clone, Copy)]
struct Blob {
    /// Centro, em fração da largura e da altura.
    cx: f32,
    cy: f32,
    /// Raio, em fração da largura.
    radius: f32,
    color: [u8; 3],
}

impl Blob {
    const fn new(cx: f32, cy: f32, radius: f32, color: [u8; 3]) -> Self {
        Blob { cx, cy, radius, color }
    }
}

/// Monta a moldura em volta de `content` e devolve a imagem final.
/// Com [`BackdropStyle::None`] devolve o conteúdo intacto.
pub fn compose(content: &RgbaImage, style: BackdropStyle) -> RgbaImage {
    let Some((base, blobs)) = style.palette() else {
        return content.clone();
    };
    let margin = MARGIN.round() as u32;
    let (w, h) = (content.width() + margin * 2, content.height() + margin * 2);
    let mut canvas = RgbaImage::filled(w, h, [base[0], base[1], base[2], 255]);

    paint_blobs(&mut canvas, &blobs);
    let image_rect = (
        (margin as f32, margin as f32),
        ((margin + content.width()) as f32, (margin + content.height()) as f32),
    );
    paint_shadow(&mut canvas, image_rect);
    paste_rounded(&mut canvas, content, margin, IMAGE_RADIUS);
    canvas
}

/// Soma as manchas radiais sobre a base: opacidade máxima no centro, zero na
/// borda, interpolada linearmente — é o que dá a transição macia do "mesh".
fn paint_blobs(canvas: &mut RgbaImage, blobs: &[Blob; 4]) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    for blob in blobs {
        let center = (blob.cx * w, blob.cy * h);
        let radius = (blob.radius * w).max(1.0);
        let bbox = (
            center.0 - radius,
            center.1 - radius,
            center.0 + radius,
            center.1 + radius,
        );
        let x0 = bbox.0.floor().max(0.0) as u32;
        let y0 = bbox.1.floor().max(0.0) as u32;
        let x1 = (bbox.2.ceil().max(0.0) as u32).min(canvas.width());
        let y1 = (bbox.3.ceil().max(0.0) as u32).min(canvas.height());

        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f32 + 0.5 - center.0, y as f32 + 0.5 - center.1);
                let distance = (dx * dx + dy * dy).sqrt() / radius;
                if distance >= 1.0 {
                    continue;
                }
                let alpha = BLOB_ALPHA * (1.0 - distance) / 255.0;
                let px = canvas.pixel_mut(x, y);
                for (channel, src) in px.iter_mut().zip(blob.color).take(3) {
                    *channel =
                        (src as f32 * alpha + *channel as f32 * (1.0 - alpha)).round() as u8;
                }
            }
        }
    }
}

/// Sombra sob a captura: duas pilhas de retângulos arredondados concêntricos,
/// uma larga e difusa e outra curta e densa, ambas deslocadas para baixo —
/// a luz vem de cima.
fn paint_shadow(canvas: &mut RgbaImage, rect: ((f32, f32), (f32, f32))) {
    let ((x0, y0), (x1, y1)) = rect;
    for (layers, spread_step, alpha_base, alpha_step, offset, radius) in
        [(24u32, 0.85_f32, 2.0_f32, 0.2_f32, 14.0_f32, 16.0_f32), (12, 0.45, 3.0, 1.0, 8.0, 14.0)]
    {
        for layer in (1..=layers).rev() {
            let spread = layer as f32 * spread_step;
            let alpha = alpha_base + (layers - layer) as f32 * alpha_step;
            raster::fill_rect(
                canvas,
                (x0 - spread, y0 - spread + offset),
                (x1 + spread, y1 + spread + offset),
                radius + spread,
                [0, 0, 0, alpha.round().clamp(0.0, 255.0) as u8],
            );
        }
    }
}

/// Cola a captura com os cantos arredondados, deixando o fundo aparecer
/// neles — um retângulo de cantos vivos sobre o degradê denunciaria a
/// colagem.
fn paste_rounded(canvas: &mut RgbaImage, content: &RgbaImage, offset: u32, radius: f32) {
    let (w, h) = (content.width(), content.height());
    let r = radius.min(w as f32 / 2.0).min(h as f32 / 2.0).max(0.0);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Projeta no centro do arco mais próximo: na faixa central a
            // distância é zero, e só nos cantos o teste vira o do círculo.
            let cx = fx.clamp(r, (w as f32 - r).max(r));
            let cy = fy.clamp(r, (h as f32 - r).max(r));
            let (dx, dy) = (fx - cx, fy - cy);
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let px = content.pixel(x, y);
            canvas.pixel_mut(x + offset, y + offset).copy_from_slice(&px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> RgbaImage {
        RgbaImage::filled(40, 30, [255, 255, 255, 255])
    }

    #[test]
    fn no_backdrop_returns_the_content_untouched() {
        let img = content();
        let out = compose(&img, BackdropStyle::None);
        assert_eq!(out.as_raw(), img.as_raw());
    }

    #[test]
    fn a_backdrop_adds_a_margin_on_every_side() {
        let out = compose(&content(), BackdropStyle::Aurora);
        let margin = MARGIN as u32;
        assert_eq!((out.width(), out.height()), (40 + margin * 2, 30 + margin * 2));
    }

    #[test]
    fn the_capture_lands_in_the_middle_intact() {
        let out = compose(&content(), BackdropStyle::Lagoon);
        let margin = MARGIN as u32;
        assert_eq!(out.pixel(margin + 20, margin + 15), [255, 255, 255, 255]);
        assert_ne!(out.pixel(2, 2), [255, 255, 255, 255], "a moldura não é branca");
    }

    #[test]
    fn every_preset_paints_something_different() {
        let mut seen = Vec::new();
        for style in BackdropStyle::ALL.iter().filter(|s| **s != BackdropStyle::None) {
            let out = compose(&content(), *style);
            let corner = out.pixel(4, 4);
            assert!(!seen.contains(&corner), "{} repetiu uma cor", style.label());
            seen.push(corner);
        }
    }

    #[test]
    fn the_cycle_visits_all_the_presets_and_returns() {
        let mut style = BackdropStyle::None;
        for _ in 0..BackdropStyle::ALL.len() {
            style = style.next();
        }
        assert_eq!(style, BackdropStyle::None, "o ciclo fecha");
    }

    #[test]
    fn the_shadow_darkens_what_is_just_below_the_capture() {
        let out = compose(&content(), BackdropStyle::Violet);
        let margin = MARGIN as u32;
        let centre_x = margin + 20;
        // Logo abaixo da captura há sombra; bem longe dela, não.
        let near = out.pixel(centre_x, margin + 30 + 4);
        let far = out.pixel(centre_x, margin + 30 + 50);
        let sum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        assert!(sum(near) < sum(far), "a sombra escurece o que está logo abaixo");
    }
}
