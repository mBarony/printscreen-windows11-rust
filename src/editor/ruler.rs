//! Régua: mede uma distância sobre a imagem e mostra o valor no próprio traço.
//!
//! A medida é em **px da imagem**, não em pontos de tela: a 150% de escala os
//! dois números diferem, e quem anota uma captura quer o tamanho do que está
//! nela — o mesmo número que o overlay mostra ao selecionar a região.
//!
//! A geometria mora aqui para o preview e a exportação desenharem a mesma
//! régua; cada um mede o próprio texto, porque a fonte é medida de um jeito
//! no egui e de outro no `ab_glyph`.

use super::shapes::{arrow_geometry_capped, Point};

/// Tamanho da fonte do rótulo, derivado da espessura como no numerador — a
/// roda que engrossa o traço também aumenta o número, sem um segundo
/// controle na barra.
pub fn label_font_size(stroke_width: f32) -> f32 {
    (stroke_width * 3.6).max(12.0)
}

/// Geometria derivada de uma régua: a haste entre as duas pontas, os dois
/// triângulos e onde o rótulo é centrado.
pub struct RulerGeometry {
    /// Haste já recuada até a base de cada ponta, para não vazar por dentro.
    pub shaft: [Point; 2],
    /// Vértices do triângulo de cada extremidade: [ponta, base, base].
    pub head_a: [Point; 3],
    pub head_b: [Point; 3],
    /// Centro do rótulo — o meio da régua.
    pub label_center: Point,
    pub font_size: f32,
    pub label: String,
}

pub fn geometry(a: Point, b: Point, stroke_width: f32) -> RulerGeometry {
    // Duas setas opostas sobre a mesma reta: a régua aponta para os dois
    // lados, que é o que a distingue de uma seta e diz "daqui até aqui".
    // Cada ponta cabe em, no máximo, metade do traço — as duas juntas não
    // podem passar do comprimento medido.
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let meio = (dx * dx + dy * dy).sqrt() / 2.0;
    let para_a = arrow_geometry_capped(b, a, stroke_width, meio);
    let para_b = arrow_geometry_capped(a, b, stroke_width, meio);
    RulerGeometry {
        shaft: [para_a.shaft_b, para_b.shaft_b],
        head_a: para_a.head,
        head_b: para_b.head,
        label_center: Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        font_size: label_font_size(stroke_width),
        label: label(a, b),
    }
}

/// Tinta do rótulo: branca ou quase-preta, a que tiver mais contraste com a
/// pílula — que é da cor do traço. Sempre branca, uma régua branca sairia com
/// o número invisível.
pub fn label_ink(pill: [u8; 4]) -> [u8; 4] {
    let fundo = [pill[0], pill[1], pill[2]];
    let claro = crate::color::apca_contrast([0xFF, 0xFF, 0xFF], fundo).abs();
    let escuro = crate::color::apca_contrast([0x12, 0x12, 0x16], fundo).abs();
    if claro >= escuro {
        [0xFF, 0xFF, 0xFF, 0xFF]
    } else {
        [0x12, 0x12, 0x16, 0xFF]
    }
}

/// O valor medido, arredondado ao pixel: uma régua com casas decimais mede
/// a precisão do arrasto, não o que está na imagem.
pub fn label(a: Point, b: Point) -> String {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    format!("{} px", (dx * dx + dy * dy).sqrt().round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mede_em_px_da_imagem() {
        let a = Point::new(10.0, 20.0);
        let b = Point::new(40.0, 60.0);
        assert_eq!(label(a, b), "50 px");
    }

    #[test]
    fn as_duas_pontas_apontam_para_fora() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(100.0, 0.0);
        let geo = geometry(a, b, 3.0);
        assert!((geo.head_a[0].x - a.x).abs() < 0.01, "a ponta A fica em A");
        assert!((geo.head_b[0].x - b.x).abs() < 0.01, "a ponta B fica em B");
        // A haste começa e termina depois das bases, dentro do intervalo.
        assert!(geo.shaft[0].x > a.x && geo.shaft[1].x < b.x);
    }

    #[test]
    fn o_rotulo_fica_no_meio() {
        let geo = geometry(Point::new(0.0, 0.0), Point::new(80.0, 60.0), 3.0);
        assert!((geo.label_center.x - 40.0).abs() < 0.01);
        assert!((geo.label_center.y - 30.0).abs() < 0.01);
    }

    #[test]
    fn regua_curta_nao_cruza_as_pontas() {
        // Com o teto padrão da seta (10 px), as duas pontas de uma régua de
        // 12 px se atravessariam e a haste sairia do avesso.
        let a = Point::new(0.0, 0.0);
        let b = Point::new(12.0, 0.0);
        let geo = geometry(a, b, 3.0);
        assert!(
            geo.shaft[0].x <= geo.shaft[1].x + 0.01,
            "haste invertida: {:?}",
            geo.shaft
        );
    }

    #[test]
    fn o_rotulo_troca_de_tinta_conforme_a_cor() {
        // A pílula é da cor do traço, e a paleta tem cores dos dois lados.
        assert_eq!(label_ink([0x00, 0x7A, 0xFF, 0xFF])[0], 0xFF, "azul: texto claro");
        assert_eq!(label_ink([0xFF, 0xFF, 0xFF, 0xFF])[0], 0x12, "branco: texto escuro");
        assert_eq!(label_ink([0xFF, 0xCC, 0x00, 0xFF])[0], 0x12, "amarelo: texto escuro");
    }

    #[test]
    fn regua_de_comprimento_zero_nao_quebra() {
        let p = Point::new(5.0, 5.0);
        let geo = geometry(p, p, 3.0);
        assert_eq!(geo.label, "0 px");
        assert!(geo.shaft[0].x.is_finite() && geo.shaft[1].y.is_finite());
    }
}
