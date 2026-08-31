//! As constantes do ISO/IEC 18004 — só os números do padrão, nada de lógica.
//!
//! Estão num módulo só porque são um tipo de código diferente do resto: não têm
//! o que raciocinar, têm o que conferir. Erro aqui não dá erro de compilação
//! nem exceção — dá texto errado, em silêncio.
//!
//! Por isso a regra deste arquivo é **derivar em vez de transcrever** sempre que
//! o padrão permitir. As palavras de informação de formato e de versão saem do
//! BCH que as define; as posições dos padrões de alinhamento saem da progressão
//! que gera a tabela E.1; o total de codewords sai da contagem de módulos. Cada
//! uma dessas é um laço de dez linhas conferível contra meia dúzia de valores
//! conhecidos, em vez de dezenas de números soltos.
//!
//! Sobra transcrito o que o padrão não deriva: a Tabela 9, e mesmo dela só duas
//! linhas por nível — quantos codewords de correção cada bloco tem e quantos
//! blocos existem. A divisão em grupos vem daí por aritmética.

/// Nível de correção de erro do símbolo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nivel {
    L,
    M,
    Q,
    H,
}

impl Nivel {
    /// Os dois bits como o nível aparece na informação de formato. A ordem não é
    /// a intuitiva: M é 00 e L é 01.
    pub fn bits(self) -> u8 {
        match self {
            Nivel::L => 0b01,
            Nivel::M => 0b00,
            Nivel::Q => 0b11,
            Nivel::H => 0b10,
        }
    }

    /// Índice na ordem L, M, Q, H em que as tabelas do padrão são publicadas.
    pub fn indice(self) -> usize {
        match self {
            Nivel::L => 0,
            Nivel::M => 1,
            Nivel::Q => 2,
            Nivel::H => 3,
        }
    }
}

/// Codewords de correção por bloco, por nível (L, M, Q, H) e versão 1 a 40.
/// Índice 0 de cada linha é buraco: versão 0 não existe.
const EC_POR_BLOCO: [[u8; 41]; 4] = [
    [0, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30,
     30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
    [0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28,
     28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
    [0, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30,
     30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
    [0, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30,
     30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
];

/// Quantos blocos de correção o símbolo tem, mesma indexação.
const NUM_BLOCOS: [[u8; 41]; 4] = [
    [0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13,
     14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
    [0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23,
     25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
    [0, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29,
     34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
    [0, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35,
     37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
];

/// Como os codewords de um símbolo se dividem em blocos.
///
/// O padrão divide os dados em até dois grupos, e os blocos do segundo grupo
/// têm exatamente um codeword de dados a mais que os do primeiro. Por isso a
/// divisão não precisa ser transcrita: ela é a divisão inteira do total pelo
/// número de blocos, com o resto distribuído no fim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blocos {
    pub ec_por_bloco: usize,
    pub blocos_g1: usize,
    pub dados_g1: usize,
    pub blocos_g2: usize,
    pub dados_g2: usize,
}

impl Blocos {
    pub fn total_blocos(&self) -> usize {
        self.blocos_g1 + self.blocos_g2
    }

    pub fn total_codewords(&self) -> usize {
        self.blocos_g1 * (self.dados_g1 + self.ec_por_bloco)
            + self.blocos_g2 * (self.dados_g2 + self.ec_por_bloco)
    }

    pub fn total_dados(&self) -> usize {
        self.blocos_g1 * self.dados_g1 + self.blocos_g2 * self.dados_g2
    }
}

/// Estrutura de blocos de `versao` (1 a 40) no nível dado.
pub fn blocos(versao: u8, nivel: Nivel) -> Blocos {
    let v = versao as usize;
    let ec_por_bloco = EC_POR_BLOCO[nivel.indice()][v] as usize;
    let n = NUM_BLOCOS[nivel.indice()][v] as usize;
    let total = total_codewords(versao);

    // Blocos curtos primeiro, longos depois — é a ordem do padrão, e é o que
    // faz a desintercalação funcionar.
    let curto = total / n;
    let longos = total % n;
    Blocos {
        ec_por_bloco,
        blocos_g1: n - longos,
        dados_g1: curto - ec_por_bloco,
        blocos_g2: longos,
        dados_g2: curto - ec_por_bloco + 1,
    }
}

/// Total de codewords (dados + correção) de uma versão.
///
/// Contado, não transcrito: são todos os módulos menos os de função. Cada
/// subtração corresponde a um padrão do símbolo, e é por isso que este laço é
/// mais confiável que quarenta números.
pub fn total_codewords(versao: u8) -> usize {
    let v = versao as usize;
    let lado = v * 4 + 17;
    let mut modulos = lado * lado;
    modulos -= 8 * 8 * 3; // três localizadores com separador e área de formato
    modulos -= 15 * 2 + 1; // as duas cópias da informação de formato e o módulo escuro
    modulos -= (lado - 16) * 2; // temporizadores, fora o que já entrou nos localizadores

    if v >= 2 {
        let n = centros_alinhamento(versao).len();
        modulos -= (n - 1) * (n - 1) * 25; // alinhamentos que não tocam o temporizador
        modulos -= (n - 2) * 2 * 20; // os que tocam, e por isso contam menos
        if v >= 7 {
            modulos -= 6 * 3 * 2; // as duas cópias da informação de versão
        }
    }
    modulos / 8
}

/// Coordenadas dos centros dos padrões de alinhamento da versão.
///
/// Os centros são o produto cartesiano desta lista consigo mesma, menos as três
/// combinações que cairiam sobre os padrões localizadores. Vazia na versão 1,
/// que não tem alinhamento.
///
/// A tabela E.1 do padrão é reproduzida por uma progressão: o primeiro centro é
/// sempre 6, o último é sempre `4v + 10`, e os do meio são igualmente espaçados
/// por um passo par. A versão 32 é a única exceção documentada — a fórmula daria
/// 28 e o padrão manda 26.
pub fn centros_alinhamento(versao: u8) -> Vec<usize> {
    let v = versao as usize;
    if v < 2 {
        return Vec::new();
    }
    let n = v / 7 + 2;
    let passo = if v == 32 { 26 } else { (v * 4 + n * 2 + 1) / (n * 2 - 2) * 2 };

    let ultimo = v * 4 + 10;
    let mut centros = vec![6usize; n];
    for (i, centro) in centros.iter_mut().enumerate().skip(1) {
        *centro = ultimo - (n - 1 - i) * passo;
    }
    centros
}

/// Máscara XOR aplicada à informação de formato antes de gravar, para que o
/// padrão todo-zeros não vire uma faixa de módulos claros.
const MASCARA_FORMATO: u16 = 0b101_0100_0001_0010;

/// Os 15 bits de informação de formato, já com a máscara XOR aplicada — que é
/// como eles aparecem gravados no símbolo.
pub fn formato(nivel: Nivel, mascara: u8) -> u16 {
    let dados = ((nivel.bits() as u32) << 3) | (mascara as u32 & 0b111);
    let palavra = (dados << 10) | resto_bch(dados, GERADOR_FORMATO);
    (palavra as u16) ^ MASCARA_FORMATO
}

/// Os 18 bits de informação de versão, gravados a partir da versão 7.
///
/// Não leva máscara XOR: ao contrário da informação de formato, ela nunca é
/// toda zeros, porque a versão mínima que a usa já é 7.
///
/// Só o gerador de teste escreve isso. A decodificação tira a versão do lado do
/// símbolo, que o detector já mediu, e conferir contra estes 18 bits só ajudaria
/// se o lado estivesse errado — e um lado errado erra a amostragem inteira, não
/// só a versão.
#[cfg(test)]
pub fn info_versao(versao: u8) -> Option<u32> {
    if versao < 7 {
        return None;
    }
    let v = versao as u32;
    Some((v << 12) | resto_bch(v, GERADOR_VERSAO))
}

/// Gerador do BCH(15,5) da informação de formato: x¹⁰+x⁸+x⁵+x⁴+x²+x+1.
const GERADOR_FORMATO: u32 = 0b101_0011_0111;
/// Gerador do BCH(18,6) da informação de versão: x¹²+x¹¹+x¹⁰+x⁹+x⁸+x⁵+x²+1.
#[cfg(test)]
const GERADOR_VERSAO: u32 = 0b1_1111_0010_0101;

/// Resto da divisão de `dados` deslocado pelo grau do gerador — são os bits de
/// correção do código BCH.
fn resto_bch(dados: u32, gerador: u32) -> u32 {
    let grau = grau_de(gerador);
    let mut resto = dados << grau;
    while resto != 0 && grau_de(resto) >= grau {
        resto ^= gerador << (grau_de(resto) - grau);
    }
    resto
}

/// Expoente do termo de maior grau. Só faz sentido para `x` diferente de zero.
fn grau_de(x: u32) -> u32 {
    31 - x.leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIVEIS: [Nivel; 4] = [Nivel::L, Nivel::M, Nivel::Q, Nivel::H];

    #[test]
    fn os_blocos_fecham_o_total_de_codewords() {
        for versao in 1..=40u8 {
            for nivel in NIVEIS {
                let b = blocos(versao, nivel);
                assert_eq!(
                    b.total_codewords(),
                    total_codewords(versao),
                    "versão {versao} nível {nivel:?}"
                );
                assert_eq!(b.total_blocos(), NUM_BLOCOS[nivel.indice()][versao as usize] as usize);
                assert!(b.dados_g1 > 0, "versão {versao} nível {nivel:?} sem dados");
                if b.blocos_g2 > 0 {
                    assert_eq!(b.dados_g2, b.dados_g1 + 1, "o grupo 2 tem um codeword a mais");
                }
            }
        }
    }

    #[test]
    fn o_total_de_codewords_bate_com_os_valores_conhecidos() {
        assert_eq!(total_codewords(1), 26);
        assert_eq!(total_codewords(2), 44);
        assert_eq!(total_codewords(3), 70);
        assert_eq!(total_codewords(6), 172);
        assert_eq!(total_codewords(7), 196);
        assert_eq!(total_codewords(10), 346);
        assert_eq!(total_codewords(40), 3706);
    }

    #[test]
    fn a_capacidade_de_dados_da_versao_1_e_a_publicada() {
        assert_eq!(blocos(1, Nivel::L).total_dados(), 19);
        assert_eq!(blocos(1, Nivel::M).total_dados(), 16);
        assert_eq!(blocos(1, Nivel::Q).total_dados(), 13);
        assert_eq!(blocos(1, Nivel::H).total_dados(), 9);
    }

    #[test]
    fn os_centros_de_alinhamento_batem_com_a_tabela_e1() {
        assert!(centros_alinhamento(1).is_empty());
        assert_eq!(centros_alinhamento(2), vec![6, 18]);
        assert_eq!(centros_alinhamento(7), vec![6, 22, 38]);
        assert_eq!(centros_alinhamento(10), vec![6, 28, 50]);
        assert_eq!(centros_alinhamento(32), vec![6, 34, 60, 86, 112, 138]);
        assert_eq!(centros_alinhamento(40), vec![6, 30, 58, 86, 114, 142, 170]);

        for versao in 2..=40u8 {
            let c = centros_alinhamento(versao);
            assert_eq!(c[0], 6, "versão {versao} começa em 6");
            assert_eq!(*c.last().unwrap(), versao as usize * 4 + 10, "versão {versao}");
            for par in c.windows(2) {
                assert!(par[1] > par[0], "versão {versao} fora de ordem");
            }
        }
    }

    #[test]
    fn a_informacao_de_formato_bate_com_a_tabela_publicada() {
        // Os quatro primeiros valores da tabela do padrão, um por nível com a
        // máscara 0 — se estes fecham, o BCH e a máscara XOR estão certos.
        assert_eq!(formato(Nivel::L, 0), 0b111011111000100);
        assert_eq!(formato(Nivel::M, 0), 0b101010000010010);
        assert_eq!(formato(Nivel::Q, 0), 0b011010101011111);
        assert_eq!(formato(Nivel::H, 0), 0b001011010001001);
        assert_eq!(formato(Nivel::M, 5), 0b100000011001110);
        assert_eq!(formato(Nivel::H, 6), 0b000110100001100);
        assert_eq!(formato(Nivel::H, 7), 0b000100000111011);

        // As 32 palavras têm de ser distintas e distantes entre si: o código
        // corrige 3 bits, logo a distância mínima é 7.
        let todas: Vec<u16> =
            NIVEIS.iter().flat_map(|&n| (0..8u8).map(move |m| formato(n, m))).collect();
        for (i, a) in todas.iter().enumerate() {
            for b in &todas[i + 1..] {
                assert!((a ^ b).count_ones() >= 7, "{a:015b} e {b:015b} perto demais");
            }
        }
    }

    #[test]
    fn a_informacao_de_versao_bate_com_a_tabela_publicada() {
        assert_eq!(info_versao(6), None, "só existe a partir da 7");
        assert_eq!(info_versao(7), Some(0b000111110010010100));
        assert_eq!(info_versao(8), Some(0b001000010110111100));
        assert_eq!(info_versao(40), Some(0b101000110001101001));

        let todas: Vec<u32> = (7..=40u8).map(|v| info_versao(v).unwrap()).collect();
        for (i, a) in todas.iter().enumerate() {
            for b in &todas[i + 1..] {
                assert!((a ^ b).count_ones() >= 8, "{a:018b} e {b:018b} perto demais");
            }
        }
    }
}
