//! Leitura de arquivos de imagem (JPG, PNG, BMP, GIF, TIFF) pelo GDI+.
//!
//! O app grava JPG com o codificador embutido, mas para *abrir* uma imagem
//! qualquer seria preciso um decodificador — e escrever um por formato é
//! trabalho demais para o que se ganha. O GDI+ já vem no Windows e sua API é
//! C plana (`Gdip*`), sem as vtables COM que o WIC exigiria: dá para chamá-la
//! direto, sem dependência nova.
//!
//! O GDI+ precisa ser inicializado uma vez por processo, e desligá-lo enquanto
//! ainda há objetos vivos derruba o processo — por isso o token é criado sob
//! demanda e nunca liberado. É um processo efêmero: ele morre logo.

use crate::error::Result;
use crate::imgbuf::RgbaImage;
use std::path::Path;

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::error::err;
    use std::sync::Once;
    use windows_sys::Win32::Graphics::GdiPlus::{
        GdipBitmapLockBits, GdipBitmapUnlockBits, GdipCreateBitmapFromFile, GdipDisposeImage,
        GdipGetImageHeight, GdipGetImageWidth, GdiplusStartup, BitmapData, GdiplusStartupInput,
        GpBitmap, ImageLockModeRead, Ok as GdipOk,
    };

    /// `PixelFormat32bppARGB` do GdiPlusPixelFormats.h. O `windows-sys` traz
    /// as funções e os tipos do GDI+, mas não as constantes de formato.
    const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026_200A;

    /// Inicializa o GDI+ na primeira chamada.
    ///
    /// O `GdiplusShutdown` nunca é chamado de propósito: desligar a
    /// biblioteca com objetos vivos derruba o processo, e este processo é
    /// efêmero — o sistema recupera tudo quando ele sai.
    fn ensure_started() {
        static START: Once = Once::new();
        START.call_once(|| {
            let input = GdiplusStartupInput {
                GdiplusVersion: 1,
                DebugEventCallback: 0,
                SuppressBackgroundThread: 0,
                SuppressExternalCodecs: 0,
            };
            let mut token: usize = 0;
            // SAFETY: `input` é válido durante a chamada; `token` recebe o
            // identificador da sessão, que não precisamos guardar.
            unsafe {
                GdiplusStartup(&mut token, &input, std::ptr::null_mut());
            }
        });
    }

    pub fn load(path: &Path) -> Result<RgbaImage> {
        ensure_started();
        let wide = crate::platform::wide(&path.to_string_lossy());

        // SAFETY: cada objeto criado é liberado antes de sair, inclusive nos
        // caminhos de erro; os buffers passados vivem durante as chamadas.
        unsafe {
            let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
            if GdipCreateBitmapFromFile(wide.as_ptr(), &mut bitmap) != GdipOk || bitmap.is_null() {
                return Err(err!("não foi possível abrir a imagem: {}", path.display()));
            }
            let result = read_pixels(bitmap);
            GdipDisposeImage(bitmap.cast());
            result
        }
    }

    unsafe fn read_pixels(bitmap: *mut GpBitmap) -> Result<RgbaImage> {
        let (mut width, mut height) = (0u32, 0u32);
        unsafe {
            if GdipGetImageWidth(bitmap.cast(), &mut width) != GdipOk
                || GdipGetImageHeight(bitmap.cast(), &mut height) != GdipOk
            {
                return Err(err!("imagem sem dimensões utilizáveis"));
            }
        }
        if width == 0 || height == 0 {
            return Err(err!("imagem vazia"));
        }

        let mut data: BitmapData = unsafe { std::mem::zeroed() };
        let rect = windows_sys::Win32::Graphics::GdiPlus::Rect {
            X: 0,
            Y: 0,
            Width: width as i32,
            Height: height as i32,
        };
        unsafe {
            if GdipBitmapLockBits(
                bitmap,
                &rect,
                ImageLockModeRead as u32,
                PIXEL_FORMAT_32BPP_ARGB,
                &mut data,
            ) != GdipOk
            {
                return Err(err!("não foi possível ler os pixels da imagem"));
            }
        }

        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        // O GDI+ entrega BGRA pré-multiplicado por linha, com `Stride` que
        // pode ser negativo (imagem de baixo para cima).
        for row in 0..height {
            let offset = data.Stride as isize * row as isize;
            // SAFETY: a view está travada e cobre `Height` linhas de `Stride`
            // bytes; `offset` fica dentro dela por construção.
            let line = unsafe {
                std::slice::from_raw_parts(
                    (data.Scan0 as *const u8).offset(offset),
                    width as usize * 4,
                )
            };
            for px in line.chunks_exact(4) {
                pixels.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        }
        unsafe {
            GdipBitmapUnlockBits(bitmap, &mut data);
        }
        Ok(RgbaImage::from_raw(width, height, pixels))
    }
}

#[cfg(windows)]
pub use imp::load;

/// Abre uma imagem do disco.
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn load(_path: &Path) -> Result<RgbaImage> {
    Err(crate::error::err!("leitura de imagem disponível apenas no Windows"))
}
