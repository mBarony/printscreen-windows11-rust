//! Gerador mínimo de símbolos QR — existe só para os testes.
//!
//! Sem ele não há entrada para exercitar o decodificador: não há crate de QR na
//! árvore nem gerador no ambiente. Ele é deliberadamente pobre — só o modo
//! byte, sem escolha automática de versão nem de máscara — porque o que se pede
//! dele é ser entrada válida, não ser bom.
//!
//! O que ele **não** prova: que o decodificador lê um QR de verdade. Gerador e
//! decodificador escritos juntos erram juntos, e o teste de ida-e-volta passa do
//! mesmo jeito. O que quebra esse ciclo são as tabelas conferidas contra os
//! valores publicados (`tabelas`) e a contagem de módulos de dados que fecha nas
//! 40 versões (`formato`): as duas vêm de fora e prendem a implementação ao
//! padrão, não a si mesma.

use super::formato;
use super::galois;
use super::grade::Grade;
use super::tabelas::{self, Nivel};
use crate::imgbuf::RgbaImage;

/// Monta o símbolo para `texto`, na versão, nível e máscara pedidos.
///
/// Entra em pânico se o texto não couber — é código de teste, e um `Option` aqui
/// só adiaria a mesma falha para uma linha adiante.
pub fn simbolo(texto: &str, versao: u8, nivel: Nivel, mascara: u8) -> Grade {
    let blocos = tabelas::blocos(versao, nivel);
    let fluxo = intercala(&bitstream(texto, versao, &blocos), &blocos);

    let lado = versao as usize * 4 + 17;
    let mut g = Grade::nova(lado);
    desenha_funcoes(&mut g, versao);
    coloca_dados(&mut g, versao, &fluxo);
    aplica_mascara(&mut g, versao, mascara);
    grava_formato(&mut g, nivel, mascara);
    g
}

/// Codewords de dados: cabeçalho do segmento byte, o texto, terminador e o
/// preenchimento alternado que o padrão manda.
fn bitstream(texto: &str, versao: u8, blocos: &tabelas::Blocos) -> Vec<u8> {
    let bytes = texto.as_bytes();
    let bits_contador = if versao <= 9 { 8 } else { 16 };
    let capacidade = blocos.total_dados();

    let mut bits: Vec<bool> = Vec::new();
    let empurra = |valor: u32, n: usize, bits: &mut Vec<bool>| {
        for i in (0..n).rev() {
            bits.push(valor & (1 << i) != 0);
        }
    };
    empurra(0b0100, 4, &mut bits);
    empurra(bytes.len() as u32, bits_contador, &mut bits);
    for &b in bytes {
        empurra(b as u32, 8, &mut bits);
    }
    assert!(bits.len() <= capacidade * 8, "{} bytes não cabem na versão {versao}", bytes.len());

    // Terminador de até 4 bits, e o resto do byte em zeros.
    let terminador = (capacidade * 8 - bits.len()).min(4);
    bits.extend(std::iter::repeat_n(false, terminador));
    let ate_o_byte = (8 - bits.len() % 8) % 8;
    bits.extend(std::iter::repeat_n(false, ate_o_byte));

    let mut dados: Vec<u8> = bits
        .chunks(8)
        .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | u8::from(b)))
        .collect();
    // 0xEC e 0x11 alternados: são os bytes de enchimento do padrão.
    for i in 0..(capacidade - dados.len()) {
        dados.push(if i % 2 == 0 { 0xEC } else { 0x11 });
    }
    dados
}

/// Divide em blocos, calcula a paridade e intercala como o símbolo grava.
fn intercala(dados: &[u8], b: &tabelas::Blocos) -> Vec<u8> {
    let mut pedacos: Vec<&[u8]> = Vec::with_capacity(b.total_blocos());
    let mut k = 0usize;
    for i in 0..b.total_blocos() {
        let n = if i < b.blocos_g1 { b.dados_g1 } else { b.dados_g2 };
        pedacos.push(&dados[k..k + n]);
        k += n;
    }
    let paridades: Vec<Vec<u8>> =
        pedacos.iter().map(|p| galois::paridade(p, b.ec_por_bloco)).collect();

    let mut fluxo = Vec::with_capacity(b.total_codewords());
    for coluna in 0..b.dados_g1.max(b.dados_g2) {
        for p in &pedacos {
            if coluna < p.len() {
                fluxo.push(p[coluna]);
            }
        }
    }
    for coluna in 0..b.ec_por_bloco {
        for p in &paridades {
            fluxo.push(p[coluna]);
        }
    }
    fluxo
}

fn desenha_funcoes(g: &mut Grade, versao: u8) {
    let lado = g.lado();

    for (ox, oy) in [(0, 0), (lado - 7, 0), (0, lado - 7)] {
        for y in 0..7 {
            for x in 0..7 {
                let anel = x == 0 || x == 6 || y == 0 || y == 6;
                let miolo = (2..=4).contains(&x) && (2..=4).contains(&y);
                g.marca(ox + x, oy + y, anel || miolo);
            }
        }
    }

    for i in 8..lado - 8 {
        g.marca(i, 6, i % 2 == 0);
        g.marca(6, i, i % 2 == 0);
    }

    // O módulo escuro, que é sempre escuro e não significa nada.
    g.marca(8, lado - 8, true);

    let centros = tabelas::centros_alinhamento(versao);
    let n = centros.len();
    for (i, &cy) in centros.iter().enumerate() {
        for (j, &cx) in centros.iter().enumerate() {
            if (i == 0 && j == 0) || (i == 0 && j == n - 1) || (i == n - 1 && j == 0) {
                continue;
            }
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let anel = dx.abs() == 2 || dy.abs() == 2;
                    let centro = dx == 0 && dy == 0;
                    g.marca(
                        (cx as i32 + dx) as usize,
                        (cy as i32 + dy) as usize,
                        anel || centro,
                    );
                }
            }
        }
    }

    if let Some(info) = tabelas::info_versao(versao) {
        for i in 0..18 {
            let bit = info & (1 << i) != 0;
            let (a, b) = (i / 3, lado - 11 + i % 3);
            g.marca(a, b, bit);
            g.marca(b, a, bit);
        }
    }
}

/// Escreve os codewords no mesmo zigue-zague que a leitura percorre.
fn coloca_dados(g: &mut Grade, versao: u8, fluxo: &[u8]) {
    let lado = g.lado();
    let mut i = 0usize;
    let mut coluna = lado - 1;
    loop {
        if coluna == 6 {
            coluna = 5;
        }
        for passo in 0..lado {
            for j in 0..2 {
                let x = coluna - j;
                let subindo = (coluna + 1) & 2 == 0;
                let y = if subindo { lado - 1 - passo } else { passo };
                if formato::e_funcao(versao, x, y) {
                    continue;
                }
                let bit = fluxo
                    .get(i / 8)
                    .is_some_and(|byte| byte & (1 << (7 - i % 8)) != 0);
                g.marca(x, y, bit);
                i += 1;
            }
        }
        if coluna < 2 {
            break;
        }
        coluna -= 2;
    }
}

fn aplica_mascara(g: &mut Grade, versao: u8, mascara: u8) {
    for y in 0..g.lado() {
        for x in 0..g.lado() {
            if !formato::e_funcao(versao, x, y) && formato::mascarado(mascara, y, x) {
                g.marca(x, y, !g.escuro(x, y));
            }
        }
    }
}

fn grava_formato(g: &mut Grade, nivel: Nivel, mascara: u8) {
    let lado = g.lado();
    let palavra = tabelas::formato(nivel, mascara);
    let bit = |i: usize| palavra & (1 << i) != 0;

    for i in 0..6 {
        g.marca(8, i, bit(i));
    }
    g.marca(8, 7, bit(6));
    g.marca(8, 8, bit(7));
    g.marca(7, 8, bit(8));
    for i in 9..15 {
        g.marca(14 - i, 8, bit(i));
    }

    for i in 0..8 {
        g.marca(lado - 1 - i, 8, bit(i));
    }
    for i in 8..15 {
        g.marca(8, lado - 15 + i, bit(i));
    }
}

/// Desenha a grade como imagem, com `escala` px por módulo e uma margem clara
/// de `margem` módulos em volta — a zona de silêncio, sem a qual nem um leitor
/// de verdade acha o símbolo.
pub fn imagem(grade: &Grade, escala: u32, margem: u32) -> RgbaImage {
    let lado_px = (grade.lado() as u32 + 2 * margem) * escala;
    let mut img = RgbaImage::filled(lado_px, lado_px, [255, 255, 255, 255]);
    for my in 0..grade.lado() {
        for mx in 0..grade.lado() {
            if !grade.escuro(mx, my) {
                continue;
            }
            let x0 = (mx as u32 + margem) * escala;
            let y0 = (my as u32 + margem) * escala;
            for y in y0..y0 + escala {
                for x in x0..x0 + escala {
                    img.pixel_mut(x, y).copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
    }
    img
}
