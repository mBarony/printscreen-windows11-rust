//! Enumeração de monitores e captura de tela (GDI via `platform::capture`),
//! composição multi-monitor e recorte (§9 da especificação).
//!
//! Todas as coordenadas e dimensões são em **pixels físicos** do desktop
//! virtual. Com o manifesto Per-Monitor V2 embutido, as APIs do Windows
//! retornam coordenadas físicas consistentes mesmo com escalas mistas.
//! Coordenadas negativas (monitor à esquerda/acima do principal) são
//! normalizadas na composição subtraindo o mínimo.

use crate::error::{err, Result};
use crate::imgbuf::RgbaImage;
use crate::platform::capture as sys;

use crate::config::FullscreenScope;

/// Captura congelada de um monitor, com sua geometria no desktop virtual.
pub struct MonitorShot {
    /// Origem física no desktop virtual (pode ser negativa).
    pub x: i32,
    pub y: i32,
    /// Dimensões físicas em px (== dimensões da imagem).
    pub width: u32,
    pub height: u32,
    /// Fator de escala do monitor (1.0 = 100%, 1.5 = 150%…).
    pub scale: f32,
    pub image: RgbaImage,
}

impl From<sys::CapturedMonitor> for MonitorShot {
    fn from(m: sys::CapturedMonitor) -> Self {
        Self {
            x: m.x,
            y: m.y,
            width: m.width,
            height: m.height,
            scale: m.scale,
            image: m.image,
        }
    }
}

/// Captura todos os monitores (congela o conteúdo da tela para o overlay).
pub fn capture_all_monitors() -> Result<Vec<MonitorShot>> {
    Ok(sys::all_monitors()?.into_iter().map(MonitorShot::from).collect())
}

/// Compõe as capturas na área virtual completa (bounding box de todos os
/// monitores); áreas não cobertas ficam pretas (layouts em "L").
pub fn compose_virtual(shots: &[MonitorShot]) -> Result<RgbaImage> {
    let min_x = shots.iter().map(|s| s.x).min().unwrap_or(0);
    let min_y = shots.iter().map(|s| s.y).min().unwrap_or(0);
    let max_x = shots.iter().map(|s| s.x as i64 + s.width as i64).max().unwrap_or(0);
    let max_y = shots.iter().map(|s| s.y as i64 + s.height as i64).max().unwrap_or(0);

    let total_w = (max_x - min_x as i64) as u32;
    let total_h = (max_y - min_y as i64) as u32;
    if total_w == 0 || total_h == 0 {
        return Err(err!("área virtual vazia"));
    }

    // Fundo preto opaco.
    let mut canvas = RgbaImage::filled(total_w, total_h, [0, 0, 0, 255]);
    for shot in shots {
        canvas.paste(&shot.image, (shot.x - min_x) as i64, (shot.y - min_y) as i64);
    }
    Ok(canvas)
}

/// Captura de tela cheia conforme o escopo configurado (RF-01).
pub fn capture_fullscreen(scope: FullscreenScope) -> Result<RgbaImage> {
    match scope {
        FullscreenScope::AllMonitors => {
            let shots = capture_all_monitors()?;
            compose_virtual(&shots)
        }
        FullscreenScope::Primary => {
            let shots = sys::all_monitors()?;
            let primary = shots
                .into_iter()
                .find(|m| m.is_primary)
                .ok_or_else(|| err!("monitor principal não encontrado"))?;
            Ok(primary.image)
        }
        FullscreenScope::MonitorUnderCursor => {
            let monitor = match sys::cursor_pos() {
                Some((x, y)) => sys::monitor_at(x, y)?,
                None => sys::all_monitors()?
                    .into_iter()
                    .find(|m| m.is_primary)
                    .ok_or_else(|| err!("monitor principal não encontrado"))?,
            };
            Ok(monitor.image)
        }
    }
}

/// Recorta `rect` (x, y, w, h em px da imagem) de uma captura.
pub fn crop(image: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    image.crop(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_normalizes_negative_origins() {
        let shots = vec![
            MonitorShot {
                x: -4,
                y: 0,
                width: 4,
                height: 2,
                scale: 1.0,
                image: RgbaImage::filled(4, 2, [10, 0, 0, 255]),
            },
            MonitorShot {
                x: 0,
                y: 0,
                width: 3,
                height: 3,
                scale: 1.0,
                image: RgbaImage::filled(3, 3, [0, 20, 0, 255]),
            },
        ];
        let composed = compose_virtual(&shots).unwrap();
        assert_eq!((composed.width(), composed.height()), (7, 3));
        assert_eq!(composed.pixel(0, 0), [10, 0, 0, 255], "monitor à esquerda");
        assert_eq!(composed.pixel(4, 0), [0, 20, 0, 255], "monitor principal");
        // Área não coberta (layout em L): preta.
        assert_eq!(composed.pixel(0, 2), [0, 0, 0, 255]);
    }
}
