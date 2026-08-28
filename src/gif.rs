//! Codificador GIF89a próprio, no mesmo espírito do JPEG vendorizado.
//!
//! Serve a um caso só: dois quadros que alternam, o "antes" e o "depois" de
//! uma captura anotada. Por isso não há nada de quadros parciais, transparência
//! nem otimização por diferença — o que existe é o suficiente para esse GIF, e
//! nem uma linha a mais.
//!
//! As duas partes que custam são a **paleta** e o **LZW**. A paleta é única
//! para os dois quadros: dois quadros com paletas próprias piscariam de cor a
//! cada troca, que é justamente o que um "antes e depois" não pode fazer.

use std::io::Write;

use crate::error::{Context as _, Result};
use crate::imgbuf::RgbaImage;

/// Cores da tabela global. É o teto do formato, e usar menos não economizaria
/// nada que importe aqui.
const CORES: usize = 256;
/// Teto de amostras na quantização. Uma captura 4K tem 8 milhões de pixels, e
/// a mediana de uma amostra grande é indistinguível da do conjunto inteiro.
const MAX_AMOSTRAS: usize = 120_000;

/// Grava os quadros como um GIF que repete para sempre.
///
/// `delay_cs` é o tempo de cada quadro em centésimos de segundo — a unidade
/// do formato, não uma escolha.
pub fn encode<W: Write>(mut w: W, frames: &[&RgbaImage], delay_cs: u16) -> Result<()> {
    let (width, height) = match frames.first() {
        Some(first) if first.width() > 0 && first.height() > 0 => (first.width(), first.height()),
        _ => return Err(crate::error::err!("GIF sem quadros")),
    };
    if frames.iter().any(|f| f.width() != width || f.height() != height) {
        return Err(crate::error::err!("os quadros do GIF têm tamanhos diferentes"));
    }
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err(crate::error::err!(
            "imagem grande demais para GIF ({width}×{height}; o limite é 65535 por lado)"
        ));
    }

    let paleta = quantize(frames);
    let mut cache = ColorCache::new(&paleta);

    let escrever = |w: &mut W, bytes: &[u8]| -> Result<()> {
        w.write_all(bytes).context("gravando o GIF")
    };

    // Cabeçalho e descritor de tela lógica: tabela global de 256 cores.
    escrever(&mut w, b"GIF89a")?;
    escrever(&mut w, &(width as u16).to_le_bytes())?;
    escrever(&mut w, &(height as u16).to_le_bytes())?;
    escrever(&mut w, &[0xF7, 0, 0])?; // tabela global, 8 bits por cor
    for cor in &paleta {
        escrever(&mut w, cor)?;
    }

    // Extensão de aplicação NETSCAPE2.0: é assim, e só assim, que um GIF
    // pede para repetir para sempre.
    escrever(&mut w, b"\x21\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00")?;

    for frame in frames {
        // Controle gráfico: o atraso do quadro, sem transparência.
        escrever(&mut w, &[0x21, 0xF9, 0x04, 0x00])?;
        escrever(&mut w, &delay_cs.to_le_bytes())?;
        escrever(&mut w, &[0x00, 0x00])?;
        // Descritor de imagem: o quadro inteiro, sem tabela local.
        escrever(&mut w, &[0x2C, 0, 0, 0, 0])?;
        escrever(&mut w, &(width as u16).to_le_bytes())?;
        escrever(&mut w, &(height as u16).to_le_bytes())?;
        escrever(&mut w, &[0x00])?;

        let indices = cache.index(frame);
        escrever(&mut w, &[8])?; // tamanho mínimo de código
        for bloco in lzw(&indices).chunks(255) {
            escrever(&mut w, &[bloco.len() as u8])?;
            escrever(&mut w, bloco)?;
        }
        escrever(&mut w, &[0x00])?; // fim dos blocos
    }
    escrever(&mut w, &[0x3B])?; // fim do arquivo
    Ok(())
}

// ---------------------------------------------------------------------------
// Paleta
// ---------------------------------------------------------------------------

/// Paleta de 256 cores por corte mediano, sobre uma amostra dos dois quadros.
///
/// O corte mediano parte repetidamente a caixa de cores mais "esticada" no
/// canal mais esticado dela. Contra o histograma popular, ele não perde as
/// cores raras que são justamente as anotações: uma seta vermelha ocupa uma
/// fração de 1% dos pixels e não pode sumir da paleta.
fn quantize(frames: &[&RgbaImage]) -> Vec<[u8; 3]> {
    let total: usize = frames.iter().map(|f| f.as_raw().len() / 4).sum();
    let passo = (total / MAX_AMOSTRAS).max(1);
    let mut amostras: Vec<[u8; 3]> = Vec::new();
    for frame in frames {
        for px in frame.as_raw().as_chunks::<4>().0.iter().step_by(passo) {
            amostras.push([px[0], px[1], px[2]]);
        }
    }
    if amostras.is_empty() {
        return vec![[0, 0, 0]; CORES];
    }

    let mut caixas = vec![amostras];
    while caixas.len() < CORES {
        // A caixa a partir é a de maior amplitude — é onde o erro mora.
        let Some((i, canal)) = caixas
            .iter()
            .enumerate()
            .filter(|(_, c)| c.len() > 1)
            .map(|(i, c)| (i, maior_canal(c)))
            .max_by(|a, b| a.1 .1.cmp(&b.1 .1))
            .map(|(i, (canal, _))| (i, canal))
        else {
            break; // todas as caixas viraram uma cor só
        };
        let mut caixa = caixas.swap_remove(i);
        caixa.sort_unstable_by_key(|c| c[canal]);
        let meio = caixa.len() / 2;
        let resto = caixa.split_off(meio.max(1));
        caixas.push(caixa);
        caixas.push(resto);
    }

    let mut paleta: Vec<[u8; 3]> = caixas.iter().map(|c| media(c)).collect();
    paleta.resize(CORES, [0, 0, 0]);
    paleta
}

/// O canal de maior amplitude na caixa, e essa amplitude.
fn maior_canal(cores: &[[u8; 3]]) -> (usize, u8) {
    let mut melhor = (0usize, 0u8);
    for canal in 0..3 {
        let (mut lo, mut hi) = (255u8, 0u8);
        for c in cores {
            lo = lo.min(c[canal]);
            hi = hi.max(c[canal]);
        }
        let amplitude = hi - lo;
        if amplitude > melhor.1 {
            melhor = (canal, amplitude);
        }
    }
    melhor
}

fn media(cores: &[[u8; 3]]) -> [u8; 3] {
    if cores.is_empty() {
        return [0, 0, 0];
    }
    let mut soma = [0u64; 3];
    for c in cores {
        for canal in 0..3 {
            soma[canal] += c[canal] as u64;
        }
    }
    let n = cores.len() as u64;
    [
        (soma[0] / n) as u8,
        (soma[1] / n) as u8,
        (soma[2] / n) as u8,
    ]
}

/// Matriz de Bayer 4×4, em desvios de −8 a +7.
///
/// O pontilhado é o que impede o gradiente de uma interface de virar faixas
/// numa paleta de 256 cores. Ordenado, e não por difusão de erro: o padrão é
/// o mesmo nos dois quadros, então a área que não mudou entre o "antes" e o
/// "depois" fica **idêntica** e o GIF não ferve.
const BAYER: [[i16; 4]; 4] = [
    [-8, 0, -6, 2],
    [4, -4, 6, -2],
    [-5, 3, -7, 1],
    [7, -1, 5, -3],
];

/// Mapa cor → índice da paleta, com cache por cor de 15 bits.
///
/// Sem o cache, cada pixel custaria uma varredura das 256 cores: numa captura
/// 4K são dois bilhões de comparações por quadro. Com ele, no máximo 32768.
struct ColorCache<'a> {
    paleta: &'a [[u8; 3]],
    tabela: Vec<u16>,
}

impl<'a> ColorCache<'a> {
    fn new(paleta: &'a [[u8; 3]]) -> Self {
        Self { paleta, tabela: vec![u16::MAX; 1 << 15] }
    }

    fn nearest(&mut self, rgb: [u8; 3]) -> u8 {
        let chave = ((rgb[0] as usize >> 3) << 10)
            | ((rgb[1] as usize >> 3) << 5)
            | (rgb[2] as usize >> 3);
        if self.tabela[chave] != u16::MAX {
            return self.tabela[chave] as u8;
        }
        let mut melhor = (0usize, i32::MAX);
        for (i, c) in self.paleta.iter().enumerate() {
            let d = (0..3)
                .map(|k| {
                    let d = rgb[k] as i32 - c[k] as i32;
                    d * d
                })
                .sum::<i32>();
            if d < melhor.1 {
                melhor = (i, d);
            }
        }
        self.tabela[chave] = melhor.0 as u16;
        melhor.0 as u8
    }

    /// Índices da paleta para a imagem inteira, com pontilhado ordenado.
    fn index(&mut self, image: &RgbaImage) -> Vec<u8> {
        let (w, h) = (image.width(), image.height());
        let mut out = Vec::with_capacity(w as usize * h as usize);
        for y in 0..h {
            for x in 0..w {
                let px = image.pixel(x, y);
                let desvio = BAYER[(y % 4) as usize][(x % 4) as usize];
                let ajustada = [
                    (px[0] as i16 + desvio).clamp(0, 255) as u8,
                    (px[1] as i16 + desvio).clamp(0, 255) as u8,
                    (px[2] as i16 + desvio).clamp(0, 255) as u8,
                ];
                out.push(self.nearest(ajustada));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// LZW
// ---------------------------------------------------------------------------

/// Compressão LZW do GIF, com código mínimo de 8 bits.
///
/// O dicionário é reiniciado ao encher os 12 bits, que é o teto do formato —
/// sem isso um quadro grande estouraria o espaço de códigos.
fn lzw(indices: &[u8]) -> Vec<u8> {
    const CLEAR: u16 = 256;
    const END: u16 = 257;

    let mut saida = BitWriter::default();
    let mut dicionario: std::collections::HashMap<(u16, u8), u16> =
        std::collections::HashMap::new();
    let mut largura = 9u8;
    let mut proximo = 258u16;

    saida.push(CLEAR, largura);
    let Some((&primeiro, resto)) = indices.split_first() else {
        saida.push(END, largura);
        return saida.finish();
    };
    let mut atual = primeiro as u16;

    for &byte in resto {
        match dicionario.get(&(atual, byte)) {
            Some(&codigo) => atual = codigo,
            None => {
                saida.push(atual, largura);
                dicionario.insert((atual, byte), proximo);
                proximo += 1;
                // O leitor amplia a largura um código antes de precisar dela;
                // escrever largo cedo demais dessincroniza os dois.
                if proximo > (1 << largura) && largura < 12 {
                    largura += 1;
                } else if proximo >= 4096 {
                    saida.push(CLEAR, largura);
                    dicionario.clear();
                    largura = 9;
                    proximo = 258;
                }
                atual = byte as u16;
            }
        }
    }
    saida.push(atual, largura);
    saida.push(END, largura);
    saida.finish()
}

/// Acumulador de bits do LZW: códigos de largura variável, **do bit menos
/// significativo para o mais**, que é a ordem do formato.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    acumulador: u32,
    bits: u8,
}

impl BitWriter {
    fn push(&mut self, codigo: u16, largura: u8) {
        self.acumulador |= (codigo as u32) << self.bits;
        self.bits += largura;
        while self.bits >= 8 {
            self.bytes.push((self.acumulador & 0xFF) as u8);
            self.acumulador >>= 8;
            self.bits -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.bytes.push((self.acumulador & 0xFF) as u8);
        }
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradiente(w: u32, h: u32, base: u8) -> RgbaImage {
        let mut img = RgbaImage::filled(w, h, [0, 0, 0, 255]);
        for y in 0..h {
            for x in 0..w {
                let v = ((x * 255 / w.max(1)) as u8).wrapping_add(base);
                img.pixel_mut(x, y).copy_from_slice(&[v, v / 2, base, 255]);
            }
        }
        img
    }

    fn encode_vec(frames: &[&RgbaImage]) -> Vec<u8> {
        let mut out = Vec::new();
        encode(&mut out, frames, 100).unwrap();
        out
    }

    #[test]
    fn o_arquivo_tem_a_cara_de_um_gif() {
        let a = gradiente(32, 16, 0);
        let b = gradiente(32, 16, 90);
        let bytes = encode_vec(&[&a, &b]);

        assert_eq!(&bytes[..6], b"GIF89a", "assinatura");
        assert_eq!(&bytes[6..10], &[32, 0, 16, 0], "largura e altura em LE");
        assert_eq!(bytes[10], 0xF7, "tabela global de 256 cores");
        assert_eq!(*bytes.last().unwrap(), 0x3B, "terminador");
        // Sem a extensão NETSCAPE o GIF roda uma vez e para.
        assert!(
            bytes.windows(11).any(|w| w == b"NETSCAPE2.0"),
            "falta o pedido de repetir para sempre"
        );
        // Dois descritores de imagem, um por quadro.
        assert!(bytes.iter().filter(|b| **b == 0x2C).count() >= 2);
    }

    #[test]
    fn quadros_de_tamanhos_diferentes_sao_recusados() {
        let a = gradiente(8, 8, 0);
        let b = gradiente(9, 8, 0);
        let mut out = Vec::new();
        assert!(encode(&mut out, &[&a, &b], 100).is_err());
        assert!(encode(&mut out, &[], 100).is_err(), "sem quadros também");
    }

    #[test]
    fn a_paleta_guarda_as_cores_raras() {
        // Uma anotação ocupa uma fração de 1% dos pixels e mesmo assim não
        // pode sumir da paleta — é ela que o "antes e depois" mostra.
        let mut img = RgbaImage::filled(200, 200, [250, 250, 250, 255]);
        for y in 0..4 {
            for x in 0..40 {
                img.pixel_mut(x, y).copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let paleta = quantize(&[&img]);
        let vermelho = paleta
            .iter()
            .any(|c| c[0] > 200 && c[1] < 60 && c[2] < 60);
        assert!(vermelho, "o vermelho da anotação sumiu da paleta");
    }

    #[test]
    fn o_lzw_comprime_uma_area_chapada() {
        // Mil bytes iguais têm de sair bem menores que mil.
        let comprimido = lzw(&[7u8; 1000]);
        assert!(comprimido.len() < 200, "saíram {} bytes", comprimido.len());
    }

    /// Descompressor LZW do GIF, só para os testes.
    ///
    /// Um codificador que ninguém decodifica é um gerador de bytes bonitos: é
    /// isto que prova que o que sai daqui abre num visualizador.
    fn unlzw(bytes: &[u8]) -> Vec<u8> {
        const CLEAR: u16 = 256;
        const END: u16 = 257;
        let mut bits = 0u32;
        let mut acumulador = 0u32;
        let mut pos = 0usize;
        let mut largura = 9u8;
        let mut tabela: Vec<Vec<u8>> = Vec::new();
        let reiniciar = |tabela: &mut Vec<Vec<u8>>| {
            tabela.clear();
            for i in 0..258u16 {
                tabela.push(if i < 256 { vec![i as u8] } else { Vec::new() });
            }
        };
        reiniciar(&mut tabela);
        let mut anterior: Option<u16> = None;
        let mut out = Vec::new();
        loop {
            while bits < largura as u32 && pos < bytes.len() {
                acumulador |= (bytes[pos] as u32) << bits;
                bits += 8;
                pos += 1;
            }
            if bits < largura as u32 {
                break;
            }
            let codigo = (acumulador & ((1 << largura) - 1)) as u16;
            acumulador >>= largura;
            bits -= largura as u32;

            if codigo == CLEAR {
                reiniciar(&mut tabela);
                largura = 9;
                anterior = None;
                continue;
            }
            if codigo == END {
                break;
            }
            let entrada = match tabela.get(codigo as usize) {
                Some(e) if !e.is_empty() => e.clone(),
                // Caso KwKwK: o código acabou de ser definido pela própria
                // sequência que estamos decodificando.
                _ => {
                    let base = tabela[anterior.expect("KwKwK sem anterior") as usize].clone();
                    let mut e = base.clone();
                    e.push(base[0]);
                    e
                }
            };
            out.extend_from_slice(&entrada);
            if let Some(ant) = anterior {
                let mut nova = tabela[ant as usize].clone();
                nova.push(entrada[0]);
                tabela.push(nova);
            }
            anterior = Some(codigo);
            if tabela.len() + 1 > (1 << largura) && largura < 12 {
                largura += 1;
            }
        }
        out
    }

    #[test]
    fn o_lzw_volta_igual_ao_que_entrou() {
        for entrada in [
            vec![7u8; 1000],
            (0..=255u8).cycle().take(3000).collect::<Vec<_>>(),
            vec![1, 1, 2, 1, 1, 2, 1, 1, 2, 3],
            vec![42],
        ] {
            assert_eq!(unlzw(&lzw(&entrada)), entrada, "entrada de {} bytes", entrada.len());
        }
    }

    #[test]
    fn os_pixels_do_gif_sobrevivem_a_ida_e_volta() {
        // Do índice de paleta ao arquivo e de volta: é o caminho inteiro.
        let img = gradiente(37, 11, 0);
        let paleta = quantize(&[&img]);
        let indices = ColorCache::new(&paleta).index(&img);
        assert_eq!(unlzw(&lzw(&indices)), indices);
    }

    #[test]
    fn o_pontilhado_e_o_mesmo_nos_dois_quadros() {
        // O que não mudou entre o antes e o depois tem de sair idêntico,
        // senão o GIF ferve na área parada.
        let img = gradiente(64, 8, 0);
        let paleta = quantize(&[&img]);
        let mut cache = ColorCache::new(&paleta);
        let primeira = cache.index(&img);
        let segunda = cache.index(&img);
        assert_eq!(primeira, segunda);
    }
}
