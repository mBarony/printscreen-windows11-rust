//! Escolhe o formato de saída de uma captura e a codifica nele.
//!
//! O JPG borra bordas duras, que é justamente o que mais aparece numa captura
//! de tela: texto, ícones, linhas de interface. O PNG guarda isso intacto, mas
//! sobre conteúdo fotográfico gera arquivos muito maiores sem ganho visível.
//!
//! Daí o padrão ser `Auto`: a decisão é por imagem, olhando quantas cores
//! distintas ela tem.

use std::io::Write;

use crate::error::{Context, Result};
use crate::imgbuf::RgbaImage;

/// Qualidade do JPG, quando é ele que sai.
const JPG_QUALITY: u8 = 90;

/// Quantos pixels a heurística examina. Amostrar é suficiente e mantém o
/// custo constante: uma captura 4K tem 8 milhões de pixels, e contar todos
/// para decidir a extensão seria desproporcional.
const AMOSTRA: usize = 4096;

/// Acima desta fração de cores distintas na amostra, a imagem é tratada como
/// fotográfica. Interface e texto ficam bem abaixo — poucas cores repetidas
/// em áreas grandes; foto e gradiente quase não repetem cor.
const LIMIAR_FOTOGRAFICO: f32 = 0.25;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Format {
    /// Decide por imagem, pela contagem de cores.
    #[default]
    Auto,
    Jpg,
    Png,
}

impl Format {
    pub fn from_str_tolerant(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "jpg" | "jpeg" => Some(Self::Jpg),
            "png" => Some(Self::Png),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Jpg => "jpg",
            Self::Png => "png",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automático",
            Self::Jpg => "JPG",
            Self::Png => "PNG",
        }
    }
}

/// Formato concreto para esta imagem — `Auto` resolvido.
pub fn resolve(format: Format, image: &RgbaImage) -> Format {
    match format {
        Format::Auto => {
            if parece_fotografica(image) {
                Format::Jpg
            } else {
                Format::Png
            }
        }
        outro => outro,
    }
}

/// `true` quando a imagem tem cores demais para ser interface ou texto.
///
/// A amostragem percorre a imagem com um passo **coprimo com a largura**, para
/// varrer todas as colunas em vez de voltar sempre às mesmas — numa captura de
/// janela, amostrar só a coluna da barra lateral diria que a tela inteira tem
/// duas cores.
///
/// A coprimalidade era prometida no comentário e não existia no código: o passo
/// saía de uma divisão e a largura fazia o que quisesse com ele. Não chegava a
/// ser o desastre que o nome sugere — a pior resolução comum, 2560×1440, ainda
/// visitava 128 colunas —, mas 128 de 2560 é 5% da imagem decidindo o formato
/// de saída.
fn parece_fotografica(image: &RgbaImage) -> bool {
    let total = (image.width() as usize) * (image.height() as usize);
    if total == 0 {
        return false;
    }
    let passo = passo_coprimo((total / AMOSTRA).max(1), image.width() as usize);
    let mut cores = std::collections::HashSet::new();
    let mut vistos = 0usize;

    let mut i = 0usize;
    while i < total {
        let x = (i % image.width() as usize) as u32;
        let y = (i / image.width() as usize) as u32;
        let px = image.pixel(x, y);
        cores.insert([px[0], px[1], px[2]]);
        vistos += 1;
        i += passo;
    }

    vistos > 0 && (cores.len() as f32 / vistos as f32) > LIMIAR_FOTOGRAFICO
}

/// O primeiro passo a partir de `inicial` que não compartilha divisor com a
/// largura — é o que faz a varredura passar por todas as colunas.
///
/// A busca acaba: entre `inicial` e `inicial + largura` há sempre um coprimo,
/// porque basta cair num valor congruente a 1 módulo a largura.
fn passo_coprimo(inicial: usize, largura: usize) -> usize {
    fn mdc(a: usize, b: usize) -> usize {
        if b == 0 { a } else { mdc(b, a % b) }
    }
    if largura <= 1 {
        return inicial.max(1);
    }
    (inicial..inicial + largura)
        .find(|p| mdc(*p, largura) == 1)
        .unwrap_or(inicial)
}

/// Codifica a imagem, devolvendo a extensão usada.
pub fn encode<W: Write>(writer: W, image: &RgbaImage, format: Format) -> Result<&'static str> {
    match resolve(format, image) {
        Format::Png => {
            encode_png(writer, image)?;
            Ok("png")
        }
        // `Auto` já foi resolvido acima; o braço existe porque o `match`
        // precisa ser total.
        Format::Jpg | Format::Auto => {
            // JPG não tem alfa: descartar é seguro, a captura é opaca.
            let rgb = image.to_rgb();
            crate::jpeg::encode_rgb(writer, &rgb, image.width(), image.height(), JPG_QUALITY)?;
            Ok("jpg")
        }
    }
}

fn encode_png<W: Write>(writer: W, image: &RgbaImage) -> Result<()> {
    let mut encoder = png::Encoder::new(writer, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut escritor = encoder
        .write_header()
        .context("escrevendo o cabeçalho do PNG")?;
    escritor
        .write_image_data(image.as_raw())
        .context("escrevendo os dados do PNG")?;
    escritor.finish().context("fechando o PNG")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nas resoluções reais o passo tem de varrer TODAS as colunas. Antes ele
    /// saía de uma divisão e a largura decidia: a 2560×1440 a varredura via 128
    /// colunas das 2560, e a 1280×720 via 256 das 1280.
    #[test]
    fn o_passo_varre_todas_as_colunas() {
        fn mdc(a: usize, b: usize) -> usize {
            if b == 0 { a } else { mdc(b, a % b) }
        }
        for (w, h) in [
            (1920usize, 1080usize),
            (2560, 1440),
            (3840, 2160),
            (1366, 768),
            (1280, 720),
            (3440, 1440),
            (800, 600),
        ] {
            let passo = passo_coprimo((w * h / AMOSTRA).max(1), w);
            assert_eq!(mdc(passo, w), 1, "{w}x{h}: passo {passo} não é coprimo com {w}");
        }
    }

    #[test]
    fn o_passo_coprimo_aguenta_largura_degenerada() {
        assert_eq!(passo_coprimo(1, 0), 1);
        assert_eq!(passo_coprimo(7, 1), 7);
    }

    /// Imagem com poucas cores em áreas grandes — o perfil de uma interface.
    fn interface() -> RgbaImage {
        let (w, h) = (200u32, 100u32);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let c = if y < 20 {
                    [240, 240, 240, 255]
                } else if x < 60 {
                    [30, 30, 40, 255]
                } else {
                    [255, 255, 255, 255]
                };
                px.extend_from_slice(&c);
            }
        }
        RgbaImage::from_raw(w, h, px)
    }

    /// Gradiente que quase não repete cor — o perfil de uma foto.
    fn fotografica() -> RgbaImage {
        let (w, h) = (200u32, 100u32);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&[
                    (x % 256) as u8,
                    (y * 2 % 256) as u8,
                    ((x + y) % 256) as u8,
                    255,
                ]);
            }
        }
        RgbaImage::from_raw(w, h, px)
    }

    #[test]
    fn interface_vai_para_png() {
        assert_eq!(resolve(Format::Auto, &interface()), Format::Png);
    }

    #[test]
    fn imagem_fotografica_vai_para_jpg() {
        assert_eq!(resolve(Format::Auto, &fotografica()), Format::Jpg);
    }

    #[test]
    fn a_escolha_explicita_manda_sobre_a_heuristica() {
        // Mesmo parecendo interface, PNG pedido é PNG; e vice-versa.
        assert_eq!(resolve(Format::Jpg, &interface()), Format::Jpg);
        assert_eq!(resolve(Format::Png, &fotografica()), Format::Png);
    }

    #[test]
    fn png_sai_com_a_assinatura_do_formato() {
        let mut saida = Vec::new();
        let ext = encode(&mut saida, &interface(), Format::Png).expect("codificar");
        assert_eq!(ext, "png");
        assert_eq!(&saida[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn jpg_sai_com_a_assinatura_do_formato() {
        let mut saida = Vec::new();
        let ext = encode(&mut saida, &fotografica(), Format::Jpg).expect("codificar");
        assert_eq!(ext, "jpg");
        assert_eq!(&saida[..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn png_preserva_a_imagem_sem_perda() {
        // O ponto de existir PNG: o que entra é o que sai. Um JPG q90 no
        // mesmo conteúdo mudaria os pixels das bordas.
        let origem = interface();
        let mut saida = Vec::new();
        encode(&mut saida, &origem, Format::Png).expect("codificar");

        let decodificador = png::Decoder::new(std::io::Cursor::new(&saida));
        let mut leitor = decodificador.read_info().expect("ler o cabeçalho");
        let mut pixels = vec![0u8; leitor.output_buffer_size().expect("tamanho")];
        let info = leitor.next_frame(&mut pixels).expect("ler os dados");
        assert_eq!(&pixels[..info.buffer_size()], origem.as_raw());
    }

    #[test]
    fn imagem_vazia_nao_quebra_a_heuristica() {
        let vazia = RgbaImage::from_raw(0, 0, Vec::new());
        assert_eq!(resolve(Format::Auto, &vazia), Format::Png);
    }

    #[test]
    fn o_nome_do_formato_faz_a_volta() {
        for f in [Format::Auto, Format::Jpg, Format::Png] {
            assert_eq!(Format::from_str_tolerant(f.as_str()), Some(f));
        }
        // Tolerante com o que o usuário escreveria à mão.
        assert_eq!(Format::from_str_tolerant(" JPEG "), Some(Format::Jpg));
        assert_eq!(Format::from_str_tolerant("bmp"), None);
    }
}
