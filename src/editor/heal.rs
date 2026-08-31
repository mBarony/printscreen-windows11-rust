//! Remover objeto: apaga um retângulo e preenche o buraco com o que estaria
//! atrás dele.
//!
//! Numa captura de tela o fundo por trás de um elemento é quase sempre liso
//! ou um gradiente suave, e propagar a cor da borda para dentro resolve esse
//! caso **exatamente**: um fundo chapado volta chapado, um degradê volta
//! degradê. Sobre foto ou textura o resultado é um borrão, e isso é aceitável
//! — o alvo aqui é a interface, não o retrato.
//!
//! O método é a equação de Laplace com a borda como condição de contorno,
//! resolvida por iteração: começa com uma interpolação entre as quatro
//! arestas e alisa até as emendas sumirem.

use crate::imgbuf::RgbaImage;

use super::shapes::Point;

/// Passadas de alisamento. Poucas deixam a emenda das quatro arestas à
/// mostra; muitas não melhoram mais nada e custam tempo — o ganho some por
/// volta da vigésima.
const ITERACOES: usize = 24;

/// Apaga o retângulo e o preenche a partir da borda.
///
/// A imagem é modificada no lugar. Os limites são presos à imagem, e um
/// retângulo que não sobre nada depois disso é ignorado.
pub fn apply(img: &mut RgbaImage, min: Point, max: Point) {
    let (iw, ih) = (img.width(), img.height());
    let x0 = min.x.floor().max(0.0) as u32;
    let y0 = min.y.floor().max(0.0) as u32;
    let x1 = (max.x.ceil().max(0.0) as u32).min(iw);
    let y1 = (max.y.ceil().max(0.0) as u32).min(ih);
    if x0 + 1 >= x1 || y0 + 1 >= y1 {
        return;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);

    // As quatro arestas de fora do buraco. Onde o buraco encosta na moldura
    // da imagem não há vizinho: vale a aresta oposta, que é o que faz um
    // retângulo colado na borda ainda ter de onde puxar cor.
    let (lin_acima, lin_abaixo) = vizinhas(y0, y1, ih);
    let (col_esquerda, col_direita) = vizinhas(x0, x1, iw);
    let amostra = |x: u32, y: u32| -> [f32; 4] {
        let p = img.pixel(x, y);
        [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
    };
    let acima: Vec<[f32; 4]> = (x0..x1).map(|x| amostra(x, lin_acima)).collect();
    let abaixo: Vec<[f32; 4]> = (x0..x1).map(|x| amostra(x, lin_abaixo)).collect();
    let esquerda: Vec<[f32; 4]> =
        (y0..y1).map(|y| amostra(col_esquerda, y)).collect();
    let direita: Vec<[f32; 4]> = (y0..y1).map(|y| amostra(col_direita, y)).collect();

    // Chute inicial: média das quatro arestas ponderada pelo inverso da
    // distância a cada uma. Num fundo chapado isto já é a resposta final, e
    // as iterações não têm o que corrigir.
    let mut buffer = vec![[0.0f32; 4]; w * h];
    for j in 0..h {
        for i in 0..w {
            let (dx0, dx1) = ((i + 1) as f32, (w - i) as f32);
            let (dy0, dy1) = ((j + 1) as f32, (h - j) as f32);
            let pesos = [1.0 / dy0, 1.0 / dy1, 1.0 / dx0, 1.0 / dx1];
            let total: f32 = pesos.iter().sum();
            let vizinhos = [acima[i], abaixo[i], esquerda[j], direita[j]];
            let mut cor = [0.0f32; 4];
            for (peso, vizinho) in pesos.iter().zip(vizinhos) {
                for c in 0..4 {
                    cor[c] += vizinho[c] * peso;
                }
            }
            for canal in &mut cor {
                *canal /= total;
            }
            buffer[j * w + i] = cor;
        }
    }

    // Alisamento: cada pixel vira a média dos quatro vizinhos, com a borda
    // fixa. É a equação de Laplace resolvida por Jacobi.
    let mut proximo = buffer.clone();
    for _ in 0..ITERACOES {
        for j in 0..h {
            for i in 0..w {
                let vizinho = |di: isize, dj: isize| -> [f32; 4] {
                    let (ni, nj) = (i as isize + di, j as isize + dj);
                    if ni < 0 {
                        esquerda[j]
                    } else if ni >= w as isize {
                        direita[j]
                    } else if nj < 0 {
                        acima[i]
                    } else if nj >= h as isize {
                        abaixo[i]
                    } else {
                        buffer[nj as usize * w + ni as usize]
                    }
                };
                let quatro = [
                    vizinho(-1, 0),
                    vizinho(1, 0),
                    vizinho(0, -1),
                    vizinho(0, 1),
                ];
                let mut cor = [0.0f32; 4];
                for v in quatro {
                    for c in 0..4 {
                        cor[c] += v[c] / 4.0;
                    }
                }
                proximo[j * w + i] = cor;
            }
        }
        std::mem::swap(&mut buffer, &mut proximo);
    }

    for j in 0..h {
        for i in 0..w {
            let cor = buffer[j * w + i];
            let px = img.pixel_mut(x0 + i as u32, y0 + j as u32);
            for c in 0..4 {
                px[c] = cor[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// As duas linhas (ou colunas) de fora do buraco num eixo, dado que ele ocupa
/// `inicio..fim` dentro de `0..limite`.
///
/// Encostado na moldura o buraco não tem vizinho daquele lado, e aí vale a
/// aresta oposta. Prender o índice ao limite — o que `saturating_sub(1)` e o
/// clamp faziam — devolvia a primeira linha de DENTRO do buraco: a condição de
/// contorno virava a cor do próprio objeto e o remendo o reconstruía.
fn vizinhas(inicio: u32, fim: u32, limite: u32) -> (u32, u32) {
    match (inicio > 0, fim < limite) {
        (true, true) => (inicio - 1, fim),
        (true, false) => (inicio - 1, inicio - 1),
        (false, true) => (fim, fim),
        // O buraco cobre o eixo inteiro: não sobrou fundo de onde puxar cor.
        (false, false) => (inicio, fim - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn um_fundo_chapado_volta_chapado() {
        // O caso comum de uma captura de tela: o objeto some e ninguém vê
        // onde ele estava.
        let fundo = [240, 240, 245, 255];
        let mut img = RgbaImage::filled(40, 40, fundo);
        for y in 12..28u32 {
            for x in 12..28u32 {
                img.pixel_mut(x, y).copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        apply(&mut img, p(12.0, 12.0), p(28.0, 28.0));
        for y in 12..28u32 {
            for x in 12..28u32 {
                assert_eq!(img.pixel(x, y), fundo, "sobrou objeto em ({x}, {y})");
            }
        }
    }

    #[test]
    fn um_degrade_horizontal_continua_degrade() {
        // Propagar a borda tem de reconstruir a rampa, não achatá-la numa
        // cor média — senão o remendo aparece como um bloco chapado.
        let mut img = RgbaImage::filled(60, 20, [0, 0, 0, 255]);
        for y in 0..20u32 {
            for x in 0..60u32 {
                let v = (x * 4) as u8;
                img.pixel_mut(x, y).copy_from_slice(&[v, v, v, 255]);
            }
        }
        let original: Vec<u8> = (0..60u32).map(|x| img.pixel(x, 10)[0]).collect();
        apply(&mut img, p(20.0, 5.0), p(40.0, 15.0));
        for x in 21..39u32 {
            let esperado = original[x as usize] as i32;
            let obtido = img.pixel(x, 10)[0] as i32;
            assert!(
                (obtido - esperado).abs() <= 6,
                "em x={x} esperava ~{esperado}, veio {obtido}"
            );
        }
    }

    #[test]
    fn um_retangulo_colado_na_borda_nao_quebra() {
        // Sem vizinho de um lado, vale o do outro.
        let mut img = RgbaImage::filled(20, 20, [10, 200, 10, 255]);
        apply(&mut img, p(0.0, 0.0), p(10.0, 10.0));
        assert_eq!(img.pixel(2, 2), [10, 200, 10, 255]);
        apply(&mut img, p(15.0, 15.0), p(40.0, 40.0));
        assert_eq!(img.pixel(18, 18), [10, 200, 10, 255]);
    }

    #[test]
    fn um_objeto_colado_na_borda_some_em_vez_de_voltar() {
        // Com o objeto encostado no topo, a linha "de cima" do buraco é a
        // linha 0, que é o próprio objeto. Tomá-la como contorno pintava o
        // remendo de vermelho — o objeto reaparecia em vez de sumir.
        let fundo = [30, 60, 90, 255];
        let mut img = RgbaImage::filled(20, 20, fundo);
        for y in 0..6u32 {
            for x in 4..16u32 {
                img.pixel_mut(x, y).copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        apply(&mut img, p(4.0, 0.0), p(16.0, 6.0));
        for y in 0..6u32 {
            for x in 4..16u32 {
                assert_eq!(img.pixel(x, y), fundo, "sobrou objeto em ({x}, {y})");
            }
        }
    }

    #[test]
    fn retangulo_degenerado_e_ignorado() {
        let antes = RgbaImage::filled(10, 10, [7, 7, 7, 255]);
        let mut img = antes.clone();
        apply(&mut img, p(5.0, 5.0), p(5.0, 5.0));
        apply(&mut img, p(-50.0, -50.0), p(-40.0, -40.0));
        apply(&mut img, p(3.0, 3.0), p(3.5, 9.0));
        assert_eq!(img.as_raw(), antes.as_raw());
    }
}
