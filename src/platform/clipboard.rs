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
                for (s, d) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
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

#[cfg(not(windows))]
pub fn set_image(_image: &RgbaImage) -> Result<()> {
    Err(err!("área de transferência disponível apenas no Windows"))
}
