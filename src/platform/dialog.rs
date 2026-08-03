//! Seletor nativo de pasta (substitui `rfd`): `SHBrowseForFolderW` com o
//! estilo novo (redimensionável, botão "Nova pasta").
//!
//! Chamado na thread do event loop, que já bombeia mensagens — o diálogo é
//! modal e roda seu próprio loop, como fazia o `rfd`.

use std::path::PathBuf;

#[cfg(windows)]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS,
        BROWSEINFOW,
    };

    let title = super::wide(title);
    let mut display: [u16; 260] = [0; 260];

    // SAFETY: BROWSEINFOW aponta para buffers válidos durante a chamada;
    // o PIDL retornado é liberado com CoTaskMemFree. CoInitializeEx é
    // idempotente na thread (RPC_E_CHANGED_MODE é ignorável — o loop do
    // winit já pode ter inicializado COM em outro modo).
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);

        let info = BROWSEINFOW {
            hwndOwner: std::ptr::null_mut(),
            pidlRoot: std::ptr::null_mut(),
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: None,
            lParam: 0,
            iImage: 0,
        };
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None; // cancelado
        }

        let mut path: [u16; 1024] = [0; 1024];
        let ok = SHGetPathFromIDListW(pidl, path.as_mut_ptr());
        CoTaskMemFree(pidl as *const core::ffi::c_void);
        if ok == 0 {
            return None;
        }
        let len = path.iter().position(|&c| c == 0).unwrap_or(0);
        Some(PathBuf::from(String::from_utf16_lossy(&path[..len])))
    }
}

#[cfg(not(windows))]
pub fn pick_folder(_title: &str) -> Option<PathBuf> {
    None
}
