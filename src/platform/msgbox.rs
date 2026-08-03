//! Caixa de mensagem nativa — usada pela segunda instância (RF-08), que
//! encerra em seguida e não tem bandeja para exibir balão.

#[cfg(windows)]
pub fn info(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };

    let title = super::wide(title);
    let text = super::wide(text);
    // SAFETY: strings NUL-terminadas válidas durante a chamada (modal).
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

#[cfg(not(windows))]
pub fn info(title: &str, text: &str) {
    eprintln!("[{title}] {text}");
}
