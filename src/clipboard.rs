//! Cópia de imagem para a área de transferência via `arboard`.
//!
//! O clipboard do Windows pode estar momentaneamente bloqueado por outro
//! processo; por isso a escrita tenta até 3 vezes com 100 ms de intervalo
//! (§8 da especificação).

use std::borrow::Cow;
use std::time::Duration;

use anyhow::{anyhow, Result};
use image::RgbaImage;

const ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(100);

/// Copia uma imagem RGBA (não pré-multiplicada) para a área de transferência.
pub fn copy_image(image: &RgbaImage) -> Result<()> {
    let data = arboard::ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        bytes: Cow::Borrowed(image.as_raw()),
    };

    let mut last_err = None;
    for attempt in 1..=ATTEMPTS {
        let result = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_image(data.clone()));
        match result {
            Ok(()) => {
                log::info!(
                    "imagem {}x{} copiada para a área de transferência (tentativa {attempt})",
                    image.width(),
                    image.height()
                );
                return Ok(());
            }
            Err(err) => {
                log::warn!("clipboard tentativa {attempt}/{ATTEMPTS} falhou: {err}");
                last_err = Some(err);
                if attempt < ATTEMPTS {
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }
    }
    Err(anyhow!(
        "área de transferência indisponível: {}",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}
