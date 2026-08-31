//! Leitura de QR code, do zero — sem dependência nova em nenhuma plataforma.
//!
//! Entra no mesmo comando do reconhecimento de texto (`Ctrl+Alt+PrtScr`), e
//! antes dele: se a região selecionada tem um QR, o que o usuário quer é o
//! conteúdo do QR, não o OCR dos quadradinhos. Quando não há QR a tentativa
//! falha rápido — sem os três padrões localizadores não há o que decodificar —
//! e o texto segue o caminho de sempre.
//!
//! ## Como o problema se divide
//!
//! `detecta` vai da imagem à `Grade` de módulos: binariza, acha os três
//! localizadores, descobre o tamanho e amostra. `formato` lê o cabeçalho do
//! símbolo (nível de correção e máscara) e sabe quais módulos são função.
//! `dados` percorre o resto em zigue-zague, desintercala os blocos, corrige
//! com Reed-Solomon (`galois`) e decodifica os segmentos.
//!
//! A `Grade` no meio é o que permite testar a segunda metade sem imagem
//! nenhuma, e é por onde o `gera` (só em teste) injeta símbolos sintéticos.
//!
//! ## Sobre confiar nisto
//!
//! Reed-Solomon errado por um bit devolve lixo em silêncio. Por isso
//! `tabelas` carrega as constantes do ISO/IEC 18004 com testes de invariante
//! em cima delas, e por isso existe um símbolo de referência vindo de fonte
//! externa: um teste de ida-e-volta contra o nosso próprio gerador passaria
//! mesmo com os dois lados errados da mesma forma.

mod dados;
mod detecta;
mod formato;
mod galois;
mod grade;
mod tabelas;

#[cfg(test)]
mod gera;
#[cfg(test)]
mod referencia;

use crate::imgbuf::RgbaImage;
use grade::Grade;

/// Decodifica o primeiro QR encontrado na imagem.
///
/// `None` quando não há QR, quando ele está ilegível, ou quando a correção de
/// erro não fecha — e a distinção não interessa a quem chama: nos três casos o
/// que sobra é tentar o OCR.
pub fn decode(image: &RgbaImage) -> Option<String> {
    let grade = detecta::grade(image)?;
    // Espelhado é barato de tentar e acontece de verdade: captura de uma tela
    // espelhada, ou foto de um adesivo pelo avesso.
    conteudo(&grade).or_else(|| conteudo(&grade.transposta()))
}

fn conteudo(grade: &Grade) -> Option<String> {
    let versao = grade.versao()?;
    let (nivel, mascara) = formato::ler(grade)?;
    dados::texto(grade, versao, nivel, mascara)
}

#[cfg(test)]
mod tests {
    use super::tabelas::Nivel;
    use super::*;

    /// O teste que amarra as duas metades: imagem entra, texto sai.
    #[test]
    fn le_um_simbolo_desenhado_como_imagem() {
        let texto = "https://github.com/mBarony/printscreen-windows11-rust";
        let g = gera::simbolo(texto, 5, Nivel::M, 3);
        for escala in [2u32, 3, 5, 11] {
            let img = gera::imagem(&g, escala, 4);
            assert_eq!(decode(&img).as_deref(), Some(texto), "escala {escala}");
        }
    }

    #[test]
    fn le_mesmo_espelhado() {
        let texto = "RustShot";
        let g = gera::simbolo(texto, 2, Nivel::Q, 6);
        let img = gera::imagem(&g.transposta(), 6, 4);
        assert_eq!(decode(&img).as_deref(), Some(texto));
    }

    #[test]
    fn a_zona_de_silencio_minima_basta() {
        let texto = "1234567890";
        let g = gera::simbolo(texto, 1, Nivel::L, 0);
        // O padrão pede quatro módulos de margem; com dois o símbolo ainda tem
        // de ser achado, porque captura de tela recortada rente acontece.
        let img = gera::imagem(&g, 8, 2);
        assert_eq!(decode(&img).as_deref(), Some(texto));
    }

    #[test]
    fn uma_captura_sem_qr_nao_inventa_conteudo() {
        // Ruído determinístico, do tipo que uma foto ou um degradê produzem.
        let mut img = RgbaImage::filled(120, 90, [255, 255, 255, 255]);
        for y in 0..90u32 {
            for x in 0..120u32 {
                let v = ((x * 37 + y * 61) % 256) as u8;
                img.pixel_mut(x, y).copy_from_slice(&[v, v, v, 255]);
            }
        }
        assert!(decode(&img).is_none());
    }
}
