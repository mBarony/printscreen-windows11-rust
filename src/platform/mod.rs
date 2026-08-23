//! Camada de plataforma: implementações Win32 diretas (via `windows-sys`)
//! do que antes vinha de crates de terceiros — somente o necessário para o
//! Windows 11 (§ pedido v1.3: código standalone).
//!
//! | Módulo | Substitui |
//! |---|---|
//! | `shell` | `tray-icon` + `muda` + `notify-rust` + `global-hotkey` |
//! | `capture` | `xcap` |
//! | `clipboard` | `arboard` |
//! | `autostart` | `auto-launch` |
//! | `instance` | `single-instance` |
//! | `dialog` | `rfd` |
//! | `folders` | `dirs` |
//! | `time` | `chrono` |
//! | `logger` | `simplelog` |
//!
//! Em hosts não-Windows (usados só para rodar os testes de lógica) cada
//! módulo compila com stubs inertes.

pub mod autostart;
pub mod capture;
pub mod clipboard;
pub mod dialog;
pub mod folders;
pub mod imagefile;
pub mod instance;
// Transporte entre processos: só existe no Windows (e sob teste, que exercita a
// serialização pura em qualquer host).
#[cfg(any(windows, test))]
pub mod ipc;
pub mod logger;
pub mod memory;
pub mod msgbox;
#[cfg(feature = "ocr")]
pub mod ocr;
pub mod shell;
pub mod time;
pub mod version;
pub mod window_list;

/// UTF-16 terminado em NUL para APIs W do Win32.
#[cfg(windows)]
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copia `text` para um buffer UTF-16 de tamanho fixo (truncando com NUL).
#[cfg(windows)]
pub(crate) fn fill_wide(dst: &mut [u16], text: &str) {
    let mut i = 0;
    for unit in text.encode_utf16() {
        if i + 1 >= dst.len() {
            break;
        }
        dst[i] = unit;
        i += 1;
    }
    dst[i] = 0;
}
