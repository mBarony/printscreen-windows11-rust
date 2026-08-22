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

#[derive(Debug, Clone)]
pub enum ShellEvent {
    /// Item de menu clicado (id definido em `MenuEntry`).
    Menu(u16),
    /// Atalho global disparado (id passado em `register_hotkey`).
    Hotkey(i32),
    /// Mensagem de um processo de GUI (`WM_COPYDATA`): `kind` é um dos
    /// `IPC_*` e `payload` o texto UTF-8 que o acompanha.
    Ipc { kind: u32, payload: String },
}

/// Pedido de balão de notificação: `payload` é `título\ntexto`.
pub const IPC_BALLOON: u32 = 1;
/// O `config.json` foi regravado pela janela de configurações; o residente
/// precisa recarregá-lo e re-registrar os atalhos.
pub const IPC_CONFIG_CHANGED: u32 = 2;
/// O processo de GUI saiu da seleção e abriu o editor. A partir daí ele tem
/// trabalho do usuário dentro, e o atalho não pode mais encerrá-lo.
pub const IPC_EDITOR_OPEN: u32 = 3;

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
        TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_COPYDATA, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP,
        WM_RBUTTONUP, WNDCLASSW,
    };
    // COPYDATASTRUCT vive em DataExchange (a feature já estava habilitada para
    // a área de transferência), não em WindowsAndMessaging.
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;

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
        if msg == WM_COPYDATA {
            // O ponteiro só é válido durante esta chamada (o Windows faz o
            // marshalling entre processos), então a cópia é obrigatória.
            let data = lparam as *const COPYDATASTRUCT;
            if !data.is_null() {
                let cds = &*data;
                let bytes = if cds.cbData == 0 || cds.lpData.is_null() {
                    &[][..]
                } else {
                    std::slice::from_raw_parts(cds.lpData.cast::<u8>(), cds.cbData as usize)
                };
                let payload = String::from_utf8_lossy(bytes).into_owned();
                if let Some(handler) = HANDLER.get() {
                    handler(ShellEvent::Ipc { kind: cds.dwData as u32, payload });
                }
            }
            return 1;
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

    /// Traz para o primeiro plano a janela de titulo `title`, **com foco de
    /// teclado**. Retorna `false` se a janela nao existe. A busca e' por titulo
    /// no desktop, entao serve tanto para uma janela do proprio processo (o
    /// editor) quanto para a de um processo de GUI filho (as configuracoes, que
    /// o residente traz de volta em vez de abrir uma segunda).
    ///
    /// Um `SetForegroundWindow` simples e' recusado pelo Windows quando o
    /// processo nao detem o primeiro plano (foreground lock) — exatamente o
    /// caso do editor, que nasce logo depois de o overlay fechar e devolver
    /// o primeiro plano para o app que estava atras. A saida documentada e'
    /// anexar a fila de entrada da thread que detem o primeiro plano, o que
    /// faz o Windows tratar a chamada como vinda dela.
    pub fn focus_window(title: &str) -> bool {
        use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            BringWindowToTop, FindWindowW, GetForegroundWindow, GetWindowThreadProcessId,
            IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };

        let wide_title = wide(title);
        // SAFETY: janelas do proprio processo; os handles sao verificados e o
        // AttachThreadInput e' desfeito no mesmo escopo.
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), wide_title.as_ptr());
            if hwnd.is_null() {
                return false;
            }
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }

            let foreground = GetForegroundWindow();
            let fg_thread = if foreground.is_null() {
                0
            } else {
                GetWindowThreadProcessId(foreground, std::ptr::null_mut())
            };
            let this_thread = GetCurrentThreadId();

            let attached = fg_thread != 0
                && fg_thread != this_thread
                && AttachThreadInput(this_thread, fg_thread, 1) != 0;

            SetForegroundWindow(hwnd);
            BringWindowToTop(hwnd);
            SetFocus(hwnd);

            if attached {
                AttachThreadInput(this_thread, fg_thread, 0);
            }
            true
        }
    }

    /// `HWND` da janela de shell como inteiro, para passar ao processo de GUI
    /// na linha de comando. `0` antes do `init`.
    pub fn hwnd_value() -> isize {
        HWND_SHELL.load(Ordering::SeqCst)
    }

    /// Bombeia mensagens até `PostQuitMessage` (o "Sair" do menu). É o corpo do
    /// processo residente — sem event loop de GUI, ele é quem mantém a bandeja
    /// e os atalhos vivos.
    ///
    /// `after_dispatch` roda depois de cada mensagem, **fora** do `WndProc`: é
    /// onde o residente trata os eventos que o handler enfileirou, sem risco de
    /// reentrar em um menu modal.
    pub fn run_message_loop(mut after_dispatch: impl FnMut()) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, MSG,
        };

        // SAFETY: laço clássico de mensagens da própria thread; `msg` é
        // preenchido pelo GetMessageW antes de qualquer leitura.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
                after_dispatch();
            }
        }
    }

    /// Liga/desliga o despertador de 1 s que permite ao residente notar que um
    /// processo de GUI encerrou (e então liberar o bloco compartilhado). Fica
    /// desligado enquanto não há filho: idle continua sendo event-driven.
    pub fn set_poll_timer(on: bool) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

        const POLL_TIMER_ID: usize = 1;
        let hwnd = HWND_SHELL.load(Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        // SAFETY: hwnd da própria janela; Set/KillTimer com o mesmo id.
        unsafe {
            if on {
                SetTimer(hwnd as HWND, POLL_TIMER_ID, 1000, None);
            } else {
                KillTimer(hwnd as HWND, POLL_TIMER_ID);
            }
        }
    }

    /// Encerra o `run_message_loop` (menu "Sair").
    pub fn post_quit() {
        use windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage;

        // SAFETY: só marca WM_QUIT na fila da thread chamadora.
        unsafe { PostQuitMessage(0) }
    }

    /// Envia `payload` ao residente (chamado pelo processo de GUI). O timeout
    /// evita que um residente ocupado — ou já encerrado — trave o filho.
    pub fn send_to_resident(hwnd_value: isize, kind: u32, payload: &str) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_ABORTIFHUNG};

        if hwnd_value == 0 {
            return false;
        }
        let bytes = payload.as_bytes();
        let cds = COPYDATASTRUCT {
            dwData: kind as usize,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut core::ffi::c_void,
        };
        // SAFETY: `bytes` e `cds` vivem por toda a chamada síncrona; o Windows
        // copia o buffer para o processo destino. Um HWND inválido apenas faz a
        // chamada falhar.
        let sent = unsafe {
            SendMessageTimeoutW(
                hwnd_value as HWND,
                WM_COPYDATA,
                0,
                &cds as *const COPYDATASTRUCT as LPARAM,
                SMTO_ABORTIFHUNG,
                // O residente só enfileira a mensagem e retorna; meio segundo é
                // folga suficiente. Um valor alto congelaria a UI do filho
                // quando o residente já tivesse encerrado.
                500,
                std::ptr::null_mut(),
            )
        };
        sent != 0
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
    focus_window, hwnd_value, init, post_quit, register_hotkey, remove_icon, run_message_loop,
    send_to_resident, set_menu_checked, set_poll_timer, show_balloon, unregister_hotkey,
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

#[cfg(not(windows))]
pub fn hwnd_value() -> isize {
    0
}

#[cfg(not(windows))]
pub fn run_message_loop(_after_dispatch: impl FnMut()) {}

#[cfg(not(windows))]
pub fn set_poll_timer(_on: bool) {}

#[cfg(not(windows))]
pub fn post_quit() {}

#[cfg(not(windows))]
pub fn send_to_resident(_hwnd_value: isize, _kind: u32, _payload: &str) -> bool {
    false
}

#[cfg(not(windows))]
pub fn focus_window(_title: &str) -> bool {
    false
}
