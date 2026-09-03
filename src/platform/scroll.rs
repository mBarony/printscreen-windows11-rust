//! Rolar a janela de outro programa, para a captura com rolagem.
//!
//! Não há API que role uma janela alheia e diga quanto rolou. O que existe é
//! mandar a ela a mesma mensagem que a roda do mouse mandaria e observar o
//! resultado nos pixels — que é o que o `stitch` faz.
//!
//! A mensagem vai para o controle **mais fundo** sob o ponto, e não para a
//! janela de topo: numa janela com painéis, quem rola é o controle sob o
//! cursor, e o de topo costuma ignorar a roda.

// Fora do Windows o módulo existe como stub, para o resto do código
// compilar; ninguém o chama.
#![cfg_attr(not(windows), allow(dead_code))]

/// Um "clique" da roda, na unidade do Windows.
#[cfg(windows)]
const WHEEL_DELTA: i16 = 120;

/// Manda `notches` cliques de roda para o que estiver em `(x, y)`, em px de
/// tela. Positivo rola para cima; negativo, para baixo.
///
/// Devolve `false` quando não há janela nenhuma no ponto.
#[cfg(windows)]
pub fn wheel_at(x: i32, y: i32, notches: i32) -> bool {
    use windows_sys::Win32::Foundation::{LPARAM, POINT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WindowFromPoint, WM_MOUSEWHEEL};

    // SAFETY: chamadas simples de Win32 com um ponto por valor; a HWND
    // devolvida é usada só como destino de `PostMessageW`, que a valida.
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.is_null() {
            return false;
        }
        let delta = (notches * WHEEL_DELTA as i32).clamp(i16::MIN as i32, i16::MAX as i32);
        // wParam: delta na palavra alta, teclas modificadoras na baixa.
        let wparam = ((delta << 16) as u32) as WPARAM;
        // lParam: as coordenadas são de **tela**, não da janela.
        let lparam = (((y as u32) << 16) | (x as u32 & 0xFFFF)) as LPARAM;
        PostMessageW(hwnd, WM_MOUSEWHEEL, wparam, lparam) != 0
    }
}

#[cfg(not(windows))]
pub fn wheel_at(_x: i32, _y: i32, _notches: i32) -> bool {
    false
}
