//! Da grade ao texto: zigue-zague, blocos, correção e segmentos.
//!
//! É a metade do problema que não depende de imagem nenhuma — entra uma
//! `Grade`, sai uma `String` —, e é por isso que ela é testável de verdade.

use super::formato;
use super::galois;
use super::grade::Grade;
use super::tabelas::{self, Blocos, Nivel};

/// Decodifica o conteúdo do símbolo.
///
/// `None` em qualquer tropeço: blocos que a correção não fecha, segmento com
/// contador maior que o que resta, modo que não sabemos ler. Recusar é a
/// resposta certa — quem chama tem o OCR como alternativa.
pub fn texto(grade: &Grade, versao: u8, nivel: Nivel, mascara: u8) -> Option<String> {
    let fluxo = codewords(grade, versao, mascara);
    let dados = desintercala(&fluxo, &tabelas::blocos(versao, nivel))?;
    segmentos(&dados, versao)
}

/// Percorre a área de dados em zigue-zague e devolve os codewords, já sem a
/// máscara.
///
/// O caminho é o do padrão: colunas de dois módulos, da direita para a
/// esquerda, alternando subida e descida, pulando a coluna 6 — que é o
/// temporizador vertical e não desloca o par, apenas some.
fn codewords(grade: &Grade, versao: u8, mascara: u8) -> Vec<u8> {
    let lado = grade.lado();
    let mut saida = Vec::with_capacity(tabelas::total_codewords(versao));
    let mut atual = 0u8;
    let mut cheios = 0u32;

    let mut coluna = lado - 1;
    loop {
        if coluna == 6 {
            coluna = 5;
        }
        for passo in 0..lado {
            for j in 0..2 {
                let x = coluna - j;
                // O sentido alterna a cada par de colunas, e o pulo da coluna
                // 6 não pode inverter a alternância — daí a conta ser sobre a
                // coluna e não sobre um contador de pares.
                let subindo = (coluna + 1) & 2 == 0;
                let y = if subindo { lado - 1 - passo } else { passo };

                if formato::e_funcao(versao, x, y) {
                    continue;
                }
                let mut bit = grade.escuro(x, y);
                if formato::mascarado(mascara, y, x) {
                    bit = !bit;
                }
                atual = (atual << 1) | u8::from(bit);
                cheios += 1;
                if cheios == 8 {
                    saida.push(atual);
                    atual = 0;
                    cheios = 0;
                }
            }
        }
        if coluna < 2 {
            break;
        }
        coluna -= 2;
    }
    saida
}

/// Desfaz a intercalação dos blocos, corrige cada um e devolve os dados.
///
/// O símbolo grava os blocos intercalados de propósito: um arranhão no papel
/// vira um erro isolado em cada bloco, em vez de destruir um bloco inteiro. Por
/// isso a correção só pode acontecer depois de separar.
fn desintercala(fluxo: &[u8], b: &Blocos) -> Option<Vec<u8>> {
    if fluxo.len() < b.total_codewords() {
        return None;
    }
    let n = b.total_blocos();
    let tamanho = |i: usize| if i < b.blocos_g1 { b.dados_g1 } else { b.dados_g2 };

    let mut blocos: Vec<Vec<u8>> = (0..n).map(|i| Vec::with_capacity(tamanho(i))).collect();
    let mut k = 0usize;
    for coluna in 0..b.dados_g1.max(b.dados_g2) {
        for (i, bloco) in blocos.iter_mut().enumerate() {
            if coluna < tamanho(i) {
                bloco.push(fluxo[k]);
                k += 1;
            }
        }
    }
    // A paridade vem depois, e todos os blocos têm a mesma quantidade dela.
    let mut paridade: Vec<Vec<u8>> = vec![Vec::with_capacity(b.ec_por_bloco); n];
    for _ in 0..b.ec_por_bloco {
        for p in paridade.iter_mut() {
            p.push(fluxo[k]);
            k += 1;
        }
    }

    let mut dados = Vec::with_capacity(b.total_dados());
    for (i, mut bloco) in blocos.into_iter().enumerate() {
        let quantos = bloco.len();
        bloco.extend_from_slice(&paridade[i]);
        if !galois::corrige(&mut bloco, b.ec_por_bloco) {
            return None;
        }
        dados.extend_from_slice(&bloco[..quantos]);
    }
    Some(dados)
}

/// Leitor de bits sobre os codewords, do mais significativo para o menos.
struct Bits<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn novo(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn restantes(&self) -> usize {
        self.bytes.len() * 8 - self.pos
    }

    /// Lê `n` bits. `None` quando não há tantos — que é como um contador
    /// corrompido se manifesta, e recusar é melhor que devolver meia string.
    fn le(&mut self, n: usize) -> Option<u32> {
        if n > 32 || self.restantes() < n {
            return None;
        }
        let mut v = 0u32;
        for _ in 0..n {
            let byte = self.bytes[self.pos / 8];
            let bit = (byte >> (7 - self.pos % 8)) & 1;
            v = (v << 1) | bit as u32;
            self.pos += 1;
        }
        Some(v)
    }
}

/// Os 45 caracteres do modo alfanumérico, na ordem em que o padrão os numera.
const ALFANUMERICO: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

/// Quantos bits tem o contador de caracteres, por modo e faixa de versão.
fn bits_do_contador(modo: u32, versao: u8) -> Option<usize> {
    let faixa = match versao {
        1..=9 => 0,
        10..=26 => 1,
        27..=40 => 2,
        _ => return None,
    };
    let tabela: [usize; 3] = match modo {
        0b0001 => [10, 12, 14],
        0b0010 => [9, 11, 13],
        0b0100 => [8, 16, 16],
        0b1000 => [8, 10, 12],
        _ => return None,
    };
    Some(tabela[faixa])
}

/// Percorre os segmentos até o terminador e monta o texto.
fn segmentos(dados: &[u8], versao: u8) -> Option<String> {
    let mut bits = Bits::novo(dados);
    let mut saida = String::new();

    loop {
        // Menos de 4 bits sobrando é o preenchimento do fim, não um segmento.
        if bits.restantes() < 4 {
            break;
        }
        let modo = bits.le(4)?;
        match modo {
            0b0000 => break, // terminador
            0b0001 => numerico(&mut bits, versao, &mut saida)?,
            0b0010 => alfanumerico(&mut bits, versao, &mut saida)?,
            0b0100 => byte(&mut bits, versao, &mut saida)?,
            // ECI: o designador diz a codificação, e nós já tentamos UTF-8
            // antes de Latin-1 — ler e seguir cobre o caso comum.
            0b0111 => {
                let primeiro = bits.le(8)?;
                // O designador tem 1, 2 ou 3 bytes, e quem diz qual é o prefixo
                // do primeiro: `0` sozinho, `10`, ou `110`. Ler menos do que
                // ele ocupa não perde só o designador — desalinha todo o resto
                // do fluxo, e o segmento seguinte vira lixo.
                let extra = if primeiro & 0b1000_0000 == 0 {
                    0
                } else if primeiro & 0b0100_0000 == 0 {
                    8
                } else if primeiro & 0b0010_0000 == 0 {
                    16
                } else {
                    return None;
                };
                if extra > 0 {
                    bits.le(extra)?;
                }
            }
            // Anexo estruturado: o símbolo é pedaço de uma mensagem maior. O
            // cabeçalho é descartado e o pedaço é lido — meia mensagem é mais
            // útil que nenhuma, e o usuário vê o que veio.
            0b0011 => {
                bits.le(16)?;
            }
            // FNC1 marca dado de aplicação (GS1); não muda como se lê.
            0b0101 => {}
            0b1001 => {
                bits.le(8)?;
            }
            // Kanji precisaria da tabela Shift-JIS inteira para um caso que
            // não aparece numa captura de tela. Recusar é honesto; devolver os
            // outros segmentos e engolir este não seria.
            _ => return None,
        }
    }

    (!saida.is_empty()).then_some(saida)
}

fn numerico(bits: &mut Bits, versao: u8, saida: &mut String) -> Option<()> {
    let mut restam = bits.le(bits_do_contador(0b0001, versao)?)? as usize;
    while restam >= 3 {
        let v = bits.le(10)?;
        if v > 999 {
            return None;
        }
        saida.push_str(&format!("{v:03}"));
        restam -= 3;
    }
    match restam {
        0 => {}
        1 => {
            let v = bits.le(4)?;
            if v > 9 {
                return None;
            }
            saida.push_str(&format!("{v}"));
        }
        _ => {
            let v = bits.le(7)?;
            if v > 99 {
                return None;
            }
            saida.push_str(&format!("{v:02}"));
        }
    }
    Some(())
}

fn alfanumerico(bits: &mut Bits, versao: u8, saida: &mut String) -> Option<()> {
    let mut restam = bits.le(bits_do_contador(0b0010, versao)?)? as usize;
    while restam >= 2 {
        let v = bits.le(11)? as usize;
        if v >= 45 * 45 {
            return None;
        }
        saida.push(ALFANUMERICO[v / 45] as char);
        saida.push(ALFANUMERICO[v % 45] as char);
        restam -= 2;
    }
    if restam == 1 {
        let v = bits.le(6)? as usize;
        if v >= 45 {
            return None;
        }
        saida.push(ALFANUMERICO[v] as char);
    }
    Some(())
}

/// Modo byte. O padrão manda ISO-8859-1, mas o mundo grava UTF-8 sem avisar —
/// tentar UTF-8 primeiro e cair para Latin-1 é o que os leitores de verdade
/// fazem, e é a diferença entre ler um endereço com acento e ler ruído.
fn byte(bits: &mut Bits, versao: u8, saida: &mut String) -> Option<()> {
    let quantos = bits.le(bits_do_contador(0b0100, versao)?)? as usize;
    let mut bytes = Vec::with_capacity(quantos);
    for _ in 0..quantos {
        bytes.push(bits.le(8)? as u8);
    }
    match String::from_utf8(bytes) {
        Ok(texto) => saida.push_str(&texto),
        Err(erro) => saida.extend(erro.into_bytes().into_iter().map(|b| b as char)),
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qr::gera;

    fn ida_e_volta(texto: &str, versao: u8, nivel: Nivel, mascara: u8) {
        let g = gera::simbolo(texto, versao, nivel, mascara);
        let lido = crate::qr::conteudo(&g);
        assert_eq!(lido.as_deref(), Some(texto), "versão {versao} {nivel:?} máscara {mascara}");
    }

    #[test]
    fn le_o_que_o_gerador_escreve() {
        for mascara in 0..8u8 {
            ida_e_volta("HELLO WORLD", 1, Nivel::M, mascara);
        }
        for nivel in [Nivel::L, Nivel::M, Nivel::Q, Nivel::H] {
            ida_e_volta("https://exemplo.com.br/x", 3, nivel, 4);
        }
    }

    #[test]
    fn le_simbolos_de_varias_versoes() {
        // Versões com uma e com duas faixas de contador, e com blocos de
        // grupos diferentes — é onde a desintercalação erra se estiver errada.
        ida_e_volta("RustShot", 1, Nivel::L, 0);
        ida_e_volta(&"a".repeat(60), 5, Nivel::M, 2);
        ida_e_volta(&"b".repeat(200), 10, Nivel::M, 5);
        ida_e_volta(&"c".repeat(250), 15, Nivel::Q, 7);
    }

    #[test]
    fn le_texto_com_acento_em_utf8() {
        ida_e_volta("ação — número 5", 4, Nivel::M, 3);
    }

    #[test]
    fn a_correcao_de_erro_salva_modulos_estragados() {
        let texto = "https://exemplo.com/abc";
        let mut g = gera::simbolo(texto, 4, Nivel::H, 2);
        // Nível H corrige ~30%: apagar um quadrado de 6×6 no meio da área de
        // dados tem de continuar legível.
        for y in 10..16 {
            for x in 10..16 {
                g.marca(x, y, false);
            }
        }
        assert_eq!(crate::qr::conteudo(&g).as_deref(), Some(texto));
    }

    #[test]
    fn recusa_grade_de_lixo() {
        let mut g = Grade::nova(21);
        for y in 0..21 {
            for x in 0..21 {
                g.marca(x, y, (x * 7 + y * 3) % 5 < 2);
            }
        }
        assert!(crate::qr::conteudo(&g).is_none());
    }
}
