//! Instância única via mutex nomeado (substitui `single-instance`, RF-08).

/// Guarda o mutex vivo pelo tempo de vida do processo.
pub struct InstanceGuard {
    #[cfg(windows)]
    handle: isize,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: handle veio de CreateMutexW e só é fechado aqui.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
    }
}

/// `Some(guard)` quando esta é a primeira instância; `None` quando já há
/// outra em execução. Erros de criação contam como "primeira" (pior caso:
/// duas instâncias — mesmo comportamento do crate substituído).
#[cfg(windows)]
pub fn acquire(name: &str) -> Option<InstanceGuard> {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let wide_name = super::wide(&format!("Local\\{name}"));
    // SAFETY: string NUL-terminada válida; handle é verificado.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, wide_name.as_ptr());
        if handle.is_null() {
            log::warn!("CreateMutexW falhou ({})", GetLastError());
            return Some(InstanceGuard { handle: 0 });
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            windows_sys::Win32::Foundation::CloseHandle(handle);
            return None;
        }
        Some(InstanceGuard { handle: handle as isize })
    }
}

#[cfg(not(windows))]
pub fn acquire(_name: &str) -> Option<InstanceGuard> {
    Some(InstanceGuard {})
}
