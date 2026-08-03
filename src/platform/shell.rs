//! Janela oculta de shell: ícone da bandeja + menu de contexto + balões de
//! notificação + atalhos globais — em um único `WndProc`.
//!
//! Substitui `tray-icon`, `muda`, `notify-rust` e `global-hotkey`. A janela
//! precisa ser criada na thread do event loop (o winit bombeia as mensagens
//! dela); os eventos são entregues ao app pelo `handler` registrado no
//! `init`, que roda dentro do pump — deve apenas enfileirar e retornar.
//!
//! Balões substituem os toasts WinRT: aparecem como notificações nativas no
//! Windows 10/11 e, bônus, exibem o nome/ícone do RustShot em vez de
//! "Windows PowerShell" (limitação antiga de exe sem AUMID registrado).

#![cfg_attr(not(windows), allow(dead_code, unused_variables))]

use crate::error::Result;

#[derive(Debug, Clone, Copy)]
pub enum ShellEvent {
    /// Item de menu clicado (id definido em `MenuEntry`).
    Menu(u16),
    /// Atalho global disparado (id passado em `register_hotkey`).
    Hotkey(i32),
}

pub enum MenuEntry {
    Item { id: u16, label: &'static str },
    Check { id: u16, label: &'static str, checked: bool },
    Separator,
}

pub type EventHandler = Box<dyn Fn(ShellEvent) + Send + Sync>;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use std::sync::{Mutex, OnceLock};

    use crate::error::err;
    use crate::platform::{fill_wide, wide};
    use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
        NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CheckMenuItem, CreateIconIndirect, CreatePopupMenu, CreateWindowExW,
        DefWindowProcW, GetCursorPos, PostMessageW, RegisterClassW, RegisterWindowMessageW,
        SetForegroundWindow, TrackPopupMenuEx, HICON, ICONINFO, MF_BYCOMMAND, MF_CHECKED,
        MF_SEPARATOR, MF_STRING, MF_UNCHECKED, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP,
        WM_RBUTTONUP, WNDCLASSW,
    };

    const TRAY_ICON_ID: u32 = 1;
    const WM_APP_TRAY: u32 = WM_APP + 1;
    const WM_APP_BALLOON: u32 = WM_APP + 2;
    const WM_APP_SETCHECK: u32 = WM_APP + 3;
    const NIIF_USER: u32 = 0x0000_0004;
    const NIIF_LARGE_ICON: u32 = 0x0000_0020;

    static HWND_SHELL: AtomicIsize = AtomicIsize::new(0);
    static HMENU_SHELL: AtomicIsize = AtomicIsize::new(0);
    static HICON_SHELL: AtomicIsize = AtomicIsize::new(0);
    static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
    static HANDLER: OnceLock<EventHandler> = OnceLock::new();
    static BALLOONS: Mutex<VecDeque<(String, String)>> = Mutex::new(VecDeque::new());
    static TOOLTIP: Mutex<String> = Mutex::new(String::new());

    /// Cria a janela, o menu e o ícone da bandeja. Chamar uma única vez, na
    /// thread do event loop.
    pub fn init(
        tooltip: &str,
        icon_rgba: (&[u8], u32, u32),
        menu: &[MenuEntry],
        handler: EventHandler,
    ) -> Result<()> {
        if HWND_SHELL.load(Ordering::SeqCst) != 0 {
            return Err(err!("shell já inicializado"));
        }
        let _ = HANDLER.set(handler);
        *TOOLTIP.lock().unwrap() = tooltip.to_string();

        // SAFETY: sequência clássica RegisterClass/CreateWindow; strings e
        // structs vivem durante as chamadas. O WndProc só usa estado 'static.
        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let class_name = wide("RustShotShellWindow");
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            if RegisterClassW(&wc) == 0 {
                return Err(err!("RegisterClassW falhou ({})", GetLastError()));
            }

            let title = wide("RustShotShell");
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                0, // sem WS_VISIBLE: janela oculta, só para mensagens
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return Err(err!("CreateWindowExW falhou ({})", GetLastError()));
            }
            HWND_SHELL.store(hwnd as isize, Ordering::SeqCst);

            // Menu de contexto.
            let hmenu = CreatePopupMenu();
            if hmenu.is_null() {
                return Err(err!("CreatePopupMenu falhou"));
            }
            for entry in menu {
                match entry {
                    MenuEntry::Item { id, label } => {
                        let text = wide(label);
                        AppendMenuW(hmenu, MF_STRING, *id as usize, text.as_ptr());
                    }
                    MenuEntry::Check { id, label, checked } => {
                        let text = wide(label);
                        let flags = if *checked { MF_STRING | MF_CHECKED } else { MF_STRING };
                        AppendMenuW(hmenu, flags, *id as usize, text.as_ptr());
                    }
                    MenuEntry::Separator => {
                        AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
                    }
                }
            }
            HMENU_SHELL.store(hmenu as isize, Ordering::SeqCst);

            // Ícone (usado na bandeja e nos balões).
            let hicon = create_icon(icon_rgba.0, icon_rgba.1, icon_rgba.2)?;
            HICON_SHELL.store(hicon as isize, Ordering::SeqCst);

            // Re-adicionar o ícone se o Explorer reiniciar.
            let msg = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
            TASKBAR_CREATED.store(msg, Ordering::SeqCst);

            add_tray_icon(hwnd, hicon)?;
        }
        Ok(())
    }

    unsafe fn add_tray_icon(hwnd: HWND, hicon: HICON) -> Result<()> {
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ICON_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_APP_TRAY;
        data.hIcon = hicon;
        fill_wide(&mut data.szTip, &TOOLTIP.lock().unwrap());
        if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
            return Err(err!("Shell_NotifyIconW(NIM_ADD) falhou"));
        }
        Ok(())
    }

    /// RGBA → HICON (bitmap de cor 32 bpp + máscara vazia).
    unsafe fn create_icon(rgba: &[u8], width: u32, height: u32) -> Result<HICON> {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        let mut bgra = Vec::with_capacity(rgba.len());
        for px in rgba.chunks_exact(4) {
            bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        let color = CreateBitmap(
            width as i32,
            height as i32,
            1,
            32,
            bgra.as_ptr() as *const core::ffi::c_void,
        );
        let mask_bits = vec![0u8; (width.div_ceil(8) * height) as usize];
        let mask = CreateBitmap(
            width as i32,
            height as i32,
            1,
            1,
            mask_bits.as_ptr() as *const core::ffi::c_void,
        );
        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let hicon = CreateIconIndirect(&info);
        DeleteObject(color as _);
        DeleteObject(mask as _);
        if hicon.is_null() {
            return Err(err!("CreateIconIndirect falhou"));
        }
        Ok(hicon)
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_APP_TRAY {
            let mouse_msg = (lparam & 0xFFFF) as u32;
            if mouse_msg == WM_LBUTTONUP || mouse_msg == WM_RBUTTONUP {
                show_menu(hwnd);
            }
            return 0;
        }
        if msg == WM_APP_BALLOON {
            flush_balloons(hwnd);
            return 0;
        }
        if msg == WM_APP_SETCHECK {
            let hmenu = HMENU_SHELL.load(Ordering::SeqCst) as _;
            let flags =
                MF_BYCOMMAND | if lparam != 0 { MF_CHECKED } else { MF_UNCHECKED };
            CheckMenuItem(hmenu, wparam as u32, flags);
            return 0;
        }
        if msg == WM_HOTKEY {
            if let Some(handler) = HANDLER.get() {
                handler(ShellEvent::Hotkey(wparam as i32));
            }
            return 0;
        }
        if msg == WM_DESTROY {
            remove_icon();
            return 0;
        }
        if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::SeqCst) {
            // Explorer reiniciou: o ícone precisa ser re-adicionado.
            let hicon = HICON_SHELL.load(Ordering::SeqCst) as HICON;
            let _ = add_tray_icon(hwnd, hicon);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe fn show_menu(hwnd: HWND) {
        let hmenu = HMENU_SHELL.load(Ordering::SeqCst) as _;
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        // Sem SetForegroundWindow o menu não fecha ao clicar fora (bug
        // clássico de menus de bandeja documentado pela Microsoft).
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenuEx(
            hmenu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            point.x,
            point.y,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, 0 /* WM_NULL */, 0, 0);
        if cmd > 0 {
            if let Some(handler) = HANDLER.get() {
                handler(ShellEvent::Menu(cmd as u16));
            }
        }
    }

    unsafe fn flush_balloons(hwnd: HWND) {
        loop {
            let next = BALLOONS.lock().unwrap().pop_front();
            let Some((title, text)) = next else { break };
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd;
            data.uID = TRAY_ICON_ID;
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
            data.hBalloonIcon = HICON_SHELL.load(Ordering::SeqCst) as HICON;
            fill_wide(&mut data.szInfoTitle, &title);
            fill_wide(&mut data.szInfo, &text);
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    // ------------------------------------------------------------------
    // API pública (as thread-safe usam PostMessage para a thread da janela)
    // ------------------------------------------------------------------

    pub fn set_menu_checked(id: u16, checked: bool) {
        let hwnd = HWND_SHELL.load(Ordering::SeqCst);
        if hwnd != 0 {
            // SAFETY: PostMessage é thread-safe; o WndProc aplica o check.
            unsafe {
                PostMessageW(hwnd as HWND, WM_APP_SETCHECK, id as WPARAM, checked as LPARAM);
            }
        }
    }

    /// Enfileira um balão de notificação (chamável de qualquer thread).
    /// Retorna `false` quando a bandeja não existe (chamador loga).
    pub fn show_balloon(title: &str, text: &str) -> bool {
        let hwnd = HWND_SHELL.load(Ordering::SeqCst);
        if hwnd == 0 {
            return false;
        }
        BALLOONS
            .lock()
            .unwrap()
            .push_back((title.to_string(), text.to_string()));
        // SAFETY: PostMessage é thread-safe.
        unsafe {
            PostMessageW(hwnd as HWND, WM_APP_BALLOON, 0, 0);
        }
        true
    }

    /// Registra um atalho global (thread do event loop; RF-01…RF-03).
    pub fn register_hotkey(id: i32, modifiers: u32, vk: u32) -> Result<()> {
        const MOD_NOREPEAT: u32 = 0x4000;
        let hwnd = HWND_SHELL.load(Ordering::SeqCst);
        if hwnd == 0 {
            return Err(err!("shell não inicializado"));
        }
        // SAFETY: hwnd válido; RegisterHotKey reporta conflito via retorno.
        let ok = unsafe { RegisterHotKey(hwnd as HWND, id, modifiers | MOD_NOREPEAT, vk) };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            return Err(if code == 1409 {
                err!("combinação já registrada por outro aplicativo")
            } else {
                err!("RegisterHotKey falhou (código {code})")
            });
        }
        Ok(())
    }

    pub fn unregister_hotkey(id: i32) {
        let hwnd = HWND_SHELL.load(Ordering::SeqCst);
        if hwnd != 0 {
            // SAFETY: par do RegisterHotKey acima.
            unsafe {
                UnregisterHotKey(hwnd as HWND, id);
            }
        }
    }

    /// Remove o ícone da bandeja (saída limpa do app).
    pub fn remove_icon() {
        let hwnd = HWND_SHELL.load(Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        // SAFETY: NIM_DELETE com o mesmo (hWnd, uID) do NIM_ADD.
        unsafe {
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd as HWND;
            data.uID = TRAY_ICON_ID;
            Shell_NotifyIconW(NIM_DELETE, &data);
        }
    }
}

#[cfg(windows)]
pub use imp::{
    init, register_hotkey, remove_icon, set_menu_checked, show_balloon, unregister_hotkey,
};

// ---------------------------------------------------------------------------
// Stubs para hosts não-Windows (apenas testes de lógica)
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub fn init(
    _tooltip: &str,
    _icon_rgba: (&[u8], u32, u32),
    _menu: &[MenuEntry],
    _handler: EventHandler,
) -> Result<()> {
    Err(crate::error::err!("bandeja disponível apenas no Windows"))
}

#[cfg(not(windows))]
pub fn set_menu_checked(_id: u16, _checked: bool) {}

#[cfg(not(windows))]
pub fn show_balloon(_title: &str, _text: &str) -> bool {
    false
}

#[cfg(not(windows))]
pub fn register_hotkey(_id: i32, _modifiers: u32, _vk: u32) -> Result<()> {
    Err(crate::error::err!("atalhos globais disponíveis apenas no Windows"))
}

#[cfg(not(windows))]
pub fn unregister_hotkey(_id: i32) {}

#[cfg(not(windows))]
pub fn remove_icon() {}
