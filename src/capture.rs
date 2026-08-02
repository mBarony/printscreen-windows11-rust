//! Enumeração de monitores e captura de tela (`xcap`), composição
//! multi-monitor e recorte (§9 da especificação).
//!
//! Todas as coordenadas e dimensões são em **pixels físicos** do desktop
//! virtual. Com o manifesto Per-Monitor V2 embutido, as APIs do Windows
//! retornam coordenadas físicas consistentes mesmo com escalas mistas.
//! Coordenadas negativas (monitor à esquerda/acima do principal) são
//! normalizadas na composição subtraindo o mínimo.

use anyhow::{anyhow, Context as _, Result};
use image::RgbaImage;
use xcap::Monitor;

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

impl MonitorShot {
    fn from_monitor(monitor: &Monitor) -> Result<Self> {
        let image = monitor
            .capture_image()
            .context("capturando imagem do monitor")?;
        // Dimensões da própria captura são a fonte de verdade em px físicos;
        // width()/height() do xcap são usados apenas como fallback.
        let width = image.width();
        let height = image.height();
        Ok(Self {
            x: monitor.x().unwrap_or(0),
            y: monitor.y().unwrap_or(0),
            width,
            height,
            scale: monitor.scale_factor().unwrap_or(1.0).max(0.5),
            image,
        })
    }
}

/// Captura todos os monitores (congela o conteúdo da tela para o overlay).
pub fn capture_all_monitors() -> Result<Vec<MonitorShot>> {
    let monitors = Monitor::all().context("enumerando monitores")?;
    if monitors.is_empty() {
        return Err(anyhow!("nenhum monitor encontrado"));
    }
    let mut shots = Vec::with_capacity(monitors.len());
    for monitor in &monitors {
        shots.push(MonitorShot::from_monitor(monitor)?);
    }
    Ok(shots)
}

/// Compõe as capturas na área virtual completa (bounding box de todos os
/// monitores); áreas não cobertas ficam pretas (layouts em "L").
pub fn compose_virtual(shots: &[MonitorShot]) -> Result<RgbaImage> {
    let min_x = shots.iter().map(|s| s.x).min().unwrap_or(0);
    let min_y = shots.iter().map(|s| s.y).min().unwrap_or(0);
    let max_x = shots
        .iter()
        .map(|s| s.x as i64 + s.width as i64)
        .max()
        .unwrap_or(0);
    let max_y = shots
        .iter()
        .map(|s| s.y as i64 + s.height as i64)
        .max()
        .unwrap_or(0);

    let total_w = (max_x - min_x as i64) as u32;
    let total_h = (max_y - min_y as i64) as u32;
    if total_w == 0 || total_h == 0 {
        return Err(anyhow!("área virtual vazia"));
    }

    // Fundo preto opaco.
    let mut canvas = RgbaImage::from_pixel(total_w, total_h, image::Rgba([0, 0, 0, 255]));
    for shot in shots {
        let dst_x = (shot.x - min_x) as i64;
        let dst_y = (shot.y - min_y) as i64;
        image::imageops::replace(&mut canvas, &shot.image, dst_x, dst_y);
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
            let monitors = Monitor::all().context("enumerando monitores")?;
            let primary = monitors
                .iter()
                .find(|m| m.is_primary().unwrap_or(false))
                .or_else(|| monitors.first())
                .ok_or_else(|| anyhow!("nenhum monitor encontrado"))?;
            primary.capture_image().context("capturando monitor principal")
        }
        FullscreenScope::MonitorUnderCursor => {
            let monitor = match cursor_pos() {
                Some((x, y)) => Monitor::from_point(x, y).or_else(|_| {
                    // Cursor em área sem monitor (ex.: acabou de desconectar):
                    // recua para o principal.
                    primary_monitor()
                })?,
                None => primary_monitor()?,
            };
            monitor.capture_image().context("capturando monitor sob o cursor")
        }
    }
}

fn primary_monitor() -> Result<Monitor> {
    let monitors = Monitor::all().context("enumerando monitores")?;
    monitors
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .ok_or_else(|| anyhow!("monitor principal não encontrado"))
}

/// Recorta `rect` (x, y, w, h em px da imagem) de uma captura.
pub fn crop(image: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    image::imageops::crop_imm(image, x, y, w, h).to_image()
}

/// Posição física do cursor no desktop virtual.
#[cfg(windows)]
pub fn cursor_pos() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: GetCursorPos escreve em um POINT válido; retorno 0 = falha.
    let ok = unsafe { GetCursorPos(&mut point) };
    (ok != 0).then_some((point.x, point.y))
}

#[cfg(not(windows))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}
