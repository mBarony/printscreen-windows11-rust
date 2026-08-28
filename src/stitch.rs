//! Costura de quadros de uma página que rola, numa imagem só.
//!
//! É a parte que decide se a captura com rolagem funciona. Não há API que
//! role uma janela alheia e diga quanto rolou: o deslocamento tem de ser
//! **descoberto nos pixels**, comparando um quadro com o seguinte.
//!
//! O método é correlação de uma faixa. Uma faixa do **meio** do quadro
//! anterior é procurada no quadro novo; onde ela casar melhor, é quanto a
//! página andou. Do meio, e não do topo nem do rodapé, porque cabeçalho e
//! barra de status ficam parados quando o resto rola — uma faixa tirada de
//! lá casaria em deslocamento zero e a costura pararia no primeiro quadro.

use crate::imgbuf::RgbaImage;

/// Altura da faixa de referência, em px. Curta demais casa em qualquer lugar
/// de um texto corrido; longa demais não cabe numa janela baixa.
const FAIXA: u32 = 48;

/// Diferença média por canal aceita como "casou". Rolagem suave produz
/// quadros intermediários borrados, e é este piso que os recusa em vez de
/// costurar um borrão.
const MAX_DIFERENCA: u32 = 14;

/// Diferença média abaixo da qual dois quadros são o mesmo — a página parou
/// de rolar, ou chegou ao fim.
const MESMO_QUADRO: u32 = 3;

/// Quanto a página andou entre `anterior` e `novo`, em px.
///
/// `None` quando não deu para saber: quadros de tamanhos diferentes, janela
/// baixa demais para a faixa, ou nenhum deslocamento com casamento bom o
/// bastante — que é o caso do quadro borrado no meio de uma rolagem suave.
/// `Some(0)` é a resposta honesta para "não rolou": a página acabou.
pub fn scroll_offset(anterior: &RgbaImage, novo: &RgbaImage) -> Option<u32> {
    let (w, h) = (anterior.width(), anterior.height());
    if w != novo.width() || h != novo.height() || w == 0 {
        return None;
    }
    // A faixa sai do meio e precisa de espaço acima dela para procurar.
    let meio = h / 2;
    if meio < FAIXA + 1 || h < FAIXA * 2 {
        return None;
    }

    // Quadros idênticos: a página não rolou.
    if diferenca(anterior, novo, meio, meio, FAIXA) <= MESMO_QUADRO {
        return Some(0);
    }

    let mut melhor: Option<(u32, u32)> = None;
    for d in 1..=(meio - 1) {
        let score = diferenca(anterior, novo, meio, meio - d, FAIXA);
        if melhor.is_none_or(|(_, s)| score < s) {
            melhor = Some((d, score));
        }
        // Casamento perfeito não melhora: para de procurar.
        if score == 0 {
            break;
        }
    }
    match melhor {
        Some((d, score)) if score <= MAX_DIFERENCA => Some(d),
        // Achou o melhor de vários ruins: é quadro borrado, não deslocamento.
        _ => None,
    }
}

/// Diferença média por canal entre a faixa de `a` a partir de `ya` e a de `b`
/// a partir de `yb`, ambas com `altura` linhas.
fn diferenca(a: &RgbaImage, b: &RgbaImage, ya: u32, yb: u32, altura: u32) -> u32 {
    let w = a.width();
    // Amostra de colunas, e não todas: a resposta é a mesma e o custo cai
    // por um fator de oito num quadro 4K.
    let passo = (w / 240).max(1);
    let mut soma: u64 = 0;
    let mut contados: u64 = 0;
    for linha in 0..altura {
        let (ry, sy) = (ya + linha, yb + linha);
        if ry >= a.height() || sy >= b.height() {
            break;
        }
        let mut x = 0;
        while x < w {
            let pa = a.pixel(x, ry);
            let pb = b.pixel(x, sy);
            for c in 0..3 {
                soma += pa[c].abs_diff(pb[c]) as u64;
            }
            contados += 3;
            x += passo;
        }
    }
    if contados == 0 {
        return u32::MAX;
    }
    (soma / contados) as u32
}

/// Junta quadros sucessivos de uma página que rola.
pub struct Stitcher {
    canvas: RgbaImage,
    anterior: RgbaImage,
}

impl Stitcher {
    pub fn new(primeiro: RgbaImage) -> Self {
        Self { canvas: primeiro.clone(), anterior: primeiro }
    }

    /// Emenda o próximo quadro e devolve quantos px novos entraram.
    ///
    /// Zero quer dizer que a página não andou — fim do conteúdo, ou rolagem
    /// que não pegou. Quem chama decide se tenta de novo ou encerra.
    pub fn push(&mut self, frame: &RgbaImage) -> u32 {
        let Some(d) = scroll_offset(&self.anterior, frame) else {
            return 0;
        };
        if d == 0 {
            return 0;
        }
        let (w, h) = (frame.width(), frame.height());
        let altura = self.canvas.height();
        let mut maior = RgbaImage::filled(w, altura + d, [0, 0, 0, 255]);
        maior.paste(&self.canvas, 0, 0);
        // Só as `d` linhas de baixo são novas: o resto já está na costura.
        let novas = frame.crop(0, h - d, w, d);
        maior.paste(&novas, 0, altura as i64);
        self.canvas = maior;
        self.anterior = frame.clone();
        d
    }

    pub fn finish(self) -> RgbaImage {
        self.canvas
    }

    pub fn height(&self) -> u32 {
        self.canvas.height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma "página" alta com conteúdo que varia linha a linha, para nenhuma
    /// faixa casar em dois lugares.
    fn pagina(w: u32, h: u32) -> RgbaImage {
        let mut img = RgbaImage::filled(w, h, [0, 0, 0, 255]);
        for y in 0..h {
            for x in 0..w {
                let v = ((y * 7 + x * 3) % 251) as u8;
                let u = ((y * 13) % 199) as u8;
                img.pixel_mut(x, y).copy_from_slice(&[v, u, v ^ u, 255]);
            }
        }
        img
    }

    /// A "janela" que mostra a página a partir de `topo`.
    fn quadro(pagina: &RgbaImage, topo: u32, h: u32) -> RgbaImage {
        pagina.crop(0, topo, pagina.width(), h)
    }

    #[test]
    fn acha_o_quanto_a_pagina_rolou() {
        let pg = pagina(200, 900);
        let a = quadro(&pg, 0, 300);
        for esperado in [1u32, 17, 60, 140] {
            let b = quadro(&pg, esperado, 300);
            assert_eq!(scroll_offset(&a, &b), Some(esperado), "rolagem de {esperado}");
        }
    }

    #[test]
    fn a_pagina_parada_devolve_zero() {
        let pg = pagina(200, 900);
        let a = quadro(&pg, 40, 300);
        assert_eq!(scroll_offset(&a, &a.clone()), Some(0));
    }

    #[test]
    fn um_cabecalho_fixo_nao_engana_a_medida() {
        // O cabeçalho fica parado enquanto o resto rola. Uma faixa tirada do
        // topo casaria em zero e a costura morreria no primeiro quadro.
        let pg = pagina(200, 900);
        let cabecalho = |img: &mut RgbaImage| {
            for y in 0..40u32 {
                for x in 0..200u32 {
                    img.pixel_mut(x, y).copy_from_slice(&[10, 20, 30, 255]);
                }
            }
        };
        let mut a = quadro(&pg, 0, 300);
        let mut b = quadro(&pg, 55, 300);
        cabecalho(&mut a);
        cabecalho(&mut b);
        assert_eq!(scroll_offset(&a, &b), Some(55));
    }

    #[test]
    fn um_quadro_borrado_e_recusado() {
        // Rolagem suave produz quadros intermediários que não casam em
        // deslocamento nenhum; costurá-los emendaria um borrão.
        let pg = pagina(200, 900);
        let a = quadro(&pg, 0, 300);
        let mut borrado = quadro(&pg, 60, 300);
        for y in 0..300u32 {
            for x in 0..200u32 {
                let p = borrado.pixel(x, y);
                let media = ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8;
                borrado
                    .pixel_mut(x, y)
                    .copy_from_slice(&[media, media, media, 255]);
            }
        }
        assert_eq!(scroll_offset(&a, &borrado), None);
    }

    #[test]
    fn a_costura_reconstroi_a_pagina() {
        // Três quadros de uma página que rola 100 px por vez devolvem a
        // página inteira até onde foi vista.
        let pg = pagina(120, 800);
        let mut s = Stitcher::new(quadro(&pg, 0, 300));
        assert_eq!(s.push(&quadro(&pg, 100, 300)), 100);
        assert_eq!(s.push(&quadro(&pg, 200, 300)), 100);
        let out = s.finish();
        assert_eq!((out.width(), out.height()), (120, 500));
        // O conteúdo tem de bater com a página, linha a linha.
        for y in [0u32, 150, 299, 400, 499] {
            for x in [0u32, 60, 119] {
                assert_eq!(out.pixel(x, y), pg.pixel(x, y), "em ({x}, {y})");
            }
        }
    }

    #[test]
    fn a_costura_para_no_fim_da_pagina() {
        let pg = pagina(120, 800);
        let mut s = Stitcher::new(quadro(&pg, 0, 300));
        let antes = s.height();
        // Mesmo quadro de novo: a página não andou.
        assert_eq!(s.push(&quadro(&pg, 0, 300)), 0);
        assert_eq!(s.height(), antes, "nada foi emendado");
    }

    #[test]
    fn quadros_incompativeis_nao_quebram() {
        let pg = pagina(120, 800);
        let a = quadro(&pg, 0, 300);
        assert_eq!(scroll_offset(&a, &pagina(100, 300)), None, "outra largura");
        // Janela baixa demais para a faixa de referência.
        let baixo = pagina(120, 60);
        assert_eq!(scroll_offset(&baixo, &baixo.clone()), None);
    }
}
