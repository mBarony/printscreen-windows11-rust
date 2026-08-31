//! O cabeçalho do símbolo: nível de correção, máscara e quais módulos são
//! função.
//!
//! Separado da leitura dos dados porque é o que precisa ser lido **antes** —
//! sem saber a máscara não dá para desfazer nada, e sem saber quais módulos são
//! função não dá para saber quais são dados.

use super::grade::Grade;
use super::tabelas::{self, Nivel};

/// Lê a informação de formato do símbolo: nível de correção e máscara (0 a 7).
///
/// Há duas cópias gravadas, em lugares e ordens diferentes, justamente para o
/// caso de uma delas estar danificada. A segunda só é consultada quando a
/// primeira não casa com nenhuma das 32 palavras válidas.
pub fn ler(grade: &Grade) -> Option<(Nivel, u8)> {
    let lado = grade.lado();
    let bit = |x: usize, y: usize| u16::from(grade.escuro(x, y));

    // Cópia 1, em volta do localizador superior esquerdo. Os bits vão do
    // menos significativo (14) para o mais — a numeração do padrão é ao
    // contrário da intuitiva, e é onde todo mundo erra.
    let mut primeira = 0u16;
    for i in 0..6 {
        primeira |= bit(8, i) << i;
    }
    primeira |= bit(8, 7) << 6;
    primeira |= bit(8, 8) << 7;
    primeira |= bit(7, 8) << 8;
    for i in 9..15 {
        primeira |= bit(14 - i, 8) << i;
    }

    // Cópia 2, partida entre a faixa da direita e a de baixo.
    let mut segunda = 0u16;
    for i in 0..8 {
        segunda |= bit(lado - 1 - i, 8) << i;
    }
    for i in 8..15 {
        segunda |= bit(8, lado - 15 + i) << i;
    }

    casa(primeira).or_else(|| casa(segunda))
}

/// Acha a palavra de formato válida mais próxima da lida.
///
/// O código corrige até 3 bits e tem distância mínima 7, então comparar contra
/// as 32 palavras e exigir distância ≤ 3 é a decodificação completa — sem
/// ambiguidade possível, e mais curta que implementar a síndrome do BCH.
fn casa(lida: u16) -> Option<(Nivel, u8)> {
    let mut melhor: Option<(u32, Nivel, u8)> = None;
    for nivel in [Nivel::L, Nivel::M, Nivel::Q, Nivel::H] {
        for mascara in 0..8u8 {
            let distancia = (tabelas::formato(nivel, mascara) ^ lida).count_ones();
            if distancia <= 3 && melhor.is_none_or(|(d, ..)| distancia < d) {
                melhor = Some((distancia, nivel, mascara));
            }
        }
    }
    melhor.map(|(_, nivel, mascara)| (nivel, mascara))
}

/// `true` quando o módulo é padrão de função e não carrega dado: localizadores
/// e separadores, temporizadores, alinhamentos, módulo escuro fixo e as áreas
/// reservadas de informação de formato e de versão.
pub fn e_funcao(versao: u8, x: usize, y: usize) -> bool {
    let lado = versao as usize * 4 + 17;

    // Os três cantos, tomados como blocos 9×9 (7×7 do localizador + separador
    // + a faixa de formato). Nos cantos de cima à direita e de baixo à esquerda
    // não há faixa de formato dos dois lados, e o bloco vira 8×9.
    if x <= 8 && y <= 8 {
        return true;
    }
    if x >= lado - 8 && y <= 8 {
        return true;
    }
    if x <= 8 && y >= lado - 8 {
        return true;
    }

    // Temporizadores: a linha e a coluna 6, de ponta a ponta.
    if x == 6 || y == 6 {
        return true;
    }

    // Informação de versão: dois blocos 3×6 encostados nos localizadores de
    // cima à direita e de baixo à esquerda.
    if versao >= 7 {
        if x < 6 && (lado - 11..lado - 8).contains(&y) {
            return true;
        }
        if y < 6 && (lado - 11..lado - 8).contains(&x) {
            return true;
        }
    }

    // Alinhamentos: 5×5 em cada cruzamento, menos os três que cairiam sobre os
    // localizadores.
    let centros = tabelas::centros_alinhamento(versao);
    let n = centros.len();
    for (i, &cy) in centros.iter().enumerate() {
        for (j, &cx) in centros.iter().enumerate() {
            let sobre_localizador = (i == 0 && j == 0)
                || (i == 0 && j == n - 1)
                || (i == n - 1 && j == 0);
            if sobre_localizador {
                continue;
            }
            if x.abs_diff(cx) <= 2 && y.abs_diff(cy) <= 2 {
                return true;
            }
        }
    }

    false
}

/// A máscara `n` aplicada à posição — `true` significa inverter o módulo.
///
/// As condições do padrão são escritas sobre (i, j) com **i = linha**. Trocar
/// os dois produz um símbolo que decodifica em algumas máscaras e falha em
/// outras, que é o pior tipo de erro: parece funcionar.
pub fn mascarado(n: u8, linha: usize, coluna: usize) -> bool {
    let (i, j) = (linha, coluna);
    match n {
        0 => (i + j) % 2 == 0,
        1 => i % 2 == 0,
        2 => j % 3 == 0,
        3 => (i + j) % 3 == 0,
        4 => (i / 2 + j / 3) % 2 == 0,
        5 => (i * j) % 2 + (i * j) % 3 == 0,
        6 => ((i * j) % 2 + (i * j) % 3) % 2 == 0,
        7 => ((i + j) % 2 + (i * j) % 3) % 2 == 0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_areas_de_funcao_sobram_o_numero_certo_de_modulos() {
        // O que não é função é dado, e o padrão diz quantos bits de dados cada
        // versão tem. Se o mapa de funções estiver errado em um módulo sequer,
        // esta conta não fecha — e ela vale para as 40 versões de uma vez.
        for versao in 1..=40u8 {
            let lado = versao as usize * 4 + 17;
            let mut dados = 0usize;
            for y in 0..lado {
                for x in 0..lado {
                    if !e_funcao(versao, x, y) {
                        dados += 1;
                    }
                }
            }
            let esperado = tabelas::total_codewords(versao) * 8;
            // Sobram de 0 a 7 bits de resto, que o símbolo deixa em branco.
            let resto = dados - esperado;
            assert!(
                resto < 8,
                "versão {versao}: {dados} módulos de dados, esperado ~{esperado} (resto {resto})"
            );
        }
    }

    #[test]
    fn a_leitura_do_formato_desfaz_a_escrita() {
        for nivel in [Nivel::L, Nivel::M, Nivel::Q, Nivel::H] {
            for mascara in 0..8u8 {
                let mut g = Grade::nova(21);
                grava_formato(&mut g, nivel, mascara);
                assert_eq!(ler(&g), Some((nivel, mascara)), "{nivel:?} máscara {mascara}");
            }
        }
    }

    #[test]
    fn a_leitura_do_formato_aguenta_tres_bits_trocados() {
        let mut g = Grade::nova(21);
        grava_formato(&mut g, Nivel::Q, 3);
        for i in 0..3 {
            g.marca(8, i, !g.escuro(8, i));
        }
        assert_eq!(ler(&g), Some((Nivel::Q, 3)), "três bits ainda são corrigíveis");
    }

    #[test]
    fn a_segunda_copia_salva_quando_a_primeira_se_perde() {
        let mut g = Grade::nova(21);
        grava_formato(&mut g, Nivel::Q, 3);
        // Apaga a primeira cópia inteira: quinze módulos escuros ficam longe
        // demais de qualquer palavra válida para casar por engano.
        //
        // Quatro a seis bits trocados são outra história: aí a palavra pode
        // cair a três de distância de OUTRA palavra válida e ser lida como um
        // formato diferente, sem que nada acuse. É limite do código de 15 bits,
        // não desta implementação — a distância mínima entre as 32 palavras é
        // 7, e o teste em `tabelas` confere isso.
        for i in 0..6 {
            g.marca(8, i, true);
        }
        g.marca(8, 7, true);
        g.marca(8, 8, true);
        g.marca(7, 8, true);
        for i in 9..15 {
            g.marca(14 - i, 8, true);
        }
        assert_eq!(ler(&g), Some((Nivel::Q, 3)));
    }

    /// Grava as duas cópias, na mesma ordem em que `ler` as consome.
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
}
