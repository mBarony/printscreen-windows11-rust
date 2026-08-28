//! Cópia de imagem para a área de transferência (CF_DIB via
//! `platform::clipboard`).
//!
//! O clipboard do Windows pode estar momentaneamente bloqueado por outro
//! processo; por isso a escrita tenta até 3 vezes com 100 ms de intervalo
//! (§8 da especificação).

use std::time::Duration;

use crate::error::{err, Result};
use crate::imgbuf::RgbaImage;
use crate::platform;

const ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(100);

/// Copia uma imagem RGBA (não pré-multiplicada) para a área de transferência.
pub fn copy_image(image: &RgbaImage) -> Result<()> {
    let mut last_err = None;
    for attempt in 1..=ATTEMPTS {
        match platform::clipboard::set_image(image) {
            Ok(()) => {
                log::info!(
                    "imagem {}x{} copiada para a área de transferência (tentativa {attempt})",
                    image.width(),
                    image.height()
                );
                return Ok(());
            }
            Err(error) => {
                log::warn!("clipboard tentativa {attempt}/{ATTEMPTS} falhou: {error}");
                last_err = Some(error);
                if attempt < ATTEMPTS {
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }
    }
    Err(err!(
        "área de transferência indisponível: {}",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Texto da área de transferência, ou `None` se não houver nenhum.
///
/// Sem repetição: quem lê está respondendo a uma tecla, e insistir por 300 ms
/// travaria o quadro. Não haver texto é o caso comum, não é erro.
pub fn read_text() -> Option<String> {
    match platform::clipboard::get_text() {
        Ok(texto) => Some(texto),
        Err(error) => {
            log::debug!("sem texto na área de transferência: {error}");
            None
        }
    }
}

/// Copia texto para a área de transferência, com a mesma política de
/// repetição da imagem — quem trava o clipboard trava para os dois formatos.
pub fn copy_text(text: &str) -> Result<()> {
    let mut last_err = None;
    for attempt in 1..=ATTEMPTS {
        match platform::clipboard::set_text(text) {
            Ok(()) => {
                log::info!(
                    "{} caracteres copiados para a área de transferência (tentativa {attempt})",
                    text.chars().count()
                );
                return Ok(());
            }
            Err(error) => {
                log::warn!("clipboard tentativa {attempt}/{ATTEMPTS} falhou: {error}");
                last_err = Some(error);
                if attempt < ATTEMPTS {
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }
    }
    Err(err!(
        "área de transferência indisponível: {}",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}
