//! Ocultar (`Tool::Redact`): apaga uma região da imagem de forma irreversível.
//!
//! O ponto central é o modo mosaico. Um pixelate comum — média dos pixels de
//! cada bloco — **preserva informação**: a média é uma função do conteúdo, e
//! sobre texto ela deixa um padrão que ataques de despixelização exploram.
//! Aqui o mosaico é sintético: as amostras servem só para descobrir quais
//! tons dominam a região, suas **posições são descartadas**, e cada bloco
//! recebe uma dessas cores sorteada por um gerador pseudoaleatório semeado.
//! O resultado parece com a região original o bastante para não virar um
//! borrão feio, sem carregar nada do que estava escrito ali.
//!
//! A semente fica guardada na anotação para o mosaico não "ferver" a cada
//! quadro, e é renovada ao duplicar — duas redações sobre o mesmo conteúdo
//! não podem sair idênticas.

use crate::imgbuf::RgbaImage;

use super::shapes::Point;

/// Cor da redação sólida.
const SOLID: [u8; 4] = [0x12, 0x12, 0x16, 0xFF];
/// Lado do bloco do mosaico, em px da imagem.
const BLOCK: u32 = 12;
/// Teto de amostras usadas para descobrir os tons dominantes.
const MAX_SAMPLES: u32 = 4096;
/// Quantos tons dominantes entram no sorteio.
const PALETTE_SIZE: usize = 6;
/// Quantização em 2 bits por canal — 4×4×4 caixas.
const BUCKETS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedactionStyle {
    /// Mosaico sintético (padrão): irreconhecível, mas com a cara da região.
    #[default]
    Pixelate,
    /// Tapa a região com uma cor chapada.
    Solid,
}

impl RedactionStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pixelate => "Mosaico",
            Self::Solid => "Sólida",
        }
    }
}

/// Semente nova para uma redação.
///
/// Duas redações sobre o mesmo conteúdo não podem sair com o mesmo mosaico —
/// isso denunciaria que escondem a mesma coisa. O contador garante que
/// redações criadas no mesmo nanossegundo ainda difiram.
pub fn fresh_seed() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let step = COUNTER.fetch_add(1, Ordering::Relaxed);
    (nanos ^ step.wrapping_mul(0x9E37_79B9)).max(1)
}

/// Gerador xorshift32. Determinístico a partir da semente, que é o que
/// mantém o mosaico estável entre quadros e entre preview e exportação.
fn next_random(state: &mut u32) -> u32 {
    if *state == 0 {
        *state = 0x6d2b_79f5; // uma semente zerada travaria o gerador em zero
    }
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

/// Região da redação em pixels inteiros, recortada à imagem.
/// `None` quando ela cai fora ou degenera.
fn pixel_region(img: &RgbaImage, min: Point, max: Point) -> Option<(u32, u32, u32, u32)> {
    let x0 = min.x.floor().max(0.0) as u32;
    let y0 = min.y.floor().max(0.0) as u32;
    let x1 = (max.x.ceil().max(0.0) as u32).min(img.width());
    let y1 = (max.y.ceil().max(0.0) as u32).min(img.height());
    (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
}

/// Tons dominantes da região, sem qualquer vínculo com onde eles estavam.
///
/// As amostras são quantizadas em 64 caixas de cor; as caixas mais populosas
/// devolvem a média dos seus próprios pixels. É só uma lista de cores: a
/// distribuição espacial que carregaria o conteúdo se perde aqui.
fn dominant_colors(img: &RgbaImage, region: (u32, u32, u32, u32)) -> Vec<[u8; 4]> {
    let (x0, y0, x1, y1) = region;
    let area = (x1 - x0) as u64 * (y1 - y0) as u64;
    let stride = ((area as f64 / MAX_SAMPLES as f64).sqrt().ceil() as u32).max(1);

    let mut sums = [[0u64; 3]; BUCKETS];
    let mut counts = [0u64; BUCKETS];
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            let px = img.pixel(x, y);
            let bucket = (px[0] as usize >> 6) * 16 + (px[1] as usize >> 6) * 4 + (px[2] as usize >> 6);
            for channel in 0..3 {
                sums[bucket][channel] += px[channel] as u64;
            }
            counts[bucket] += 1;
            x += stride;
        }
        y += stride;
    }

    let mut ranked: Vec<usize> = (0..BUCKETS).filter(|&b| counts[b] > 0).collect();
    ranked.sort_by_key(|&b| std::cmp::Reverse(counts[b]));
    ranked
        .into_iter()
        .take(PALETTE_SIZE)
        .map(|b| {
            let n = counts[b];
            [
                (sums[b][0] / n) as u8,
                (sums[b][1] / n) as u8,
                (sums[b][2] / n) as u8,
                255,
            ]
        })
        .collect()
}

/// Queima a região: depois disso não há o que recuperar do conteúdo dela.
pub fn apply(img: &mut RgbaImage, min: Point, max: Point, style: RedactionStyle, seed: u32) {
    let Some(region) = pixel_region(img, min, max) else {
        return;
    };
    let palette = match style {
        RedactionStyle::Solid => vec![SOLID],
        RedactionStyle::Pixelate => {
            let found = dominant_colors(img, region);
            if found.is_empty() {
                vec![SOLID]
            } else {
                found
            }
        }
    };

    let (x0, y0, x1, y1) = region;
    // A sólida é um bloco só; o mosaico é uma grade.
    let block = if style == RedactionStyle::Solid { u32::MAX } else { BLOCK };
    let mut state = seed;

    let mut by = y0;
    while by < y1 {
        let mut bx = x0;
        while bx < x1 {
            let color = palette[next_random(&mut state) as usize % palette.len()];
            let bw = block.min(x1 - bx);
            let bh = block.min(y1 - by);
            for y in by..by + bh {
                for x in bx..bx + bw {
                    img.pixel_mut(x, y).copy_from_slice(&color);
                }
            }
            bx = bx.saturating_add(block).max(bx + 1);
        }
        by = by.saturating_add(block).max(by + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Imagem com uma faixa clara sobre fundo escuro — um proxy de texto.
    fn text_like() -> RgbaImage {
        let mut img = RgbaImage::filled(64, 64, [20, 20, 24, 255]);
        for y in 20..30 {
            for x in 10..50 {
                img.pixel_mut(x, y).copy_from_slice(&[240, 240, 245, 255]);
            }
        }
        img
    }

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn solid_covers_the_region_with_one_flat_colour() {
        let mut img = text_like();
        apply(&mut img, p(8.0, 18.0), p(52.0, 32.0), RedactionStyle::Solid, 1);
        for (x, y) in [(10, 20), (30, 25), (49, 29)] {
            assert_eq!(img.pixel(x, y), SOLID, "coberto em ({x},{y})");
        }
        assert_eq!(img.pixel(2, 2), [20, 20, 24, 255], "fora da região, intacto");
    }

    #[test]
    fn the_mosaic_destroys_the_content() {
        // O conteúdo original não pode sobreviver em lugar nenhum da região.
        let original = text_like();
        let mut img = original.clone();
        apply(&mut img, p(8.0, 18.0), p(52.0, 32.0), RedactionStyle::Pixelate, 7);
        let mut untouched = 0;
        for y in 20..30 {
            for x in 10..50 {
                if img.pixel(x, y) == original.pixel(x, y) {
                    untouched += 1;
                }
            }
        }
        // Uma coincidência ou outra é aceitável (a paleta sai da própria
        // região); o que não pode é a faixa continuar legível.
        assert!(untouched < 40, "{untouched} pixels intocados na área redigida");
    }

    #[test]
    fn the_mosaic_carries_no_trace_of_where_the_ink_was() {
        // A prova de que o mosaico é sintético: mover a faixa clara dentro da
        // região não pode mudar o resultado, porque as posições das amostras
        // são descartadas. Um pixelate por média mudaria.
        let mut top = RgbaImage::filled(48, 48, [20, 20, 24, 255]);
        let mut bottom = top.clone();
        for x in 4..44 {
            for y in 6..14 {
                top.pixel_mut(x, y).copy_from_slice(&[240, 240, 245, 255]);
            }
            for y in 34..42 {
                bottom.pixel_mut(x, y).copy_from_slice(&[240, 240, 245, 255]);
            }
        }
        apply(&mut top, p(0.0, 0.0), p(48.0, 48.0), RedactionStyle::Pixelate, 99);
        apply(&mut bottom, p(0.0, 0.0), p(48.0, 48.0), RedactionStyle::Pixelate, 99);
        assert_eq!(
            top.as_raw(),
            bottom.as_raw(),
            "o mosaico não pode depender de onde o conteúdo estava"
        );
    }

    #[test]
    fn different_seeds_give_different_mosaics() {
        let mut a = text_like();
        let mut b = text_like();
        apply(&mut a, p(8.0, 18.0), p(52.0, 32.0), RedactionStyle::Pixelate, 1);
        apply(&mut b, p(8.0, 18.0), p(52.0, 32.0), RedactionStyle::Pixelate, 2);
        assert_ne!(a.as_raw(), b.as_raw(), "sementes distintas, mosaicos distintos");
    }

    #[test]
    fn the_same_seed_is_stable() {
        // Estabilidade entre quadros e entre preview e exportação.
        let mut a = text_like();
        let mut b = text_like();
        apply(&mut a, p(8.0, 18.0), p(52.0, 32.0), RedactionStyle::Pixelate, 42);
        apply(&mut b, p(8.0, 18.0), p(52.0, 32.0), RedactionStyle::Pixelate, 42);
        assert_eq!(a.as_raw(), b.as_raw());
    }

    #[test]
    fn a_zero_seed_still_produces_a_mosaic() {
        // Zero travaria o xorshift; o gerador precisa se defender disso.
        let mut img = text_like();
        apply(&mut img, p(8.0, 18.0), p(52.0, 32.0), RedactionStyle::Pixelate, 0);
        assert_ne!(img.pixel(30, 25), [240, 240, 245, 255], "a faixa foi coberta");
    }

    #[test]
    fn fresh_seeds_do_not_repeat() {
        let seeds: std::collections::HashSet<u32> = (0..64).map(|_| fresh_seed()).collect();
        assert_eq!(seeds.len(), 64, "cada redação precisa da sua semente");
        assert!(!seeds.contains(&0), "zero travaria o gerador");
    }

    #[test]
    fn a_region_outside_the_image_is_a_noop() {
        let mut img = text_like();
        let before = img.clone();
        apply(&mut img, p(100.0, 100.0), p(200.0, 200.0), RedactionStyle::Solid, 1);
        assert_eq!(img.as_raw(), before.as_raw());
    }

    #[test]
    fn a_region_hanging_off_the_edge_is_clipped() {
        let mut img = text_like();
        apply(&mut img, p(-20.0, -20.0), p(20.0, 20.0), RedactionStyle::Solid, 1);
        assert_eq!(img.pixel(0, 0), SOLID);
        assert_eq!(img.pixel(40, 40), [20, 20, 24, 255], "fora do pedido");
    }
}
