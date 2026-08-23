//! Enumeração de monitores e captura de tela via GDI (substitui `xcap`).
//!
//! Com o manifesto Per-Monitor V2 embutido, `EnumDisplayMonitors` reporta
//! retângulos em **pixels físicos** do desktop virtual e o `BitBlt` do DC de
//! tela captura 1:1 — exatamente o contrato que o restante do app espera.

use crate::error::{err, Result};
use crate::imgbuf::RgbaImage;

/// Um monitor com geometria física e a captura congelada.
pub struct CapturedMonitor {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// 1.0 = 100%, 1.5 = 150%…
    pub scale: f32,
    pub is_primary: bool,
    pub image: RgbaImage,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::error::Context as _;
    use windows_sys::Win32::Foundation::{LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
        EnumDisplayMonitors, GetDC, GetMonitorInfoW, MonitorFromPoint, ReleaseDC, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HDC, HMONITOR,
        MONITORINFO, MONITOR_DEFAULTTONEAREST, SRCCOPY,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    const MONITORINFOF_PRIMARY: u32 = 1;

    struct MonitorGeom {
        handle: HMONITOR,
        rect: RECT,
        primary: bool,
        scale: f32,
    }

    unsafe extern "system" fn enum_proc(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> i32 {
        let list = &mut *(lparam as *mut Vec<MonitorGeom>);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            rcWork: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            dwFlags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            let mut dpi_x = 96u32;
            let mut dpi_y = 96u32;
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) != 0 {
                dpi_x = 96;
            }
            list.push(MonitorGeom {
                handle: monitor,
                rect: info.rcMonitor,
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
                scale: dpi_x as f32 / 96.0,
            });
        }
        1 // continua a enumeração
    }

    fn enumerate() -> Result<Vec<MonitorGeom>> {
        let mut list: Vec<MonitorGeom> = Vec::new();
        // SAFETY: o callback só escreve no Vec apontado pelo lparam durante
        // a chamada síncrona de EnumDisplayMonitors.
        let ok = unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(enum_proc),
                &mut list as *mut _ as LPARAM,
            )
        };
        if ok == 0 || list.is_empty() {
            return Err(err!("nenhum monitor encontrado"));
        }
        Ok(list)
    }

    /// Captura o retângulo físico `rect` do desktop virtual.
    fn capture_rect(rect: &RECT) -> Result<RgbaImage> {
        let width = (rect.right - rect.left) as u32;
        let height = (rect.bottom - rect.top) as u32;
        if width == 0 || height == 0 {
            return Err(err!("monitor com área zero"));
        }

        // SAFETY: recursos GDI criados e liberados aqui; a DIB section dá um
        // ponteiro válido de `width*height*4` bytes enquanto o HBITMAP vive.
        unsafe {
            let screen_dc = GetDC(std::ptr::null_mut());
            if screen_dc.is_null() {
                return Err(err!("GetDC falhou"));
            }
            let mem_dc = CreateCompatibleDC(screen_dc);

            let header = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // negativo = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let info = BITMAPINFO { bmiHeader: header, bmiColors: [std::mem::zeroed()] };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(mem_dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(mem_dc);
                ReleaseDC(std::ptr::null_mut(), screen_dc);
                return Err(err!("CreateDIBSection falhou"));
            }

            let previous = SelectObject(mem_dc, bitmap as _);
            let ok = BitBlt(
                mem_dc,
                0,
                0,
                width as i32,
                height as i32,
                screen_dc,
                rect.left,
                rect.top,
                SRCCOPY | CAPTUREBLT,
            );
            SelectObject(mem_dc, previous);

            let result = if ok == 0 {
                Err(err!("BitBlt falhou"))
            } else {
                // BGRX → RGBA (alfa opaco).
                let src = std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
                let mut pixels = vec![0u8; src.len()];
                let (src_px, _) = src.as_chunks::<4>();
                let (dst_px, _) = pixels.as_chunks_mut::<4>();
                for (s, d) in src_px.iter().zip(dst_px.iter_mut()) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
                Ok(RgbaImage::from_raw(width, height, pixels))
            };

            DeleteObject(bitmap as _);
            DeleteDC(mem_dc);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            result
        }
    }

    pub fn all_monitors() -> Result<Vec<CapturedMonitor>> {
        let mut out = Vec::new();
        for geom in enumerate()? {
            let image = capture_rect(&geom.rect).context("capturando monitor")?;
            out.push(CapturedMonitor {
                x: geom.rect.left,
                y: geom.rect.top,
                width: image.width(),
                height: image.height(),
                scale: geom.scale.max(0.5),
                is_primary: geom.primary,
                image,
            });
        }
        Ok(out)
    }

    pub fn monitor_at(x: i32, y: i32) -> Result<CapturedMonitor> {
        // SAFETY: MonitorFromPoint não falha com DEFAULTTONEAREST.
        let target = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
        let monitors = enumerate()?;
        let geom = monitors
            .into_iter()
            .find(|m| std::ptr::eq(m.handle, target))
            .context("monitor sob o cursor não encontrado")?;
        let image = capture_rect(&geom.rect)?;
        Ok(CapturedMonitor {
            x: geom.rect.left,
            y: geom.rect.top,
            width: image.width(),
            height: image.height(),
            scale: geom.scale.max(0.5),
            is_primary: geom.primary,
            image,
        })
    }

    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: GetCursorPos escreve em um POINT válido.
        let ok = unsafe { GetCursorPos(&mut point) };
        (ok != 0).then_some((point.x, point.y))
    }
}

#[cfg(windows)]
pub use imp::{all_monitors, cursor_pos, monitor_at};

#[cfg(not(windows))]
pub fn all_monitors() -> Result<Vec<CapturedMonitor>> {
    Err(err!("captura de tela disponível apenas no Windows"))
}

#[cfg(not(windows))]
pub fn monitor_at(_x: i32, _y: i32) -> Result<CapturedMonitor> {
    Err(err!("captura de tela disponível apenas no Windows"))
}

#[cfg(not(windows))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}
