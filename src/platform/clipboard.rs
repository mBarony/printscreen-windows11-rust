//! Área de transferência de imagem via CF_DIB (substitui `arboard`).
//!
//! O formato CF_DIB (BITMAPINFOHEADER + BGRA bottom-up, 32 bpp) é o que
//! Paint, Word e navegadores esperam ao colar bitmaps.

use crate::error::{err, Result};
use crate::imgbuf::RgbaImage;

#[cfg(windows)]
pub fn set_image(image: &RgbaImage) -> Result<()> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    const CF_DIB: u32 = 8;

    let (w, h) = (image.width() as usize, image.height() as usize);
    let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
    let pixels_size = w * h * 4;
    let total = header_size + pixels_size;

    // SAFETY: sequência clássica OpenClipboard → Empty → SetClipboardData.
    // Depois de um SetClipboardData bem-sucedido o HGLOBAL pertence ao
    // sistema (não liberamos). Em falha antes disso, o handle vaza apenas
    // se GlobalAlloc teve sucesso e SetClipboardData falhou — caso raro,
    // tratado com GlobalFree implícito no fechamento do processo (mesma
    // política do arboard).
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err(err!("OpenClipboard falhou ({})", GetLastError()));
        }
        let result = (|| -> Result<()> {
            if EmptyClipboard() == 0 {
                return Err(err!("EmptyClipboard falhou ({})", GetLastError()));
            }
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, total);
            if hglobal.is_null() {
                return Err(err!("GlobalAlloc({} bytes) falhou", total));
            }
            let dst = GlobalLock(hglobal) as *mut u8;
            if dst.is_null() {
                return Err(err!("GlobalLock falhou ({})", GetLastError()));
            }

            let header = BITMAPINFOHEADER {
                biSize: header_size as u32,
                biWidth: w as i32,
                biHeight: h as i32, // positivo = bottom-up
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: pixels_size as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            std::ptr::copy_nonoverlapping(
                &header as *const BITMAPINFOHEADER as *const u8,
                dst,
                header_size,
            );

            // RGBA top-down → BGRA bottom-up.
            let src = image.as_raw();
            let out = std::slice::from_raw_parts_mut(dst.add(header_size), pixels_size);
            for row in 0..h {
                let src_row = &src[(h - 1 - row) * w * 4..][..w * 4];
                let dst_row = &mut out[row * w * 4..][..w * 4];
                let (src_px, _) = src_row.as_chunks::<4>();
                let (dst_px, _) = dst_row.as_chunks_mut::<4>();
                for (s, d) in src_px.iter().zip(dst_px.iter_mut()) {
                    d[0] = s[2]; // B
                    d[1] = s[1]; // G
                    d[2] = s[0]; // R
                    d[3] = s[3]; // (reservado no CF_DIB; inofensivo)
                }
            }
            GlobalUnlock(hglobal);

            if SetClipboardData(CF_DIB, hglobal as _).is_null() {
                return Err(err!("SetClipboardData falhou ({})", GetLastError()));
            }
            Ok(())
        })();
        CloseClipboard();
        result
    }
}

/// Lê a imagem que estiver na área de transferência.
///
/// Aceita `CF_DIB` de 24 e 32 bits, que é o que Paint, navegadores e a
/// própria Ferramenta de Captura colocam lá. Formatos comprimidos (JPEG
/// dentro do clipboard) e paletas indexadas ficam de fora: valem pouco
/// diante do que custaria decodificá-los aqui.
#[cfg(windows)]
pub fn get_image() -> Result<RgbaImage> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    const CF_DIB: u32 = 8;

    // SAFETY: abre o clipboard, lê o handle (que continua sendo do sistema —
    // nunca liberado aqui) e fecha antes de retornar, em qualquer caminho.
    unsafe {
        if IsClipboardFormatAvailable(CF_DIB) == 0 {
            return Err(err!("não há imagem na área de transferência"));
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err(err!("OpenClipboard falhou ({})", GetLastError()));
        }
        let result = (|| -> Result<RgbaImage> {
            let handle = GetClipboardData(CF_DIB);
            if handle.is_null() {
                return Err(err!("GetClipboardData falhou ({})", GetLastError()));
            }
            let src = GlobalLock(handle) as *const u8;
            if src.is_null() {
                return Err(err!("GlobalLock falhou ({})", GetLastError()));
            }
            let size = GlobalSize(handle);
            let bytes = std::slice::from_raw_parts(src, size);
            let image = decode_dib(bytes);
            GlobalUnlock(handle);
            image
        })();
        CloseClipboard();
        result
    }
}

/// Converte um `CF_DIB` em RGBA top-down.
#[cfg(windows)]
fn decode_dib(bytes: &[u8]) -> Result<RgbaImage> {
    use windows_sys::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};

    let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
    if bytes.len() < header_size {
        return Err(err!("área de transferência com bitmap truncado"));
    }
    // SAFETY: o tamanho foi conferido acima e o layout do cabeçalho é fixo.
    let header: BITMAPINFOHEADER =
        unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const BITMAPINFOHEADER) };

    if header.biCompression != BI_RGB {
        return Err(err!("formato de bitmap não suportado na área de transferência"));
    }
    let bpp = header.biBitCount;
    if bpp != 24 && bpp != 32 {
        return Err(err!("bitmap de {bpp} bits não suportado"));
    }
    let width = header.biWidth;
    if width <= 0 || header.biHeight == 0 {
        return Err(err!("bitmap com dimensões inválidas"));
    }
    // `biHeight` negativo significa top-down; positivo, bottom-up.
    let bottom_up = header.biHeight > 0;
    let height = header.biHeight.unsigned_abs();
    let width = width as u32;

    // Cada linha do DIB é alinhada em 4 bytes.
    let channels = bpp as usize / 8;
    let stride = (width as usize * channels + 3) & !3;
    let start = header.biSize.max(header_size as u32) as usize;
    let needed = start + stride * height as usize;
    if bytes.len() < needed {
        return Err(err!("área de transferência com pixels truncados"));
    }

    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for row in 0..height {
        let source_row = if bottom_up { height - 1 - row } else { row };
        let line = &bytes[start + stride * source_row as usize..][..width as usize * channels];
        for px in line.chunks_exact(channels) {
            let alpha = if channels == 4 { px[3] } else { 255 };
            // Um DIB de 32 bits costuma trazer o quarto byte zerado em vez do
            // alfa; tratá-lo como transparência deixaria a imagem invisível.
            let alpha = if alpha == 0 { 255 } else { alpha };
            pixels.extend_from_slice(&[px[2], px[1], px[0], alpha]);
        }
    }
    Ok(RgbaImage::from_raw(width, height, pixels))
}

/// Põe texto na área de transferência, em `CF_UNICODETEXT`.
///
/// O Windows guarda UTF-16 terminado em zero. A conversão a partir do `str`
/// é feita aqui, e não por quem chama, porque é detalhe do formato — o resto
/// do programa trabalha em UTF-8.
///
/// Texto vazio é recusado em vez de esvaziar a área de transferência: quem
/// pede para copiar um reconhecimento que não achou nada preferiria manter o
/// que já tinha copiado antes a perdê-lo em silêncio.
#[cfg(windows)]
pub fn set_text(text: &str) -> Result<()> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    const CF_UNICODETEXT: u32 = 13;

    if text.is_empty() {
        return Err(err!("nada a copiar"));
    }

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = std::mem::size_of_val(wide.as_slice());

    // SAFETY: mesma sequência e mesma política de posse do `set_image` — o
    // HGLOBAL passa a ser do sistema assim que o SetClipboardData aceita.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err(err!("OpenClipboard falhou ({})", GetLastError()));
        }
        let result = (|| -> Result<()> {
            if EmptyClipboard() == 0 {
                return Err(err!("EmptyClipboard falhou ({})", GetLastError()));
            }
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if hglobal.is_null() {
                return Err(err!("GlobalAlloc falhou ({})", GetLastError()));
            }
            let dst = GlobalLock(hglobal);
            if dst.is_null() {
                return Err(err!("GlobalLock falhou ({})", GetLastError()));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst as *mut u16, wide.len());
            GlobalUnlock(hglobal);
            if SetClipboardData(CF_UNICODETEXT, hglobal as _).is_null() {
                return Err(err!("SetClipboardData falhou ({})", GetLastError()));
            }
            Ok(())
        })();
        CloseClipboard();
        result
    }
}

/// Texto da área de transferência, em UTF-8.
///
/// Serve para reconhecer o formato próprio das anotações copiadas; um texto
/// qualquer também volta daqui, e é quem chama que decide se ele interessa.
#[cfg(windows)]
pub fn get_text() -> Result<String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;

    // SAFETY: mesma política do `get_image` — o handle continua sendo do
    // sistema, e o clipboard fecha em qualquer caminho de saída.
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return Err(err!("não há texto na área de transferência"));
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err(err!("OpenClipboard falhou ({})", GetLastError()));
        }
        let result = (|| -> Result<String> {
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                return Err(err!("GetClipboardData falhou ({})", GetLastError()));
            }
            let src = GlobalLock(handle) as *const u16;
            if src.is_null() {
                return Err(err!("GlobalLock falhou ({})", GetLastError()));
            }
            // `GlobalSize` conta bytes e inclui o terminador; a conversão
            // para em qualquer caso ao encontrar o zero.
            let unidades = GlobalSize(handle) / std::mem::size_of::<u16>();
            let wide = std::slice::from_raw_parts(src, unidades);
            let fim = wide.iter().position(|c| *c == 0).unwrap_or(wide.len());
            let texto = String::from_utf16_lossy(&wide[..fim]);
            GlobalUnlock(handle);
            Ok(texto)
        })();
        CloseClipboard();
        result
    }
}

/// Fora do Windows não há área de transferência de imagem.
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn get_image() -> Result<RgbaImage> {
    Err(err!("área de transferência disponível apenas no Windows"))
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn get_text() -> Result<String> {
    Err(err!("área de transferência disponível apenas no Windows"))
}

#[cfg(not(windows))]
pub fn set_image(_image: &RgbaImage) -> Result<()> {
    Err(err!("área de transferência disponível apenas no Windows"))
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn set_text(_text: &str) -> Result<()> {
    Err(err!("área de transferência disponível apenas no Windows"))
}
