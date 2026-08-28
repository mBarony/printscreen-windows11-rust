//! Lembra a última região capturada, para poder repeti-la.
//!
//! O retângulo é guardado em coordenadas absolutas do desktop virtual, e não
//! relativas a um monitor: quem grava é o processo de GUI e quem lê é o
//! residente, e entre um e outro a lista de monitores pode ter mudado. Um
//! índice de monitor apontaria para outro lugar; um retângulo absoluto, não.
//!
//! Fica num arquivo ao lado do `config.json` porque é estado, não
//! configuração — não faz sentido o usuário editá-lo, nem que ele sobreviva
//! a uma reinstalação.

use std::path::PathBuf;

/// `(x, y, largura, altura)` em px do desktop virtual.
pub type Region = (i32, i32, u32, u32);

fn path() -> PathBuf {
    crate::config::state_dir().join("last-region")
}

pub fn save(region: Region) {
    let (x, y, w, h) = region;
    if let Err(err) = std::fs::write(path(), format!("{x} {y} {w} {h}")) {
        // Não vale interromper a captura por causa disto: perder a memória
        // da última região custa um arrasto, não o trabalho.
        log::debug!("não foi possível lembrar a última região: {err}");
    }
}

pub fn load() -> Option<Region> {
    parse(&std::fs::read_to_string(path()).ok()?)
}

/// `"x y w h"` para um retângulo, ou `None` se o texto não servir.
fn parse(text: &str) -> Option<Region> {
    let mut campos = text.split_whitespace();
    let x = campos.next()?.parse().ok()?;
    let y = campos.next()?.parse().ok()?;
    let w: u32 = campos.next()?.parse().ok()?;
    let h: u32 = campos.next()?.parse().ok()?;
    // Região degenerada não é repetível, e gravá-la seria um bug em outro
    // lugar; ler defensivamente é mais barato que confiar.
    (w > 0 && h > 0).then_some((x, y, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_que_grava() {
        assert_eq!(parse("10 20 300 400"), Some((10, 20, 300, 400)));
    }

    #[test]
    fn aceita_coordenadas_negativas() {
        // Um monitor à esquerda do principal tem x negativo.
        assert_eq!(parse("-1920 -100 800 600"), Some((-1920, -100, 800, 600)));
    }

    #[test]
    fn recusa_texto_que_nao_serve() {
        for texto in ["", "10 20", "a b c d", "10 20 0 400", "10 20 300 0"] {
            assert_eq!(parse(texto), None, "deveria recusar {texto:?}");
        }
    }

    #[test]
    fn ignora_sobras_no_fim() {
        assert_eq!(parse("10 20 300 400 lixo\n"), Some((10, 20, 300, 400)));
    }
}
