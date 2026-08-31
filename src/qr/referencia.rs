//! Símbolos de referência vindos de fora — o teste que o resto não consegue ser.
//!
//! Todo o resto da suíte prova consistência interna: o `gera` escreve, o `dados`
//! lê, e os dois concordam. Se ambos partirem da mesma premissa errada — a
//! ordem de leitura invertida, a máscara com linha e coluna trocadas, o
//! expoente inicial do Reed-Solomon deslocado — os testes passam e o
//! decodificador falha no primeiro QR de verdade. É o alerta que estava escrito
//! no item de backlog desta funcionalidade, e é este arquivo que o responde.
//!
//! As cinco matrizes abaixo não saíram daqui. Três são as figuras do ISO/IEC
//! 18004:2015(E) — o Anexo I.2 (pág. 94), a Figura 1 (pág. 7) e a Figura 29
//! (pág. 60) —, transcritas nos arquivos de referência da biblioteca segno; duas
//! são os literais de teste do zxing, que é outra implementação, de outro
//! projeto. Cada uma traz junto o conteúdo que o padrão publica.
//!
//! Entre elas cobrem o que dá para errar em silêncio: os três modos de segmento
//! (numérico, alfanumérico e byte), as duas famílias de máscara, um símbolo de
//! versão 4 com **dois blocos** intercalados e padrão de alinhamento, e um cujo
//! formato desmascarado é exatamente zero — o caso que derruba quem trata
//! "formato igual a zero" como "não achei o formato".

use super::grade::Grade;
use super::tabelas::Nivel;

pub struct Referencia {
    pub nome: &'static str,
    pub nivel: Nivel,
    pub mascara: u8,
    pub conteudo: &'static str,
    /// `1` é módulo escuro, `0` claro. Linha 0 é o topo, sem zona de silêncio.
    pub linhas: &'static [&'static str],
}

impl Referencia {
    pub fn grade(&self) -> Grade {
        let lado = self.linhas.len();
        let mut g = Grade::nova(lado);
        for (y, linha) in self.linhas.iter().enumerate() {
            assert_eq!(linha.len(), lado, "{}: linha {y} com largura errada", self.nome);
            for (x, c) in linha.bytes().enumerate() {
                g.marca(x, y, c == b'1');
            }
        }
        g
    }
}

pub const REFERENCIAS: &[Referencia] = &[
    // ISO/IEC 18004:2015(E), Anexo I.2 — o exemplo canônico do padrão.
    Referencia {
        nome: "iso-18004-anexo-I.2",
        nivel: Nivel::M,
        mascara: 2,
        conteudo: "01234567",
        linhas: &[
            "111111100101101111111",
            "100000100111101000001",
            "101110101000001011101",
            "101110101100001011101",
            "101110101011101011101",
            "100000101000101000001",
            "111111101010101111111",
            "000000001001100000000",
            "101111100100101111100",
            "000101011010100101100",
            "001000110101010011111",
            "000010000100000111100",
            "000111111001010010000",
            "000000001011111001100",
            "111111100110101100000",
            "100000101011111000101",
            "101110101000100101100",
            "101110101100100100000",
            "101110101011010010100",
            "100000100000000110110",
            "111111101111010010100",
        ],
    },
    // ISO/IEC 18004:2015(E), Figura 1 — modo byte.
    Referencia {
        nome: "iso-18004-figura-1",
        nivel: Nivel::M,
        mascara: 5,
        conteudo: "QR Code Symbol",
        linhas: &[
            "111111100001101111111",
            "100000101001101000001",
            "101110101110101011101",
            "101110101010001011101",
            "101110100000101011101",
            "100000100010101000001",
            "111111101010101111111",
            "000000001100100000000",
            "100000101111011001110",
            "100010001110001000111",
            "011101111001100100010",
            "110100001011010100110",
            "011111111110001011011",
            "000000001000000010110",
            "111111100111111000110",
            "100000100010011011100",
            "101110100000111000111",
            "101110100100001010100",
            "101110100100101010011",
            "100000100001110111100",
            "111111101011001010010",
        ],
    },
    // ISO/IEC 18004:2015(E), Figura 29 — versão 4, dois blocos intercalados e
    // padrão de alinhamento. É o único que exercita a desintercalação.
    Referencia {
        nome: "iso-18004-figura-29",
        nivel: Nivel::M,
        mascara: 4,
        conteudo: "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        linhas: &[
            "111111101100110010010010101111111",
            "100000100010111010111000101000001",
            "101110100000001101101100001011101",
            "101110101010000111000110001011101",
            "101110101101100011010010001011101",
            "100000101100010100001101101000001",
            "111111101010101010101010101111111",
            "000000001010000000011100100000000",
            "100010111100001100100011011111001",
            "100101000111001001000110000101100",
            "010001100011111010101000011011001",
            "101101011101010010000010010000000",
            "001111110011010110010011101001100",
            "011001000101001000111100110101001",
            "101001111001111101001111000110111",
            "100100010001000111100101111100000",
            "110010111101110000011110111111100",
            "010000010111100010001000010000111",
            "111111111111010101000110010001111",
            "001100010000000111100101010101110",
            "111101111011101000111001010010001",
            "100110000101001010010111000100001",
            "000110111110111010010001011001000",
            "001011010011101000011111011101111",
            "111011111000010111001001111110000",
            "000000001110110011111100100010100",
            "111111101000110100101000101010011",
            "100000100001010010001011100010000",
            "101110101111011010000010111111100",
            "101110100000111000111100000000101",
            "101110100101010100001000010110100",
            "100000100010110111000110101001001",
            "111111101101101011010000111100011",
        ],
    },
    // zxing, EncoderTestCase.testEncode() — outra implementação, outro projeto.
    // Nível H, a correção mais forte.
    Referencia {
        nome: "zxing-ABCDEF-1H",
        nivel: Nivel::H,
        mascara: 0,
        conteudo: "ABCDEF",
        linhas: &[
            "111111101111001111111",
            "100000100111001000001",
            "101110100101101011101",
            "101110101110101011101",
            "101110100111001011101",
            "100000100100001000001",
            "111111101010101111111",
            "000000000010100000000",
            "001011101100110001001",
            "101110010001010000000",
            "001100101000101010110",
            "110101011101010000010",
            "001101111000101011110",
            "000000001001110101000",
            "111111100010101100001",
            "100000101111010111101",
            "101110101011010100001",
            "101110100110111101010",
            "101110101000101011101",
            "100000100110110100011",
            "111111100000000010101",
        ],
    },
    // zxing, EncoderTestCase.testEncodeShiftjisNumeric(). O formato deste
    // símbolo, desmascarado, é zero — pega quem confunde "formato zero" com
    // "não achei o formato".
    Referencia {
        nome: "zxing-0123-1M",
        nivel: Nivel::M,
        mascara: 0,
        conteudo: "0123",
        linhas: &[
            "111111100000101111111",
            "100000101101001000001",
            "101110100110001011101",
            "101110100010001011101",
            "101110101011101011101",
            "100000100101001000001",
            "111111101010101111111",
            "000000000110000000000",
            "101010100000100010010",
            "000000011011010101010",
            "010101111001011101010",
            "011100000011110111010",
            "000111111111011100101",
            "000000001100001000110",
            "111111100100100010001",
            "100000100100001000100",
            "101110101100101010101",
            "101110100111010101010",
            "101110101011011101101",
            "100000100011110111000",
            "111111101011011101101",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qr::{formato, gera};

    #[test]
    fn o_formato_lido_e_o_publicado() {
        for r in REFERENCIAS {
            let g = r.grade();
            assert_eq!(
                formato::ler(&g),
                Some((r.nivel, r.mascara)),
                "{}: nível e máscara vêm do próprio símbolo",
                r.nome
            );
        }
    }

    #[test]
    fn decodifica_os_simbolos_de_referencia() {
        for r in REFERENCIAS {
            let g = r.grade();
            assert_eq!(
                crate::qr::conteudo(&g).as_deref(),
                Some(r.conteudo),
                "{} não decodificou",
                r.nome
            );
        }
    }

    /// O caminho inteiro, imagem inclusive: é o que o app roda de verdade.
    #[test]
    fn decodifica_os_de_referencia_desenhados_como_imagem() {
        for r in REFERENCIAS {
            let img = gera::imagem(&r.grade(), 4, 4);
            assert_eq!(crate::qr::decode(&img).as_deref(), Some(r.conteudo), "{}", r.nome);
        }
    }
}
