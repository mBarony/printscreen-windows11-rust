//! Seleção inteligente: descobre, pelos pixels, o elemento sob o cursor.
//!
//! A alternativa seria `EnumChildWindows`, que devolve os controles nativos
//! com precisão — e não enxerga nada desenhado por conta própria. Como
//! Electron, Qt e navegadores desenham a interface inteira dentro de uma
//! janela só, esse caminho acerta muito no Bloco de Notas e nada no VS Code.
//! Por imagem funciona em qualquer coisa, ao preço de errar mais.
//!
//! A ideia é simples: um elemento de interface é uma **superfície de cor
//! uniforme** — o fundo de um botão, de um painel, de uma barra. Achar a
//! região conectada dessa cor a partir do cursor e tomar a caixa dela dá o
//! elemento. O texto e os ícones de dentro ficam como buracos na região, e
//! não atrapalham: a caixa os contém.

use crate::imgbuf::RgbaImage;

/// Alcance da busca a partir do cursor, em px da imagem. É o que impede um
/// fundo chapado de tela cheia de devolver o monitor inteiro, e o que mantém
/// o custo por quadro previsível.
pub const REACH: u32 = 420;

/// Diferença máxima por canal para duas cores contarem como a mesma
/// superfície. Interfaces têm degradês sutis e anti-aliasing; zero recusaria
/// o próprio botão.
const TOLERANCIA: i32 = 12;

/// Lado mínimo do que vale como elemento, em px. Abaixo disso é glifo ou
/// ícone, e selecionar a letra sob o cursor não é o que ninguém quer.
const MIN_LADO: u32 = 8;

/// Janela em que a cor dominante é procurada quando o cursor cai em cima de
/// texto. Ímpar e larga o bastante para conter o fundo em volta do glifo.
const AMOSTRA: i32 = 5;

/// A caixa do elemento sob `(cx, cy)`, em px da imagem.
///
/// `None` quando o resultado seria pequeno demais para ser um elemento, ou
/// grande a ponto de encostar no alcance da busca por todos os lados — nesse
/// caso o que está sob o cursor é o fundo, e o fundo não é um alvo.
pub fn element_at(img: &RgbaImage, cx: u32, cy: u32) -> Option<(u32, u32, u32, u32)> {
    let (iw, ih) = (img.width(), img.height());
    if cx >= iw || cy >= ih {
        return None;
    }
    // A cor da superfície é a dominante em volta do cursor, e não a de baixo
    // dele: sobre uma letra, a de baixo é a tinta do glifo e a região
    // conectada seria a própria letra.
    let alvo = cor_dominante(img, cx, cy);

    let x0 = cx.saturating_sub(REACH);
    let y0 = cy.saturating_sub(REACH);
    let x1 = (cx + REACH + 1).min(iw);
    let y1 = (cy + REACH + 1).min(ih);
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);

    // Ponto de partida: o pixel da superfície mais perto do cursor. Sobre uma
    // letra, o cursor não está na superfície e a busca começaria vazia.
    let inicio = vizinho_da_superficie(img, cx, cy, alvo)?;

    // Preenchimento por inundação, com pilha própria: a recursão estouraria
    // em regiões grandes, e este código roda a cada movimento do ponteiro.
    let mut visto = vec![false; w * h];
    let mut pilha = vec![inicio];
    let idx = |x: u32, y: u32| (y - y0) as usize * w + (x - x0) as usize;
    visto[idx(inicio.0, inicio.1)] = true;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (inicio.0, inicio.1, inicio.0, inicio.1);

    while let Some((x, y)) = pilha.pop() {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        let vizinhos = [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ];
        for (nx, ny) in vizinhos {
            if nx < x0 || nx >= x1 || ny < y0 || ny >= y1 {
                continue;
            }
            let slot = idx(nx, ny);
            if visto[slot] || !parecido(img.pixel(nx, ny), alvo) {
                continue;
            }
            visto[slot] = true;
            pilha.push((nx, ny));
        }
    }

    let (largura, altura) = (max_x - min_x + 1, max_y - min_y + 1);
    if largura < MIN_LADO || altura < MIN_LADO {
        return None;
    }
    // Encostar no alcance dos dois lados de um eixo quer dizer que a
    // superfície não coube na busca: é o fundo, não um elemento.
    let estourou_x = min_x == x0 && max_x == x1 - 1 && x0 > 0 && x1 < iw;
    let estourou_y = min_y == y0 && max_y == y1 - 1 && y0 > 0 && y1 < ih;
    if estourou_x && estourou_y {
        return None;
    }
    Some((min_x, min_y, largura, altura))
}

/// A cor que mais aparece numa janelinha em volta do cursor.
///
/// Sobre uma letra, o glifo é fino e o fundo é maioria — que é exatamente o
/// que se quer: a superfície em que a letra está pousada.
fn cor_dominante(img: &RgbaImage, cx: u32, cy: u32) -> [u8; 4] {
    let mut contagem: Vec<([u8; 4], u32)> = Vec::new();
    for dy in -AMOSTRA..=AMOSTRA {
        for dx in -AMOSTRA..=AMOSTRA {
            let x = cx as i64 + dx as i64;
            let y = cy as i64 + dy as i64;
            if x < 0 || y < 0 || x >= img.width() as i64 || y >= img.height() as i64 {
                continue;
            }
            let cor = img.pixel(x as u32, y as u32);
            match contagem.iter_mut().find(|(c, _)| parecido(*c, cor)) {
                Some((_, n)) => *n += 1,
                None => contagem.push((cor, 1)),
            }
        }
    }
    contagem
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(c, _)| c)
        .unwrap_or_else(|| img.pixel(cx, cy))
}

/// O pixel da superfície mais próximo do cursor, procurado em anéis.
fn vizinho_da_superficie(img: &RgbaImage, cx: u32, cy: u32, alvo: [u8; 4]) -> Option<(u32, u32)> {
    if parecido(img.pixel(cx, cy), alvo) {
        return Some((cx, cy));
    }
    for raio in 1..=AMOSTRA {
        for dy in -raio..=raio {
            for dx in -raio..=raio {
                if dx.abs() != raio && dy.abs() != raio {
                    continue; // só a casca do anel
                }
                let x = cx as i64 + dx as i64;
                let y = cy as i64 + dy as i64;
                if x < 0 || y < 0 || x >= img.width() as i64 || y >= img.height() as i64 {
                    continue;
                }
                if parecido(img.pixel(x as u32, y as u32), alvo) {
                    return Some((x as u32, y as u32));
                }
            }
        }
    }
    None
}

fn parecido(a: [u8; 4], b: [u8; 4]) -> bool {
    (0..3).all(|c| (a[c] as i32 - b[c] as i32).abs() <= TOLERANCIA)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma "janela" cinza sobre um fundo escuro, com um "botão" azul dentro e
    /// um "texto" preto sobre o botão.
    fn tela() -> RgbaImage {
        let mut img = RgbaImage::filled(300, 200, [20, 20, 24, 255]);
        preencher(&mut img, 40, 30, 220, 140, [235, 235, 238, 255]); // janela
        preencher(&mut img, 70, 60, 120, 34, [40, 120, 220, 255]); // botão
        // "Texto" no meio do botão: barras finas, como glifos.
        for x in (85..175).step_by(6) {
            preencher(&mut img, x, 70, 2, 14, [10, 10, 10, 255]);
        }
        img
    }

    fn preencher(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, cor: [u8; 4]) {
        for j in y..y + h {
            for i in x..x + w {
                img.pixel_mut(i, j).copy_from_slice(&cor);
            }
        }
    }

    #[test]
    fn o_cursor_no_botao_devolve_o_botao() {
        let img = tela();
        assert_eq!(element_at(&img, 75, 65), Some((70, 60, 120, 34)));
    }

    #[test]
    fn o_cursor_sobre_o_texto_devolve_o_botao_e_nao_a_letra() {
        // A cor de baixo do cursor é a tinta do glifo; a região conectada
        // dela seria uma barrinha de 2 px.
        let img = tela();
        let (x, y, w, h) = element_at(&img, 85, 77).expect("achou algo");
        assert_eq!((x, y, w, h), (70, 60, 120, 34), "veio a letra, não o botão");
    }

    #[test]
    fn o_cursor_na_janela_devolve_a_janela() {
        let img = tela();
        assert_eq!(element_at(&img, 50, 150), Some((40, 30, 220, 140)));
    }

    #[test]
    fn o_fundo_da_tela_nao_e_um_alvo() {
        // Uma superfície que atravessa o alcance da busca nos dois eixos é o
        // fundo, e selecionar o fundo não ajuda ninguém.
        let img = RgbaImage::filled(2000, 2000, [30, 30, 30, 255]);
        assert_eq!(element_at(&img, 1000, 1000), None);
    }

    #[test]
    fn um_glifo_solto_e_pequeno_demais() {
        let mut img = RgbaImage::filled(100, 100, [240, 240, 240, 255]);
        preencher(&mut img, 50, 50, 3, 9, [0, 0, 0, 255]);
        // Mesmo mirando no glifo, a dominante em volta é o fundo — e o fundo
        // desta imagem cabe na busca, então volta a imagem inteira.
        let achado = element_at(&img, 51, 54).expect("achou algo");
        assert!(achado.2 >= MIN_LADO && achado.3 >= MIN_LADO);
    }

    #[test]
    fn fora_da_imagem_nao_quebra() {
        let img = tela();
        assert_eq!(element_at(&img, 300, 10), None);
        assert_eq!(element_at(&img, 10, 200), None);
    }

    #[test]
    fn a_tolerancia_aceita_um_degrade_suave() {
        // Interfaces têm degradês e anti-aliasing; um botão com rampa leve
        // continua sendo um botão só.
        let mut img = RgbaImage::filled(200, 200, [20, 20, 20, 255]);
        for j in 60..100u32 {
            let v = 200 + (j - 60) as u8 / 8;
            preencher(&mut img, 50, j, 100, 1, [v, v, v, 255]);
        }
        let (x, y, w, h) = element_at(&img, 100, 80).expect("achou o botão");
        assert_eq!((x, y, w, h), (50, 60, 100, 40));
    }
}
