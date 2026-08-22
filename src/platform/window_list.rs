//! Enumeração das janelas visíveis, para a captura por janela.
//!
//! Duas armadilhas do Win32 que este módulo existe para evitar:
//!
//! - `GetWindowRect` devolve o retângulo *com* a sombra que o DWM desenha em
//!   volta da janela — capturar por ele traria uma faixa transparente de
//!   vários pixels em cada lado. O retângulo real é o
//!   `DWMWA_EXTENDED_FRAME_BOUNDS`.
//! - Aplicativos da Store que estão suspensos continuam sendo janelas
//!   visíveis para o `IsWindowVisible`, mas não são desenhados em lugar
//!   nenhum. O DWM os marca como *cloaked*, e sem esse filtro eles aparecem
//!   na lista como retângulos fantasmas.

/// Uma janela candidata à captura, em px físicos do desktop virtual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTarget {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title: String,
}

impl WindowTarget {
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x + self.width as i32
            && y < self.y + self.height as i32
    }

    pub fn center(&self) -> (i32, i32) {
        (self.x + self.width as i32 / 2, self.y + self.height as i32 / 2)
    }
}

/// Janela sob o ponto, se houver.
///
/// Empata pela **menor área**: janelas se sobrepõem, e a menor entre as que
/// contêm o ponto é quase sempre a que está por cima — a alternativa seria
/// percorrer a ordem z, que a enumeração já entrega mas o transporte entre
/// processos perderia.
pub fn window_at(windows: &[WindowTarget], x: i32, y: i32) -> Option<usize> {
    windows
        .iter()
        .enumerate()
        .filter(|(_, w)| w.contains(x, y))
        .min_by_key(|(_, w)| w.area())
        .map(|(i, _)| i)
}

/// Próxima janela na direção pedida, a partir de `from`.
///
/// A pontuação soma a distância no eixo do movimento com o desvio lateral
/// pesado em 1,75: sem esse peso, uma janela muito deslocada para o lado
/// venceria outra bem à frente só por estar um pouco mais perto.
pub fn window_in_direction(
    windows: &[WindowTarget],
    from: Option<usize>,
    dx: i32,
    dy: i32,
) -> Option<usize> {
    let origin = match from {
        Some(i) => windows.get(i)?.center(),
        None => return windows.first().map(|_| 0),
    };
    windows
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != from)
        .filter_map(|(i, w)| {
            let c = w.center();
            let (delta_x, delta_y) = (c.0 - origin.0, c.1 - origin.1);
            // Só conta quem está do lado para onde se pediu.
            let (along, across) = match (dx, dy) {
                (d, 0) if d < 0 && delta_x < 0 => (-delta_x, delta_y.abs()),
                (d, 0) if d > 0 && delta_x > 0 => (delta_x, delta_y.abs()),
                (0, d) if d < 0 && delta_y < 0 => (-delta_y, delta_x.abs()),
                (0, d) if d > 0 && delta_y > 0 => (delta_y, delta_x.abs()),
                _ => return None,
            };
            Some((i, along as i64 + (across as f32 * 1.75) as i64))
        })
        .min_by_key(|(_, score)| *score)
        .map(|(i, _)| i)
}

#[cfg(windows)]
mod imp {
    use super::WindowTarget;
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, TRUE};
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
        IsIconic, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    /// `DWMWA_EXTENDED_FRAME_BOUNDS` — o retângulo sem a sombra.
    const EXTENDED_FRAME_BOUNDS: u32 = 9;

    /// Menor lado aceitável, em px: abaixo disso é decoração, não janela.
    const MIN_SIDE: i32 = 32;

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` é o `Vec` que `visible_windows` passou, vivo
        // durante toda a enumeração.
        let list = unsafe { &mut *(lparam as *mut Vec<WindowTarget>) };
        if let Some(target) = unsafe { describe(hwnd) } {
            list.push(target);
        }
        TRUE
    }

    /// Descreve a janela, ou `None` se ela não deve entrar na lista.
    unsafe fn describe(hwnd: HWND) -> Option<WindowTarget> {
        if unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { IsIconic(hwnd) } != 0 {
            return None;
        }
        // Janelas de ferramenta (paletas flutuantes, dicas) não são alvos.
        let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        if ex_style & WS_EX_TOOLWINDOW != 0 {
            return None;
        }
        if unsafe { is_cloaked(hwnd) } {
            return None;
        }

        let title = unsafe { window_title(hwnd) };
        if title.is_empty() {
            return None;
        }

        let rect = unsafe { frame_bounds(hwnd) }?;
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        if width < MIN_SIDE || height < MIN_SIDE {
            return None;
        }
        Some(WindowTarget {
            x: rect.left,
            y: rect.top,
            width: width as u32,
            height: height as u32,
            title,
        })
    }

    /// Aplicativo suspenso que o DWM não desenha em lugar nenhum.
    unsafe fn is_cloaked(hwnd: HWND) -> bool {
        let mut cloaked: u32 = 0;
        let ok = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED as u32,
                &mut cloaked as *mut u32 as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        ok == 0 && cloaked != 0
    }

    /// Retângulo real da janela, sem a sombra do DWM. Cai no
    /// `GetWindowRect` quando o DWM não responde (janelas muito antigas).
    unsafe fn frame_bounds(hwnd: HWND) -> Option<RECT> {
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let ok = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                EXTENDED_FRAME_BOUNDS,
                &mut rect as *mut RECT as *mut core::ffi::c_void,
                std::mem::size_of::<RECT>() as u32,
            )
        };
        if ok != 0 && unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return None;
        }
        Some(rect)
    }

    unsafe fn window_title(hwnd: HWND) -> String {
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; len as usize + 1];
        let written = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        if written <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..written as usize])
    }

    /// Janelas visíveis, da mais ao fundo para a mais à frente.
    pub fn visible_windows() -> Vec<WindowTarget> {
        let mut list: Vec<WindowTarget> = Vec::new();
        // SAFETY: o ponteiro para `list` vive além da chamada, que é síncrona.
        unsafe {
            EnumWindows(Some(enum_proc), &mut list as *mut Vec<WindowTarget> as LPARAM);
        }
        list
    }
}

#[cfg(windows)]
pub use imp::visible_windows;

/// Fora do Windows não há o que enumerar: só o residente chama isto, e ele
/// não existe nas outras plataformas.
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn visible_windows() -> Vec<WindowTarget> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(x: i32, y: i32, w: u32, h: u32) -> WindowTarget {
        WindowTarget { x, y, width: w, height: h, title: format!("{x},{y}") }
    }

    #[test]
    fn the_smallest_window_under_the_pointer_wins() {
        // Uma janelinha sobre uma maior: clicar nela tem de pegar a de cima.
        let windows = vec![win(0, 0, 800, 600), win(100, 100, 200, 150)];
        assert_eq!(window_at(&windows, 150, 150), Some(1));
        assert_eq!(window_at(&windows, 500, 500), Some(0));
        assert_eq!(window_at(&windows, 900, 900), None);
    }

    #[test]
    fn navigation_only_considers_windows_on_the_right_side() {
        let windows = vec![win(0, 0, 100, 100), win(200, 0, 100, 100)];
        assert_eq!(window_in_direction(&windows, Some(0), 1, 0), Some(1), "há uma à direita");
        assert_eq!(window_in_direction(&windows, Some(0), -1, 0), None, "nada à esquerda");
    }

    #[test]
    fn navigation_prefers_what_is_straight_ahead() {
        // A candidata alinhada vence a que está um pouco mais perto porém
        // muito deslocada para o lado.
        let windows = vec![
            win(0, 0, 100, 100),          // origem
            win(300, 0, 100, 100),        // à frente, alinhada
            win(260, 600, 100, 100),      // um pouco mais perto, mas lá longe
        ];
        assert_eq!(window_in_direction(&windows, Some(0), 1, 0), Some(1));
    }

    #[test]
    fn navigation_without_a_starting_window_takes_the_first() {
        let windows = vec![win(0, 0, 100, 100), win(200, 0, 100, 100)];
        assert_eq!(window_in_direction(&windows, None, 1, 0), Some(0));
        assert_eq!(window_in_direction(&[], None, 1, 0), None);
    }

    #[test]
    fn a_window_contains_its_own_corner_but_not_the_far_edge() {
        let w = win(10, 20, 100, 50);
        assert!(w.contains(10, 20), "canto superior esquerdo está dentro");
        assert!(!w.contains(110, 70), "a borda oposta já é a de fora");
        assert!(w.contains(109, 69));
    }
}
