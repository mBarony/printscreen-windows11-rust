//! Da imagem à grade de módulos.
//!
//! O alvo é um QR **numa captura de tela**: nítido, sem sombra e sem
//! perspectiva de câmera. Isso permite um caminho bem mais curto que o de um
//! leitor de celular — limiar global em vez de adaptativo, e um mapeamento afim
//! a partir dos três localizadores em vez de homografia refinada por padrão de
//! alinhamento. O que continua valendo é rotação e escala: um QR colado torto
//! num documento ainda é um caso real, e o mapeamento afim o cobre.
//!
//! O reconhecimento dos localizadores é o clássico: numa linha qualquer que
//! atravesse um deles, as faixas escura-clara-escura-clara-escura medem
//! 1:1:3:1:1. Achar essa proporção horizontalmente é barato e dá muitos
//! candidatos; confirmá-la verticalmente no mesmo ponto elimina quase todos os
//! falsos.

use super::grade::Grade;
use crate::imgbuf::RgbaImage;

/// Um padrão localizador candidato: onde está e de que tamanho é o módulo.
#[derive(Clone, Copy, Debug)]
struct Canto {
    x: f32,
    y: f32,
    modulo: f32,
}

/// Acha o símbolo na imagem e amostra seus módulos.
///
/// `None` quando não há três localizadores plausíveis — que é o caso comum,
/// porque este caminho é tentado em toda seleção do comando de reconhecer.
pub fn grade(image: &RgbaImage) -> Option<Grade> {
    let (escuro, largura, altura) = binariza(image);
    let cantos = localizadores(&escuro, largura, altura);
    let (tl, tr, bl) = ordena(&cantos)?;

    let modulo = (tl.modulo + tr.modulo + bl.modulo) / 3.0;
    let lado = lado_do_simbolo(tl, tr, modulo)?;

    Some(amostra(&escuro, largura, altura, tl, tr, bl, lado))
}

/// Luminância + limiar de Otsu.
///
/// Global, e não adaptativo, porque a entrada é uma captura de tela: contraste
/// uniforme e sem gradiente de iluminação. Um limiar por janela custaria caro
/// para resolver um problema que não existe aqui.
fn binariza(image: &RgbaImage) -> (Vec<bool>, usize, usize) {
    let largura = image.width() as usize;
    let altura = image.height() as usize;
    let mut luz = Vec::with_capacity(largura * altura);
    let mut histograma = [0u32; 256];

    for y in 0..altura {
        for x in 0..largura {
            let p = image.pixel(x as u32, y as u32);
            // Coeficientes de luminância perceptual; o alfa não entra porque a
            // captura é sempre opaca.
            let l = ((p[0] as u32 * 77 + p[1] as u32 * 151 + p[2] as u32 * 28) >> 8) as u8;
            histograma[l as usize] += 1;
            luz.push(l);
        }
    }

    let limiar = otsu(&histograma, (largura * altura) as u32);
    let escuro = luz.iter().map(|&l| l <= limiar).collect();
    (escuro, largura, altura)
}

/// Limiar que separa as duas populações de luminância maximizando a variância
/// entre elas.
fn otsu(histograma: &[u32; 256], total: u32) -> u8 {
    let soma_total: u64 = histograma.iter().enumerate().map(|(i, &n)| i as u64 * n as u64).sum();
    let mut soma_fundo = 0u64;
    let mut peso_fundo = 0u32;
    let mut melhor = (0.0f64, 0u8);

    for (t, &quantos) in histograma.iter().enumerate() {
        peso_fundo += quantos;
        if peso_fundo == 0 {
            continue;
        }
        let peso_frente = total - peso_fundo;
        if peso_frente == 0 {
            break;
        }
        soma_fundo += t as u64 * quantos as u64;

        let media_fundo = soma_fundo as f64 / peso_fundo as f64;
        let media_frente = (soma_total - soma_fundo) as f64 / peso_frente as f64;
        let diferenca = media_fundo - media_frente;
        let variancia = peso_fundo as f64 * peso_frente as f64 * diferenca * diferenca;

        if variancia > melhor.0 {
            melhor = (variancia, t as u8);
        }
    }
    melhor.1
}

fn e_escuro(escuro: &[bool], largura: usize, altura: usize, x: isize, y: isize) -> bool {
    if x < 0 || y < 0 || x as usize >= largura || y as usize >= altura {
        return false;
    }
    escuro[y as usize * largura + x as usize]
}

/// As cinco faixas medem 1:1:3:1:1? Devolve o tamanho do módulo se sim.
///
/// A tolerância é de meio módulo por faixa, como fazem os leitores de verdade:
/// mais apertado recusa QR redimensionado com interpolação, mais frouxo aceita
/// qualquer listra da interface.
fn proporcao(faixas: [usize; 5]) -> Option<f32> {
    let total: usize = faixas.iter().sum();
    if total < 7 {
        return None;
    }
    let modulo = total as f32 / 7.0;
    let folga = modulo / 2.0;
    let esperado = [1.0, 1.0, 3.0, 1.0, 1.0];
    for (i, &f) in faixas.iter().enumerate() {
        if (f as f32 - esperado[i] * modulo).abs() > folga {
            return None;
        }
    }
    Some(modulo)
}

/// Varre as linhas atrás da assinatura 1:1:3:1:1 e confirma cada acerto na
/// vertical.
fn localizadores(escuro: &[bool], largura: usize, altura: usize) -> Vec<Canto> {
    let mut brutos: Vec<Canto> = Vec::new();

    for y in 0..altura {
        let mut faixas = [0usize; 5];
        let mut cor_atual = false; // começa contando claro
        let mut x = 0usize;

        while x < largura {
            let escuro_aqui = escuro[y * largura + x];
            if escuro_aqui == cor_atual {
                faixas[4] += 1;
            } else if escuro_aqui {
                // Vira escuro: fecha a faixa clara e abre uma escura.
                faixas.rotate_left(1);
                faixas[4] = 1;
                cor_atual = true;
            } else {
                faixas.rotate_left(1);
                faixas[4] = 1;
                cor_atual = false;
            }

            // Só faz sentido conferir quando a última faixa é escura, que é o
            // fim de um "escuro-claro-escuro-claro-escuro".
            if cor_atual && faixas[0] > 0 {
                if let Some(modulo) = proporcao(faixas) {
                    let fim = x + 1;
                    let centro_x = fim as f32 - faixas[4] as f32 - faixas[3] as f32
                        - faixas[2] as f32 / 2.0;
                    if let Some(centro_y) =
                        confirma_vertical(escuro, largura, altura, centro_x as usize, y, modulo)
                    {
                        brutos.push(Canto { x: centro_x, y: centro_y, modulo });
                    }
                }
            }
            x += 1;
        }
    }

    agrupa(brutos)
}

/// Repete a medida na vertical passando pelo centro achado na horizontal.
///
/// É o filtro que separa um localizador de qualquer listra da interface: uma
/// barra horizontal casa a proporção numa linha e falha na coluna.
fn confirma_vertical(
    escuro: &[bool],
    largura: usize,
    altura: usize,
    x: usize,
    y: usize,
    modulo: f32,
) -> Option<f32> {
    let dentro = |yy: isize| e_escuro(escuro, largura, altura, x as isize, yy);
    if !dentro(y as isize) {
        return None;
    }

    let mut faixas = [0usize; 5];
    // Faixa central: sobe e desce enquanto for escuro.
    let mut topo = y as isize;
    while dentro(topo - 1) {
        topo -= 1;
    }
    let mut base = y as isize;
    while dentro(base + 1) {
        base += 1;
    }
    faixas[2] = (base - topo + 1) as usize;

    let mut i = topo - 1;
    while i >= 0 && !dentro(i) {
        faixas[1] += 1;
        i -= 1;
    }
    while i >= 0 && dentro(i) {
        faixas[0] += 1;
        i -= 1;
    }

    let mut j = base + 1;
    while (j as usize) < altura && !dentro(j) {
        faixas[3] += 1;
        j += 1;
    }
    while (j as usize) < altura && dentro(j) {
        faixas[4] += 1;
        j += 1;
    }

    let vertical = proporcao(faixas)?;
    // Um localizador é quadrado: módulo horizontal e vertical têm de bater.
    if (vertical - modulo).abs() > modulo / 2.0 {
        return None;
    }
    Some((topo + base) as f32 / 2.0)
}

/// Junta candidatos que descrevem o mesmo localizador — cada linha que
/// atravessa um deles gera um.
fn agrupa(brutos: Vec<Canto>) -> Vec<Canto> {
    let mut grupos: Vec<(Canto, usize)> = Vec::new();

    for c in brutos {
        let mut juntou = false;
        for (centro, n) in grupos.iter_mut() {
            let perto = (centro.x - c.x).abs() < centro.modulo * 2.0
                && (centro.y - c.y).abs() < centro.modulo * 2.0;
            if perto {
                let peso = *n as f32;
                centro.x = (centro.x * peso + c.x) / (peso + 1.0);
                centro.y = (centro.y * peso + c.y) / (peso + 1.0);
                centro.modulo = (centro.modulo * peso + c.modulo) / (peso + 1.0);
                *n += 1;
                juntou = true;
                break;
            }
        }
        if !juntou {
            grupos.push((c, 1));
        }
    }

    // Um localizador de verdade é confirmado por várias linhas; um casual, por
    // uma ou duas. O corte tira ruído antes da busca combinatória.
    grupos.retain(|(_, n)| *n >= 2);
    grupos.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    grupos.truncate(8);
    grupos.into_iter().map(|(c, _)| c).collect()
}

/// Escolhe os três que formam o triângulo retângulo isósceles do símbolo e os
/// devolve na ordem (superior-esquerdo, superior-direito, inferior-esquerdo).
fn ordena(cantos: &[Canto]) -> Option<(Canto, Canto, Canto)> {
    if cantos.len() < 3 {
        return None;
    }

    let dist2 = |a: &Canto, b: &Canto| {
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        dx * dx + dy * dy
    };

    let mut melhor: Option<(f32, usize, usize, usize)> = None;
    for i in 0..cantos.len() {
        for j in (i + 1)..cantos.len() {
            for k in (j + 1)..cantos.len() {
                // O canto do ângulo reto é o oposto ao lado mais longo.
                let lados = [
                    (dist2(&cantos[j], &cantos[k]), i, j, k),
                    (dist2(&cantos[i], &cantos[k]), j, i, k),
                    (dist2(&cantos[i], &cantos[j]), k, i, j),
                ];
                let (hip, canto, a, b) =
                    lados.iter().copied().fold(lados[0], |m, l| if l.0 > m.0 { l } else { m });

                let ca = dist2(&cantos[canto], &cantos[a]);
                let cb = dist2(&cantos[canto], &cantos[b]);
                if ca <= 0.0 || cb <= 0.0 {
                    continue;
                }
                // Isósceles e retângulo: os dois catetos iguais e a hipotenusa
                // igual à soma dos quadrados. O erro relativo é o critério.
                let erro_isosceles = (ca - cb).abs() / ca.max(cb);
                let erro_reto = (hip - ca - cb).abs() / hip;
                let erro = erro_isosceles + erro_reto;
                if erro < 0.3 && melhor.is_none_or(|(e, ..)| erro < e) {
                    melhor = Some((erro, canto, a, b));
                }
            }
        }
    }

    let (_, canto, a, b) = melhor?;
    let (tl, p, q) = (cantos[canto], cantos[a], cantos[b]);

    // Com y para baixo, (TR−TL)×(BL−TL) é positivo num símbolo não espelhado.
    // O sinal decide qual dos dois é qual, sem tentativa e erro.
    let cruz = (p.x - tl.x) * (q.y - tl.y) - (p.y - tl.y) * (q.x - tl.x);
    if cruz > 0.0 {
        Some((tl, p, q))
    } else {
        Some((tl, q, p))
    }
}

/// Quantos módulos tem o lado, pela distância entre os dois localizadores de
/// cima: são `lado − 7` módulos de centro a centro.
fn lado_do_simbolo(tl: Canto, tr: Canto, modulo: f32) -> Option<usize> {
    let dx = tr.x - tl.x;
    let dy = tr.y - tl.y;
    let distancia = (dx * dx + dy * dy).sqrt();
    let lado = (distancia / modulo).round() as isize + 7;

    // Todo símbolo tem 17 + 4v módulos. Um lado que caia entre dois válidos é
    // arredondado para o mais próximo — a estimativa do módulo tem erro, o
    // conjunto de tamanhos válidos não.
    let versao = ((lado - 17) as f32 / 4.0).round() as isize;
    if !(1..=40).contains(&versao) {
        return None;
    }
    let ajustado = 17 + 4 * versao;
    // Longe demais do válido mais próximo é sinal de que não era um QR.
    if (ajustado - lado).abs() > 2 {
        return None;
    }
    Some(ajustado as usize)
}

/// Amostra os módulos pelo mapeamento afim dos três centros.
///
/// Os centros dos localizadores ficam em (3,5, 3,5), (lado−3,5, 3,5) e
/// (3,5, lado−3,5) em coordenadas de módulo. Três pontos definem a afinidade,
/// que já cobre rotação, escala e cisalhamento — o que falta é só a
/// perspectiva, que uma captura de tela não tem.
fn amostra(
    escuro: &[bool],
    largura: usize,
    altura: usize,
    tl: Canto,
    tr: Canto,
    bl: Canto,
    lado: usize,
) -> Grade {
    let vao = (lado - 7) as f32;
    let dx = ((tr.x - tl.x) / vao, (tr.y - tl.y) / vao);
    let dy = ((bl.x - tl.x) / vao, (bl.y - tl.y) / vao);

    let mut g = Grade::nova(lado);
    for my in 0..lado {
        for mx in 0..lado {
            let u = mx as f32 - 3.5;
            let v = my as f32 - 3.5;
            let px = tl.x + u * dx.0 + v * dy.0;
            let py = tl.y + u * dx.1 + v * dy.1;
            g.marca(mx, my, voto(escuro, largura, altura, px, py));
        }
    }
    g
}

/// Maioria entre o centro do módulo e seus quatro vizinhos imediatos.
///
/// Um pixel só bastaria num QR 1:1, mas basta o símbolo estar redimensionado
/// com interpolação para o centro cair sobre a transição entre dois módulos.
fn voto(escuro: &[bool], largura: usize, altura: usize, px: f32, py: f32) -> bool {
    let amostras = [(0.0, 0.0), (-0.25, 0.0), (0.25, 0.0), (0.0, -0.25), (0.0, 0.25)];
    let escuros = amostras
        .iter()
        .filter(|(ox, oy)| {
            e_escuro(
                escuro,
                largura,
                altura,
                (px + ox).round() as isize,
                (py + oy).round() as isize,
            )
        })
        .count();
    escuros >= 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qr::gera;

    /// Desenha só os três localizadores e os temporizadores de uma versão 1.
    /// Não é um QR válido — é o mínimo que o detector precisa ver.
    fn esqueleto(lado: usize) -> Grade {
        let mut g = Grade::nova(lado);
        for (ox, oy) in [(0, 0), (lado - 7, 0), (0, lado - 7)] {
            for y in 0..7 {
                for x in 0..7 {
                    let borda = x == 0 || x == 6 || y == 0 || y == 6;
                    let miolo = (2..=4).contains(&x) && (2..=4).contains(&y);
                    g.marca(ox + x, oy + y, borda || miolo);
                }
            }
        }
        for i in 8..(lado - 8) {
            g.marca(i, 6, i % 2 == 0);
            g.marca(6, i, i % 2 == 0);
        }
        g
    }

    #[test]
    fn acha_os_tres_localizadores_e_o_tamanho() {
        for lado in [21usize, 25, 45] {
            for escala in [1u32, 3, 8] {
                let img = gera::imagem(&esqueleto(lado), escala, 4);
                let achada = grade(&img).unwrap_or_else(|| {
                    panic!("não detectou lado {lado} na escala {escala}")
                });
                assert_eq!(achada.lado(), lado, "escala {escala}");
                // Os cantos do localizador superior esquerdo têm de bater.
                assert!(achada.escuro(0, 0));
                assert!(achada.escuro(3, 3));
                assert!(!achada.escuro(7, 7));
            }
        }
    }

    #[test]
    fn imagem_sem_qr_e_recusada() {
        let branco = RgbaImage::filled(80, 80, [255, 255, 255, 255]);
        assert!(grade(&branco).is_none());

        let mut listrada = RgbaImage::filled(80, 80, [255, 255, 255, 255]);
        for y in 0..80u32 {
            for x in 0..80u32 {
                if (x / 3) % 2 == 0 {
                    listrada.pixel_mut(x, y).copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        assert!(grade(&listrada).is_none(), "listras não são localizadores");
    }
}
