//! Amostragem e conversão de cores para o conta-gotas.
//!
//! Um clique devolve o pixel exato, que é o esperado. Mas sobre texto isso
//! quase sempre pega o fundo — a letra ocupa menos área que o espaço em volta
//! dela —, e sobre uma área com ruído ou gradiente o pixel exato não
//! representa o que se está olhando. Daí as duas amostragens extras.

use crate::imgbuf::RgbaImage;

/// Lado do quadrado examinado ao procurar a cor do texto.
const VIZINHANCA_TEXTO: i64 = 20;

/// Cor média de um retângulo da imagem, em RGB.
///
/// A média é aritmética por canal, no espaço sRGB — não no linear. É o que
/// bate com a expectativa de quem está olhando a tela: a cor "no meio" das
/// que aparecem, não a média física de luz.
pub fn average(image: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> [u8; 4] {
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 {
        return [0, 0, 0, 255];
    }
    let x0 = x0.min(w - 1);
    let y0 = y0.min(h - 1);
    let x1 = x1.min(w - 1);
    let y1 = y1.min(h - 1);
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));

    let mut soma = [0u64; 3];
    let mut n = 0u64;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let px = image.pixel(x, y);
            for c in 0..3 {
                soma[c] += px[c] as u64;
            }
            n += 1;
        }
    }
    if n == 0 {
        return [0, 0, 0, 255];
    }
    [
        (soma[0] / n) as u8,
        (soma[1] / n) as u8,
        (soma[2] / n) as u8,
        255,
    ]
}

/// O tom mais escuro num quadrado em volta do ponto — num texto, a cor da
/// letra, e não a do fundo em que ela está.
pub fn darkest_around(image: &RgbaImage, cx: u32, cy: u32) -> [u8; 4] {
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 {
        return [0, 0, 0, 255];
    }
    let metade = VIZINHANCA_TEXTO / 2;
    let x0 = (cx as i64 - metade).max(0) as u32;
    let y0 = (cy as i64 - metade).max(0) as u32;
    let x1 = ((cx as i64 + metade) as u32).min(w - 1);
    let y1 = ((cy as i64 + metade) as u32).min(h - 1);

    let mut melhor = image.pixel(cx.min(w - 1), cy.min(h - 1));
    let mut menor = luminancia(melhor);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let px = image.pixel(x, y);
            let l = luminancia(px);
            if l < menor {
                menor = l;
                melhor = px;
            }
        }
    }
    [melhor[0], melhor[1], melhor[2], 255]
}

/// Luminância relativa (WCAG), em 0–1.
fn luminancia(px: [u8; 4]) -> f32 {
    let f = |c: u8| {
        let v = c as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(px[0]) + 0.7152 * f(px[1]) + 0.0722 * f(px[2])
}

/// sRGB para OKLCH: claridade 0–1, croma e matiz em graus.
///
/// OKLCH é perceptualmente uniforme, ao contrário do HSL: dois tons com a
/// mesma claridade parecem igualmente claros, o que o HSL não garante.
pub fn to_oklch(rgb: [u8; 3]) -> (f32, f32, f32) {
    let linear = |c: u8| {
        let v = c as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(rgb[0]), linear(rgb[1]), linear(rgb[2]));

    let l = (0.412_221_47 * r + 0.536_332_54 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_5 * b).cbrt();

    let ll = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;

    let croma = (a * a + bb * bb).sqrt();
    let matiz = bb.atan2(a).to_degrees().rem_euclid(360.0);
    (ll, croma, matiz)
}

/// Contraste APCA (Lc) entre um texto e o fundo em que ele está.
///
/// Diferente do WCAG 2, é assimétrico: texto escuro sobre claro e texto claro
/// sobre escuro não dão o mesmo número, porque de fato não se leem igual. O
/// sinal indica a polaridade; o que importa é o módulo — em geral pede-se
/// |Lc| ≥ 60 para texto de corpo.
pub fn apca_contrast(texto: [u8; 3], fundo: [u8; 3]) -> f32 {
    const EXP: f32 = 2.4;
    const CLAMP: f32 = 0.022;
    const TRC: f32 = 1.414;

    let y = |c: [u8; 3]| {
        let f = |v: u8| (v as f32 / 255.0).powf(EXP);
        0.2126729 * f(c[0]) + 0.7151522 * f(c[1]) + 0.0721750 * f(c[2])
    };
    // Tons quase pretos são elevados: abaixo do limiar o olho deixa de
    // distinguir, e sem isto o número dispararia.
    let soft = |v: f32| {
        if v < CLAMP {
            v + (CLAMP - v).powf(TRC)
        } else {
            v
        }
    };
    let (yt, yf) = (soft(y(texto)), soft(y(fundo)));

    let lc = if yf > yt {
        // Texto escuro sobre fundo claro.
        (yf.powf(0.56) - yt.powf(0.57)) * 1.14
    } else {
        (yf.powf(0.65) - yt.powf(0.62)) * 1.14
    };
    // Abaixo deste piso o resultado é ruído, e o padrão manda devolver zero.
    if lc.abs() < 0.1 {
        0.0
    } else if lc > 0.0 {
        (lc - 0.027) * 100.0
    } else {
        (lc + 0.027) * 100.0
    }
}

pub fn format_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

pub fn format_oklch(rgb: [u8; 3]) -> String {
    let (l, c, h) = to_oklch(rgb);
    format!("oklch({:.1}% {:.3} {:.1})", l * 100.0, c, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img_2x2(pixels: [[u8; 4]; 4]) -> RgbaImage {
        let mut px = Vec::new();
        for p in pixels {
            px.extend_from_slice(&p);
        }
        RgbaImage::from_raw(2, 2, px)
    }

    #[test]
    fn a_media_de_uma_area_uniforme_e_a_propria_cor() {
        let img = img_2x2([[10, 20, 30, 255]; 4]);
        assert_eq!(average(&img, 0, 0, 1, 1), [10, 20, 30, 255]);
    }

    #[test]
    fn a_media_fica_entre_os_extremos() {
        let img = img_2x2([
            [0, 0, 0, 255],
            [100, 100, 100, 255],
            [0, 0, 0, 255],
            [100, 100, 100, 255],
        ]);
        assert_eq!(average(&img, 0, 0, 1, 1), [50, 50, 50, 255]);
    }

    #[test]
    fn a_media_aceita_o_retangulo_invertido() {
        // Arrastar da direita para a esquerda dá o mesmo retângulo.
        let img = img_2x2([[10, 20, 30, 255]; 4]);
        assert_eq!(average(&img, 1, 1, 0, 0), average(&img, 0, 0, 1, 1));
    }

    #[test]
    fn a_media_recorta_coordenadas_fora_da_imagem() {
        let img = img_2x2([[10, 20, 30, 255]; 4]);
        assert_eq!(average(&img, 0, 0, 99, 99), [10, 20, 30, 255]);
    }

    #[test]
    fn a_cor_do_texto_e_o_tom_mais_escuro_por_perto() {
        // Fundo claro com uma "letra" escura ao lado do ponto amostrado: o
        // clique cai no fundo, e mesmo assim sai a cor da letra.
        let mut px = Vec::new();
        for y in 0..8u32 {
            for x in 0..8u32 {
                let c = if x == 3 && y == 3 {
                    [20, 20, 24, 255]
                } else {
                    [250, 250, 250, 255]
                };
                px.extend_from_slice(&c);
            }
        }
        let img = RgbaImage::from_raw(8, 8, px);
        assert_eq!(darkest_around(&img, 5, 5), [20, 20, 24, 255]);
    }

    #[test]
    fn oklch_do_branco_e_do_preto() {
        let (l, c, _) = to_oklch([255, 255, 255]);
        assert!((l - 1.0).abs() < 0.01, "branco tem claridade 1, veio {l}");
        assert!(c < 0.01, "branco não tem croma, veio {c}");

        let (l, _, _) = to_oklch([0, 0, 0]);
        assert!(l < 0.01, "preto tem claridade 0, veio {l}");
    }

    #[test]
    fn oklch_separa_matizes() {
        let (_, c_vermelho, h_vermelho) = to_oklch([255, 0, 0]);
        let (_, _, h_azul) = to_oklch([0, 0, 255]);
        assert!(c_vermelho > 0.1, "vermelho saturado tem croma");
        assert!(
            (h_vermelho - h_azul).abs() > 30.0,
            "vermelho e azul não podem ter a mesma matiz"
        );
    }

    #[test]
    fn apca_preto_sobre_branco_tem_contraste_alto() {
        let lc = apca_contrast([0, 0, 0], [255, 255, 255]);
        assert!(lc > 100.0, "preto sobre branco deveria passar de 100, veio {lc}");
    }

    #[test]
    fn apca_e_assimetrico() {
        // É o que separa o APCA do WCAG 2: inverter texto e fundo não devolve
        // o mesmo número, porque as duas polaridades não se leem igual.
        let escuro_sobre_claro = apca_contrast([0, 0, 0], [255, 255, 255]).abs();
        let claro_sobre_escuro = apca_contrast([255, 255, 255], [0, 0, 0]).abs();
        assert!((escuro_sobre_claro - claro_sobre_escuro).abs() > 1.0);
    }

    #[test]
    fn apca_de_cores_iguais_e_zero() {
        assert_eq!(apca_contrast([120, 120, 120], [120, 120, 120]), 0.0);
    }

    #[test]
    fn os_formatos_de_texto_saem_legiveis() {
        assert_eq!(format_hex([255, 59, 48]), "#FF3B30");
        let texto = format_oklch([255, 0, 0]);
        assert!(texto.starts_with("oklch("), "veio {texto}");
        assert!(texto.ends_with(')'));
    }
}
