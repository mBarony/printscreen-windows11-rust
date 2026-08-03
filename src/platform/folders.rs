//! Pasta Imagens do usuário (substitui `dirs`).

use std::path::PathBuf;

/// `FOLDERID_Pictures` — respeita redirecionamento (ex.: OneDrive).
#[cfg(windows)]
pub fn pictures_dir() -> Option<PathBuf> {
    use windows_sys::core::GUID;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;

    // {33E28130-4E1E-4676-835A-98395C3BC3BB}
    const FOLDERID_PICTURES: GUID = GUID {
        data1: 0x33E2_8130,
        data2: 0x4E1E,
        data3: 0x4676,
        data4: [0x83, 0x5A, 0x98, 0x39, 0x5C, 0x3B, 0xC3, 0xBB],
    };

    let mut path_ptr: *mut u16 = std::ptr::null_mut();
    // SAFETY: ponteiro de saída válido; em sucesso o SO aloca a string, que
    // é copiada e liberada com CoTaskMemFree (exigência da API).
    unsafe {
        let hr = SHGetKnownFolderPath(&FOLDERID_PICTURES, 0, std::ptr::null_mut(), &mut path_ptr);
        if hr < 0 || path_ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(path_ptr, len));
        CoTaskMemFree(path_ptr as *const core::ffi::c_void);
        Some(PathBuf::from(text))
    }
}

#[cfg(not(windows))]
pub fn pictures_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Pictures"))
}
