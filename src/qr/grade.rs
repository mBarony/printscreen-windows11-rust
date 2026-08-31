//! A matriz de módulos de um símbolo QR — quadrada, um bit por módulo.
//!
//! É a fronteira entre as duas metades do problema: a detecção termina
//! produzindo uma `Grade`, e a leitura começa consumindo uma. Separá-las
//! assim é o que permite testar a leitura sem imagem nenhuma.

/// Módulos de um símbolo, `true` = escuro.
#[derive(Clone, PartialEq, Eq)]
pub struct Grade {
    lado: usize,
    modulos: Vec<bool>,
}

impl Grade {
    /// Grade toda clara de `lado × lado`.
    pub fn nova(lado: usize) -> Self {
        Self { lado, modulos: vec![false; lado * lado] }
    }

    pub fn lado(&self) -> usize {
        self.lado
    }

    /// Versão do símbolo, de 1 a 40. `None` se o lado não for de um QR:
    /// todo símbolo tem `17 + 4 × versão` módulos de lado.
    pub fn versao(&self) -> Option<u8> {
        if self.lado < 21 || self.lado > 177 || !(self.lado - 17).is_multiple_of(4) {
            return None;
        }
        Some(((self.lado - 17) / 4) as u8)
    }

    /// Fora da grade conta como claro: simplifica quem varre vizinhança sem
    /// precisar checar borda a cada passo.
    pub fn escuro(&self, x: usize, y: usize) -> bool {
        if x >= self.lado || y >= self.lado {
            return false;
        }
        self.modulos[y * self.lado + x]
    }

    pub fn marca(&mut self, x: usize, y: usize, escuro: bool) {
        if x < self.lado && y < self.lado {
            self.modulos[y * self.lado + x] = escuro;
        }
    }

    /// Espelha na diagonal principal. Um símbolo lido de trás (adesivo visto
    /// pelo verso, captura espelhada) decodifica depois desta transposição, e
    /// tentá-la é mais barato que recusar.
    pub fn transposta(&self) -> Grade {
        let mut fora = Grade::nova(self.lado);
        for y in 0..self.lado {
            for x in 0..self.lado {
                fora.marca(y, x, self.escuro(x, y));
            }
        }
        fora
    }
}

/// Desenha a grade, `#` escuro e `.` claro.
///
/// Vale o custo de escrever: quando um teste falha, a diferença entre duas
/// grades é a única coisa que diz o que aconteceu, e um `Vec<bool>` de 31 mil
/// posições não diz nada.
impl std::fmt::Debug for Grade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Grade {}x{}", self.lado, self.lado)?;
        for y in 0..self.lado {
            for x in 0..self.lado {
                f.write_str(if self.escuro(x, y) { "#" } else { "." })?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_lado_determina_a_versao() {
        assert_eq!(Grade::nova(21).versao(), Some(1));
        assert_eq!(Grade::nova(25).versao(), Some(2));
        assert_eq!(Grade::nova(177).versao(), Some(40));
        assert_eq!(Grade::nova(22).versao(), None, "22 não é 17 + 4k");
        assert_eq!(Grade::nova(17).versao(), None, "menor que a versão 1");
    }

    #[test]
    fn transpor_duas_vezes_e_a_identidade() {
        let mut g = Grade::nova(21);
        g.marca(3, 5, true);
        g.marca(0, 20, true);
        assert_eq!(g.transposta().transposta(), g);
        assert!(g.transposta().escuro(5, 3));
    }
}
