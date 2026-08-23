//! Reconhecimento de texto pelo motor do Windows (`Windows.Media.Ocr`).
//!
//! É o mesmo motor da Ferramenta de Captura, então funciona num Windows 11
//! limpo, sem o usuário instalar nada — a alternativa seria pedir a ele que
//! instalasse o Tesseract, 25 MB que sequer entram no PATH.
//!
//! Esta é a única parte do app que usa a crate `windows` em vez da
//! `windows-sys`. Não é uma dependência nova: ela já entra no binário por
//! `eframe → wgpu → wgpu-hal → gpu-allocator`, por causa do backend DX12. O
//! `windows-sys` cobre só Win32 clássico, e OCR é WinRT — reescrever as
//! vtables COM à mão custaria umas 450 linhas de `unsafe` para economizar
//! alguns KB num binário que tem folga de 9 MB.
//!
//! O reconhecimento é síncrono para quem chama, mas a API por baixo é
//! assíncrona: `RecognizeAsync().get()` bloqueia. Por isso este módulo só
//! pode ser usado de uma thread de trabalho (`crate::jobs`), nunca da thread
//! da interface.

// Módulo ainda não ligado à interface: por ora é a prova de conceito que
// mede o custo real do OCR no binário. O allow sai junto com o botão.
#![allow(dead_code)]

use crate::error::Result;
use crate::imgbuf::RgbaImage;

/// Ampliação aplicada antes de reconhecer.
///
/// Truque emprestado do PowerToys (PowerOCR faz o mesmo 1,5×): o motor foi
/// treinado para texto de documento, e fonte de interface numa captura
/// 1:1 fica pequena demais para ele. Ampliar antes melhora sensivelmente
/// o acerto, e é barato perto do próprio reconhecimento.
pub(super) const UPSCALE: f32 = 1.5;

/// Amplia por interpolação bilinear e já entrega em BGRA, que é o formato
/// que o `SoftwareBitmap` espera — as duas passadas num laço só.
pub(super) fn upscaled_bgra(image: &RgbaImage, scale: f32) -> (Vec<u8>, u32, u32) {
    let (sw, sh) = (image.width(), image.height());
    let dw = ((sw as f32 * scale).round() as u32).max(1);
    let dh = ((sh as f32 * scale).round() as u32).max(1);
    let mut out = Vec::with_capacity(dw as usize * dh as usize * 4);

    for y in 0..dh {
        let fy = ((y as f32 + 0.5) / scale - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy.floor() as u32, fy - fy.floor());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) / scale - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx.floor() as u32, fx - fx.floor());
            let x1 = (x0 + 1).min(sw - 1);
            let (p00, p10) = (image.pixel(x0, y0), image.pixel(x1, y0));
            let (p01, p11) = (image.pixel(x0, y1), image.pixel(x1, y1));
            // RGBA de origem → BGRA de destino: os canais saem trocados.
            let mut px = [0u8; 4];
            for (dst, src) in [(0usize, 2usize), (1, 1), (2, 0), (3, 3)] {
                let top = p00[src] as f32 * (1.0 - tx) + p10[src] as f32 * tx;
                let bottom = p01[src] as f32 * (1.0 - tx) + p11[src] as f32 * tx;
                px[dst] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
            }
            out.extend_from_slice(&px);
        }
    }
    (out, dw, dh)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::error::err;
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::DataWriter;

    /// Lado máximo aceito pelo motor. Acima disso ele recusa a imagem, então
    /// vale conferir antes e dar uma mensagem melhor que o erro cru do WinRT.
    fn max_dimension() -> u32 {
        OcrEngine::MaxImageDimension().unwrap_or(10_000)
    }

    /// Motor para o idioma pedido, ou o do perfil do usuário.
    fn engine(language: Option<&str>) -> Result<OcrEngine> {
        let Some(tag) = language else {
            return default_engine();
        };
        let lang = Language::CreateLanguage(&HSTRING::from(tag))
            .map_err(|e| err!("idioma inválido para OCR ({tag}): {e}"))?;
        if !OcrEngine::IsLanguageSupported(&lang).unwrap_or(false) {
            return Err(err!(
                "não há pacote de OCR para {tag}. \
                 Instale-o em Configurações › Hora e idioma › Idioma"
            ));
        }
        OcrEngine::TryCreateFromLanguage(&lang)
            .map_err(|e| err!("não foi possível iniciar o OCR para {tag}: {e}"))
    }

    /// Motor para o perfil do usuário, recuando para o primeiro pacote
    /// instalado.
    ///
    /// O recuo não é luxo: `TryCreateFromUserProfileLanguages` devolve
    /// **nulo** — e não erro — quando nenhum idioma do perfil tem pacote de
    /// OCR. A windows-rs transforma esse nulo num `Error` cujo `HRESULT` é
    /// `S_OK`, e a mensagem que sai dele é "The operation completed
    /// successfully", que não ajuda ninguém. Um Windows em pt-BR com apenas
    /// o pacote en-US instalado cai exatamente aqui: existe motor utilizável
    /// e mesmo assim a criação falha.
    ///
    /// Cair no primeiro instalado é o que o PowerToys faz (`ImageMethods.cs`
    /// tenta o idioma do teclado, depois o `AbbreviatedName`, e por fim o
    /// primeiro da lista).
    fn default_engine() -> Result<OcrEngine> {
        if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
            return Ok(engine);
        }
        let langs = OcrEngine::AvailableRecognizerLanguages()
            .map_err(|e| err!("não foi possível listar os idiomas de OCR: {e}"))?;
        let first = langs.into_iter().next().ok_or_else(|| {
            err!(
                "nenhum pacote de OCR instalado. \
                 Instale um em Configurações › Hora e idioma › Idioma"
            )
        })?;
        let tag = first
            .LanguageTag()
            .map(|t| t.to_string_lossy())
            .unwrap_or_else(|_| "?".to_owned());
        OcrEngine::TryCreateFromLanguage(&first)
            .map_err(|e| err!("não foi possível iniciar o OCR para {tag}: {e}"))
    }

    fn software_bitmap(image: &RgbaImage, scale: f32) -> Result<SoftwareBitmap> {
        let (bytes, width, height) = upscaled_bgra(image, scale);
        let writer = DataWriter::new().map_err(|e| err!("DataWriter falhou: {e}"))?;
        writer
            .WriteBytes(&bytes)
            .map_err(|e| err!("escrita do buffer falhou: {e}"))?;
        let buffer = writer
            .DetachBuffer()
            .map_err(|e| err!("buffer de imagem falhou: {e}"))?;
        SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
        )
        .map_err(|e| err!("bitmap para OCR falhou: {e}"))
    }

    /// Reconhece o texto da imagem. **Bloqueia** — só chame de thread de
    /// trabalho.
    pub fn recognize(image: &RgbaImage, language: Option<&str>) -> Result<String> {
        if image.width() == 0 || image.height() == 0 {
            return Err(err!("imagem vazia"));
        }
        let limit = max_dimension();
        if image.width() > limit || image.height() > limit {
            return Err(err!(
                "imagem grande demais para o OCR ({}×{}; o limite é {limit} px por lado)",
                image.width(),
                image.height()
            ));
        }
        // Só amplia se o resultado ainda couber no motor; senão vai 1:1, que
        // é o que o PowerOCR também faz nesse caso.
        let scale = if (image.width().max(image.height()) as f32 * UPSCALE) <= limit as f32 {
            UPSCALE
        } else {
            1.0
        };

        let engine = engine(language)?;
        let bitmap = software_bitmap(image, scale)?;
        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| err!("OCR falhou: {e}"))?
            .get()
            .map_err(|e| err!("OCR não concluiu: {e}"))?;

        // `OcrResult::Text` devolve tudo numa linha só. Percorrer as linhas
        // preserva as quebras, que é o que faz o texto colado continuar
        // legível.
        let lines = result.Lines().map_err(|e| err!("OCR sem linhas: {e}"))?;
        let mut text = String::new();
        for line in lines {
            if let Ok(content) = line.Text() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&content.to_string_lossy());
            }
        }
        if text.trim().is_empty() {
            return Err(err!("nenhum texto reconhecido na imagem"));
        }
        Ok(text)
    }

    /// Idiomas com pacote de OCR instalado, em etiquetas BCP-47.
    pub fn available_languages() -> Vec<String> {
        let Ok(list) = OcrEngine::AvailableRecognizerLanguages() else {
            return Vec::new();
        };
        list.into_iter()
            .filter_map(|lang| lang.LanguageTag().ok())
            .map(|tag| tag.to_string_lossy())
            .collect()
    }
}

#[cfg(windows)]
#[allow(unused_imports)]
pub use imp::{available_languages, recognize};

/// Fora do Windows não há motor de OCR do sistema.
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn recognize(_image: &RgbaImage, _language: Option<&str>) -> Result<String> {
    Err(crate::error::err!("OCR disponível apenas no Windows"))
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn available_languages() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Duas cores bem separadas, uma por metade horizontal.
    fn duas_faixas() -> RgbaImage {
        let mut px = Vec::new();
        for _ in 0..4 {
            for x in 0..4u32 {
                let c = if x < 2 {
                    [10, 20, 30, 255]
                } else {
                    [200, 210, 220, 255]
                };
                px.extend_from_slice(&c);
            }
        }
        RgbaImage::from_raw(4, 4, px)
    }

    #[test]
    fn escala_1_so_troca_rgba_por_bgra() {
        let (bytes, w, h) = upscaled_bgra(&duas_faixas(), 1.0);
        assert_eq!((w, h), (4, 4));
        assert_eq!(bytes.len(), 4 * 4 * 4);
        // RGBA (10,20,30,255) vira BGRA (30,20,10,255).
        assert_eq!(&bytes[0..4], &[30, 20, 10, 255]);
        // Último pixel da primeira linha, já do outro lado da faixa.
        assert_eq!(&bytes[12..16], &[220, 210, 200, 255]);
    }

    #[test]
    fn ampliacao_da_as_dimensoes_esperadas() {
        let (_, w, h) = upscaled_bgra(&duas_faixas(), UPSCALE);
        assert_eq!((w, h), (6, 6));
    }

    #[test]
    fn cantos_sobrevivem_a_ampliacao() {
        let (bytes, w, h) = upscaled_bgra(&duas_faixas(), 3.0);
        let em = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
        };
        // O alinhamento por centro de pixel preserva os extremos: a borda não
        // é misturada com um vizinho de fora, que não existe.
        assert_eq!(em(0, 0), [30, 20, 10, 255]);
        assert_eq!(em(w - 1, h - 1), [220, 210, 200, 255]);
    }

    #[test]
    fn ampliacao_interpola_a_transicao() {
        let (bytes, w, _) = upscaled_bgra(&duas_faixas(), 4.0);
        // Na fronteira das faixas tem de haver tons intermediários; uma
        // ampliação por vizinho mais próximo só teria os dois extremos.
        let linha: Vec<u8> = (0..w).map(|x| bytes[((x * 4) + 2) as usize]).collect();
        assert!(
            linha.iter().any(|&r| r > 15 && r < 195),
            "sem transição suave: {linha:?}"
        );
    }

    #[test]
    fn imagem_de_1px_nao_degenera() {
        let img = RgbaImage::from_raw(1, 1, vec![1, 2, 3, 4]);
        let (bytes, w, h) = upscaled_bgra(&img, UPSCALE);
        assert_eq!((w, h), (2, 2));
        assert!(bytes.chunks(4).all(|p| p == [3, 2, 1, 4]));
    }

    /// Texto preto sobre fundo branco, rasterizado com a mesma fonte que a
    /// exportação usa. Serve de entrada conhecida para o OCR.
    #[cfg(windows)]
    fn imagem_com_texto(conteudo: &str) -> RgbaImage {
        use crate::editor::render::draw_text;
        use crate::editor::shapes::Point;
        use ab_glyph::FontRef;

        let (w, h) = (640u32, 200u32);
        let mut img = RgbaImage::from_raw(w, h, vec![255; (w * h * 4) as usize]);
        let font = FontRef::try_from_slice(crate::editor::FONT_BYTES).expect("fonte embutida");
        draw_text(
            &mut img,
            &font,
            Point { x: 24.0, y: 40.0 },
            conteudo,
            [0, 0, 0, 255],
            56.0,
        );
        img
    }

    /// Exercita o motor de verdade: rasteriza um texto conhecido e confere que
    /// o OCR o devolve. Precisa de Windows com pacote de idioma instalado —
    /// por isso `#[ignore]`.
    ///
    /// Rasterizar em vez de capturar a tela mantém o teste determinístico e
    /// evita que conteúdo da máquina de quem roda vá parar na saída.
    ///
    /// `cargo test --features ocr -- --ignored --nocapture ocr_de_verdade`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn ocr_de_verdade() {
        let idiomas = available_languages();
        println!("idiomas com pacote de OCR: {idiomas:?}");
        assert!(
            !idiomas.is_empty(),
            "nenhum pacote de OCR instalado — instale um em \
             Configurações › Hora e idioma › Idioma"
        );

        let esperado = "RUSTSHOT";
        let img = imagem_com_texto(esperado);
        let texto = recognize(&img, None).expect("reconhecimento falhou");
        println!("--- reconhecido ---\n{texto}\n--- fim ---");

        // Comparação tolerante: o motor pode confundir O/0 ou inserir espaços,
        // e o que se quer provar aqui é que ele lê, não que é perfeito.
        let normalizado: String = texto
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_uppercase())
            .collect();
        assert!(
            normalizado.contains("RUST"),
            "esperava reconhecer {esperado:?}, veio {texto:?}"
        );
    }
}
