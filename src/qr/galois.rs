//! GF(256) e Reed-Solomon — a correção de erro do QR.
//!
//! O campo é o do padrão: 256 elementos, polinômio primitivo `0x11D`, gerador
//! α = 2, e o polinômio gerador de código com raízes a partir de α⁰. Esse
//! último detalhe é onde as implementações divergem, e errá-lo não produz erro
//! de compilação nem exceção: produz correção que "funciona" em bloco sem erro
//! e devolve lixo assim que houver um.
//!
//! As tabelas de log e antilog são construídas por um laço de oito linhas, e
//! não transcritas: 512 bytes de constante à mão seriam 512 oportunidades de
//! errar em silêncio.
//!
//! ## Convenção dos polinômios
//!
//! Um bloco é lido como polinômio de **grau decrescente**: o byte de índice 0 é
//! o coeficiente de maior grau. É a ordem em que o símbolo grava os codewords,
//! e mantê-la aqui evita a inversão que costuma aparecer entre o codificador e
//! o decodificador de uma mesma base de código.
//!
//! A decodificação recusa em vez de adivinhar. Quando há mais erros do que a
//! correção alcança, a resposta é "não deu", nunca um bloco plausível — um
//! decodificador que devolve lixo sem avisar é pior que um que falha, porque o
//! usuário cola o lixo achando que é o conteúdo do QR.

/// Tabelas de potência e logaritmo de α, montadas uma vez.
///
/// `EXP` tem 512 posições em vez de 255 para que a soma de dois logaritmos
/// possa ser indexada direto, sem `% 255` a cada multiplicação.
struct Campo {
    exp: [u8; 512],
    log: [u8; 256],
}

impl Campo {
    fn novo() -> Campo {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x: u16 = 1;
        for (i, potencia) in exp.iter_mut().take(255).enumerate() {
            *potencia = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= 0x11D;
            }
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        Campo { exp, log }
    }
}

fn campo() -> &'static Campo {
    use std::sync::OnceLock;
    static CAMPO: OnceLock<Campo> = OnceLock::new();
    CAMPO.get_or_init(Campo::novo)
}

fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let c = campo();
    c.exp[c.log[a as usize] as usize + c.log[b as usize] as usize]
}

/// Inverso multiplicativo. `a` não pode ser zero.
fn inv(a: u8) -> u8 {
    let c = campo();
    c.exp[255 - c.log[a as usize] as usize]
}

/// α elevado a `n`.
fn alfa(n: usize) -> u8 {
    campo().exp[n % 255]
}

/// Polinômio gerador de grau `ec`, sem o coeficiente líder, em grau
/// decrescente. É o produto de (x − α⁰)(x − α¹)…(x − α^(ec−1)).
///
/// Só a codificação precisa dele: decodificar é achar as raízes, e para isso
/// bastam as síndromes. Daí ele existir apenas junto do `paridade`.
#[cfg(test)]
fn gerador(ec: usize) -> Vec<u8> {
    let mut g = vec![0u8; ec];
    g[ec - 1] = 1;
    let mut raiz = 1u8;
    for _ in 0..ec {
        for i in 0..ec {
            g[i] = mul(g[i], raiz);
            if i + 1 < ec {
                g[i] ^= g[i + 1];
            }
        }
        raiz = mul(raiz, 2);
    }
    g
}

/// Os `ec` codewords de correção de uma sequência de dados.
///
/// Só o gerador de símbolos de teste usa isto: o app decodifica, não codifica.
/// Fica aqui mesmo assim porque é o mesmo polinômio gerador da decodificação, e
/// separá-los deixaria duas fontes da mesma verdade.
#[cfg(test)]
pub fn paridade(dados: &[u8], ec: usize) -> Vec<u8> {
    let g = gerador(ec);
    let mut resto = vec![0u8; ec];
    for &b in dados {
        let fator = b ^ resto[0];
        resto.rotate_left(1);
        resto[ec - 1] = 0;
        for i in 0..ec {
            resto[i] ^= mul(g[i], fator);
        }
    }
    resto
}

/// Síndromes do bloco: o polinômio recebido avaliado em α⁰…α^(ec−1).
///
/// Todas nulas significa bloco íntegro — é o caminho de longe mais comum, e
/// sair por ele cedo poupa o resto do algoritmo.
fn sindromes(bloco: &[u8], ec: usize) -> Vec<u8> {
    (0..ec)
        .map(|j| {
            let x = alfa(j);
            bloco.iter().fold(0u8, |acc, &b| mul(acc, x) ^ b)
        })
        .collect()
}

/// Multiplica dois polinômios em grau **crescente** (índice = expoente).
fn mul_poly(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        if x == 0 {
            continue;
        }
        for (j, &y) in b.iter().enumerate() {
            r[i + j] ^= mul(x, y);
        }
    }
    r
}

/// Avalia um polinômio em grau crescente no ponto `x`.
fn avalia(p: &[u8], x: u8) -> u8 {
    p.iter().rev().fold(0u8, |acc, &c| mul(acc, x) ^ c)
}

/// Berlekamp-Massey: acha o polinômio localizador de erros Λ(x), em grau
/// crescente, a partir das síndromes.
fn localizador(sind: &[u8]) -> Vec<u8> {
    let mut lambda = vec![1u8];
    let mut anterior = vec![1u8];
    let mut atraso = 1usize;
    let mut b = 1u8;

    for n in 0..sind.len() {
        // Discrepância entre o que Λ prevê e a síndrome observada.
        let mut delta = sind[n];
        for i in 1..lambda.len() {
            if i <= n {
                delta ^= mul(lambda[i], sind[n - i]);
            }
        }

        if delta == 0 {
            atraso += 1;
            continue;
        }

        let escala = mul(delta, inv(b));
        let mut novo = lambda.clone();
        if novo.len() < anterior.len() + atraso {
            novo.resize(anterior.len() + atraso, 0);
        }
        for (i, &c) in anterior.iter().enumerate() {
            novo[i + atraso] ^= mul(escala, c);
        }

        if 2 * (lambda.len() - 1) <= n {
            anterior = lambda;
            b = delta;
            atraso = 1;
        } else {
            atraso += 1;
        }
        lambda = novo;
    }

    lambda
}

/// Corrige `bloco` no lugar. `ec` é quantos codewords finais são de correção.
///
/// Devolve `false` quando o bloco não é corrigível — e nesse caso o conteúdo de
/// `bloco` não vale nada. A conferência final das síndromes é o que separa
/// "corrigi" de "achei uma solução qualquer": Berlekamp-Massey devolve um
/// localizador mesmo quando há mais erros que o código alcança, e sem essa
/// conferência o resultado passaria por bom.
pub fn corrige(bloco: &mut [u8], ec: usize) -> bool {
    if ec == 0 || bloco.len() <= ec || bloco.len() > 255 {
        return false;
    }

    let sind = sindromes(bloco, ec);
    if sind.iter().all(|&s| s == 0) {
        return true;
    }

    let lambda = localizador(&sind);
    let erros = lambda.len() - 1;
    if erros == 0 || erros > ec / 2 {
        return false;
    }

    // Busca de Chien: as raízes de Λ apontam as posições erradas. A raiz α^-i
    // corresponde ao termo de grau i, e daí ao índice `len-1-i` do bloco.
    let mut posicoes = Vec::with_capacity(erros);
    for i in 0..bloco.len() {
        if avalia(&lambda, inv(alfa(i))) == 0 {
            posicoes.push(bloco.len() - 1 - i);
        }
    }
    if posicoes.len() != erros {
        // Menos raízes que o grau: o localizador não descreve erros reais.
        return false;
    }

    // Ω(x) = S(x)·Λ(x) mod x^ec, com S em grau crescente.
    let mut omega = mul_poly(&sind, &lambda);
    omega.truncate(ec);

    // Λ'(x) em GF(2): sobram só os termos de expoente ímpar, deslocados.
    let derivada: Vec<u8> = lambda
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, &c)| if i % 2 == 1 { c } else { 0 })
        .collect();

    for &pos in &posicoes {
        let i = bloco.len() - 1 - pos;
        let xi = alfa(i);
        let xi_inv = inv(xi);
        let den = avalia(&derivada, xi_inv);
        if den == 0 {
            return false;
        }
        // Forney com raízes a partir de α⁰: e = X·Ω(X⁻¹)/Λ'(X⁻¹).
        let magnitude = mul(xi, mul(avalia(&omega, xi_inv), inv(den)));
        bloco[pos] ^= magnitude;
    }

    sindromes(bloco, ec).iter().all(|&s| s == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_campo_fecha_em_255() {
        assert_eq!(alfa(0), 1);
        assert_eq!(alfa(1), 2);
        assert_eq!(alfa(255), 1, "α^255 = α^0");
        // O primitivo 0x11D: α^8 = α^4 + α^3 + α^2 + 1 = 0b0001_1101.
        assert_eq!(alfa(8), 0x1D);
        for a in 1u8..=255 {
            assert_eq!(mul(a, inv(a)), 1, "inverso de {a}");
        }
    }

    #[test]
    fn multiplicar_e_comutativo_e_distributivo() {
        for a in [0u8, 1, 2, 3, 87, 199, 255] {
            for b in [0u8, 1, 2, 3, 87, 199, 255] {
                assert_eq!(mul(a, b), mul(b, a));
                for c in [0u8, 5, 130] {
                    assert_eq!(mul(a, b ^ c), mul(a, b) ^ mul(a, c));
                }
            }
        }
    }

    #[test]
    fn um_bloco_sem_erro_passa_intacto() {
        let dados: Vec<u8> = (0..16).collect();
        let mut bloco = dados.clone();
        bloco.extend(paridade(&dados, 10));
        let copia = bloco.clone();

        assert!(corrige(&mut bloco, 10));
        assert_eq!(bloco, copia, "não havia o que corrigir");
    }

    #[test]
    fn corrige_ate_a_metade_dos_codewords_de_correcao() {
        let dados: Vec<u8> = (0..26).map(|i| i * 7 + 3).collect();
        let ec = 18; // corrige até 9 erros
        let mut original = dados.clone();
        original.extend(paridade(&dados, ec));

        for quantos in 1..=9 {
            let mut bloco = original.clone();
            // Espalha os erros pelo bloco, dados e paridade.
            for k in 0..quantos {
                let pos = (k * 4 + 1) % bloco.len();
                bloco[pos] ^= 0xA5 ^ (k as u8);
            }
            assert!(corrige(&mut bloco, ec), "{quantos} erros deveriam caber");
            assert_eq!(bloco, original, "{quantos} erros corrigidos para o valor certo");
        }
    }

    #[test]
    fn recusa_quando_ha_erro_demais() {
        let dados: Vec<u8> = (0u8..26).map(|i| i.wrapping_mul(11).wrapping_add(5)).collect();
        let ec = 10; // corrige até 5
        let mut bloco = dados.clone();
        bloco.extend(paridade(&dados, ec));
        for k in 0..9 {
            bloco[k * 3] ^= 0x5C;
        }
        assert!(!corrige(&mut bloco, ec), "9 erros com ec=10 não têm conserto");
    }

    #[test]
    fn recusa_bloco_degenerado() {
        let mut vazio: Vec<u8> = vec![1, 2, 3];
        assert!(!corrige(&mut vazio, 0), "sem correção não há o que conferir");
        assert!(!corrige(&mut vazio, 3), "bloco só de paridade não tem dado");
    }
}
