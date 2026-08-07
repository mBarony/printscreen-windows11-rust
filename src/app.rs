//! Estado global e máquina de estados (§5): `Idle → Selecting → Editing →
//! Idle`. Enquanto um fluxo está ativo, novos atalhos de captura são
//! ignorados com um toast informativo.
//!
//! Eventos externos (atalhos globais, cliques no menu da bandeja) são
//! empurrados para uma fila estática pelos handlers (que rodam no pump de
//! mensagens Win32 da própria thread do event loop) e drenados a cada
//! `update`; `ctx.request_repaint()` garante que o `update` aconteça logo.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::{self, Config, APP_NAME};
use crate::editor::{self, EditorSession};
use crate::hotkeys::{HotkeyAction, Hotkeys};
use crate::notify;
use crate::overlay::{self, Outcome, Purpose, SelectSession, SelectedAction};
use crate::platform::shell::ShellEvent;
use crate::settings::SettingsState;
use crate::storage::{self, SaveTarget};
use crate::tray::{self, Tray};
use crate::{capture, hotkeys, platform};

// ---------------------------------------------------------------------------
// Fila de eventos externos
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum AppEvent {
    /// Id do `WM_HOTKEY` de um atalho global pressionado.
    Hotkey(i32),
    /// Id do item de menu da bandeja clicado (constantes em `tray`).
    Menu(u16),
}

pub mod events {
    use super::AppEvent;
    use std::sync::Mutex;

    static QUEUE: Mutex<Vec<AppEvent>> = Mutex::new(Vec::new());

    pub fn push(event: AppEvent) {
        if let Ok(mut queue) = QUEUE.lock() {
            queue.push(event);
        }
    }

    pub fn drain() -> Vec<AppEvent> {
        QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }
}

fn next_serial() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Estado compartilhado com os viewports
// ---------------------------------------------------------------------------

/// Máquina de estados global (§5).
pub enum Flow {
    Idle,
    Selecting(SelectSession),
    Editing(EditorSession),
}

pub struct AppShared {
    pub config: Config,
    pub flow: Flow,
    pub settings: Option<SettingsState>,
    pub quit: bool,
}

pub struct RustShotApp {
    shared: Arc<Mutex<AppShared>>,
    hotkeys: Hotkeys,
    /// Mantém o ícone/menu da bandeja vivos (RF-06).
    tray: Option<Tray>,
    window_icon: Arc<egui::IconData>,
    last_busy_toast: Option<Instant>,
}

impl RustShotApp {
    pub fn new(cc: &eframe::CreationContext<'_>, loaded: config::LoadedConfig) -> Self {
        crate::theme::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx);

        let config = loaded.config;

        // Honra "Iniciar com o Windows" persistido (CA-07) — também corrige o
        // caminho no registro caso o exe tenha sido movido. Só sincroniza
        // quando o config veio de disco: se o arquivo não pôde ser lido ou
        // gravado (pasta somente-leitura), o padrão `false` removeria do
        // registro um autostart que o usuário ativou em sessão anterior.
        if !loaded.created {
            if let Err(err) = apply_autostart(config.start_with_windows) {
                log::warn!("não foi possível sincronizar autostart: {err:#}");
            }
        }

        // A janela-raiz é visível para o SO (ver main.rs), mas não deve
        // aparecer no Alt-Tab/Win-Tab: troca WS_EX_APPWINDOW por
        // WS_EX_TOOLWINDOW — o `with_taskbar(false)` do winit remove apenas o
        // botão da barra de tarefas (ITaskbarList::DeleteTab), não o switcher.
        #[cfg(windows)]
        remove_root_from_alt_tab();

        // Bandeja + janela de shell — a mesma janela recebe cliques do menu
        // e os WM_HOTKEY. O handler roda no pump Win32 da própria thread do
        // event loop: só enfileira o evento e acorda a UI.
        let ctx = cc.egui_ctx.clone();
        let tray = match Tray::new(config.start_with_windows, move |event| {
            match event {
                ShellEvent::Menu(id) => events::push(AppEvent::Menu(id)),
                ShellEvent::Hotkey(id) => events::push(AppEvent::Hotkey(id)),
            }
            ctx.request_repaint();
        }) {
            Ok(tray) => Some(tray),
            Err(err) => {
                notify::toast_error(
                    "Bandeja indisponível",
                    &format!("O menu da bandeja não pôde ser criado: {err}"),
                );
                None
            }
        };

        // Atalhos globais: registrados na janela de shell criada acima.
        let mut hotkeys = Hotkeys::new();
        toast_hotkey_failures(hotkeys.apply(&config.hotkeys));

        // Hook de desenvolvimento (builds debug): abre a janela de
        // configurações direto, para inspeção visual sem passar pela bandeja.
        #[cfg(debug_assertions)]
        let settings = std::env::var_os("RUSTSHOT_OPEN")
            .is_some_and(|v| v == "settings")
            .then(|| SettingsState::new(config.clone()));
        #[cfg(not(debug_assertions))]
        let settings = None;

        if loaded.created {
            notify::toast(
                "RustShot em execução",
                "Ícone disponível na bandeja do sistema. Ctrl+PrtScr captura a tela.",
            );
        }
        if loaded.recovered {
            notify::toast(
                "Configuração recriada",
                "O config.json estava inválido; backup salvo como config.json.bak.",
            );
        }

        let window_icon = Arc::new(app_icon_data());

        Self {
            shared: Arc::new(Mutex::new(AppShared {
                config,
                flow: Flow::Idle,
                settings,
                quit: false,
            })),
            hotkeys,
            tray,
            window_icon,
            last_busy_toast: None,
        }
    }

    // -----------------------------------------------------------------------
    // Disparo dos fluxos de captura
    // -----------------------------------------------------------------------

    fn trigger(&mut self, ctx: &egui::Context, action: HotkeyAction) {
        // O fluxo só inicia a partir de Idle (§5).
        {
            let shared = self.shared.lock().unwrap();
            if !matches!(shared.flow, Flow::Idle) {
                drop(shared);
                self.busy_toast();
                return;
            }
        }

        match action {
            HotkeyAction::Fullscreen => {
                let (scope, target) = {
                    let shared = self.shared.lock().unwrap();
                    (
                        shared.config.fullscreen_scope,
                        SaveTarget::from_config(&shared.config),
                    )
                };
                // A captura em si é rápida (BitBlt); codificação e escrita
                // acontecem em thread de trabalho (RNF-03).
                match capture::capture_fullscreen(scope) {
                    Ok(image) => storage::save_in_background(target, image),
                    Err(err) => {
                        notify::toast_error("Falha na captura", &format!("{err:#}"));
                    }
                }
            }
            HotkeyAction::Region | HotkeyAction::Edit => {
                let purpose = if action == HotkeyAction::Region {
                    Purpose::SaveDirect
                } else {
                    Purpose::Edit
                };
                // Captura imediatamente todos os monitores: congela o
                // conteúdo antes do overlay aparecer (§7, Fluxo B).
                match capture::capture_all_monitors() {
                    Ok(shots) => {
                        let session =
                            SelectSession::new(ctx, next_serial(), shots, purpose);
                        let mut shared = self.shared.lock().unwrap();
                        shared.flow = Flow::Selecting(session);
                    }
                    Err(err) => {
                        notify::toast_error("Falha na captura", &format!("{err:#}"));
                    }
                }
            }
        }
    }

    fn busy_toast(&mut self) {
        let now = Instant::now();
        let recent = self
            .last_busy_toast
            .is_some_and(|t| now.duration_since(t) < Duration::from_millis(1500));
        if !recent {
            self.last_busy_toast = Some(now);
            notify::toast(
                "Captura em andamento",
                "Conclua ou cancele a seleção/edição atual antes de iniciar outra.",
            );
        }
    }

    fn handle_menu(&mut self, ctx: &egui::Context, id: u16) {
        match id {
            tray::MENU_CAPTURE_FULLSCREEN => self.trigger(ctx, HotkeyAction::Fullscreen),
            tray::MENU_CAPTURE_REGION => self.trigger(ctx, HotkeyAction::Region),
            tray::MENU_CAPTURE_EDIT => self.trigger(ctx, HotkeyAction::Edit),
            tray::MENU_OPEN_FOLDER => {
                let dir = self.shared.lock().unwrap().config.effective_output_dir();
                open_folder(&dir);
            }
            tray::MENU_SETTINGS => {
                let mut shared = self.shared.lock().unwrap();
                if shared.settings.is_none() {
                    let config = shared.config.clone();
                    shared.settings = Some(SettingsState::new(config));
                }
            }
            tray::MENU_AUTOSTART => {
                let wanted = {
                    let shared = self.shared.lock().unwrap();
                    !shared.config.start_with_windows
                };
                match apply_autostart(wanted) {
                    Ok(()) => {
                        let mut shared = self.shared.lock().unwrap();
                        shared.config.start_with_windows = wanted;
                        if let Err(err) = config::save(&shared.config) {
                            log::warn!("falha ao salvar config: {err:#}");
                        }
                        drop(shared);
                        notify::toast(
                            "Iniciar com o Windows",
                            if wanted { "Ativado." } else { "Desativado." },
                        );
                    }
                    Err(err) => {
                        notify::toast_error(
                            "Falha ao alterar inicialização automática",
                            &format!("{err:#}"),
                        );
                    }
                }
                let checked = self.shared.lock().unwrap().config.start_with_windows;
                if let Some(tray) = &self.tray {
                    tray.set_autostart_checked(checked);
                }
            }
            tray::MENU_QUIT => {
                self.shared.lock().unwrap().quit = true;
            }
            other => log::debug!("item de menu desconhecido: {other:#06x}"),
        }
    }

    // -----------------------------------------------------------------------
    // Transições pendentes vindas dos viewports
    // -----------------------------------------------------------------------

    fn process_shared(&mut self) {
        // Resultado do overlay de seleção.
        let outcome_step = {
            let mut shared = self.shared.lock().unwrap();
            match &mut shared.flow {
                Flow::Selecting(sel) if sel.outcome.is_some() => {
                    let outcome = sel.outcome.take().expect("checado acima");
                    let old = std::mem::replace(&mut shared.flow, Flow::Idle);
                    let Flow::Selecting(sel) = old else { unreachable!() };
                    Some((sel, outcome, SaveTarget::from_config(&shared.config)))
                }
                _ => None,
            }
        };
        if let Some((session, outcome, target)) = outcome_step {
            match outcome {
                Outcome::Cancelled => {
                    log::info!("seleção de região cancelada");
                }
                Outcome::Selected { monitor, rect: (x, y, w, h), action } => {
                    let shot = &session.monitors[monitor].shot;
                    let cropped = capture::crop(&shot.image, x, y, w, h);
                    match action {
                        // Ctrl+C na seleção pendente: só copia (v1.2).
                        SelectedAction::CopyToClipboard => {
                            std::thread::spawn(move || {
                                match crate::clipboard::copy_image(&cropped) {
                                    Ok(()) => notify::toast(
                                        "Copiado para a área de transferência",
                                        "A região selecionada está pronta para colar.",
                                    ),
                                    Err(err) => notify::toast_error(
                                        "Falha ao copiar",
                                        &format!("{err:#}"),
                                    ),
                                }
                            });
                        }
                        // Ctrl+S na seleção pendente: só salva (v1.2).
                        SelectedAction::SaveToFile => {
                            storage::save_in_background(target, cropped);
                        }
                        SelectedAction::OpenEditor => {
                            let mut shared = self.shared.lock().unwrap();
                            let defaults = shared.config.editor.clone();
                            shared.flow = Flow::Editing(EditorSession::new(
                                next_serial(),
                                cropped,
                                &defaults,
                            ));
                        }
                    }
                }
            }
        }

        // Editor concluído (salvou ou descartou).
        {
            let mut shared = self.shared.lock().unwrap();
            if let Flow::Editing(session) = &shared.flow {
                if session.finished {
                    shared.flow = Flow::Idle;
                }
            }
        }

        // Janela de configurações: aplicar e/ou fechar.
        let pending = {
            let mut shared = self.shared.lock().unwrap();
            let pending = shared
                .settings
                .as_mut()
                .and_then(|s| s.pending_apply.take());
            if shared.settings.as_ref().is_some_and(|s| s.close_requested) {
                shared.settings = None;
            }
            pending
        };
        if let Some(new_config) = pending {
            self.apply_config(new_config);
        }
    }

    fn apply_config(&mut self, new_config: Config) {
        // Atalhos: re-registro imediato, sem reiniciar (RF-05).
        let failures = self.hotkeys.apply(&new_config.hotkeys);

        if let Err(err) = apply_autostart(new_config.start_with_windows) {
            notify::toast_error(
                "Falha ao alterar inicialização automática",
                &format!("{err:#}"),
            );
        }
        if let Some(tray) = &self.tray {
            tray.set_autostart_checked(new_config.start_with_windows);
        }

        if let Err(err) = config::save(&new_config) {
            notify::toast_error("Falha ao gravar config.json", &format!("{err:#}"));
        }

        let failure_lines: Vec<String> = failures
            .iter()
            .map(|f| {
                format!(
                    "⚠ {} — o atalho {} não pôde ser registrado: {}",
                    f.action.label(),
                    f.pretty,
                    f.reason
                )
            })
            .collect();

        {
            let mut shared = self.shared.lock().unwrap();
            shared.config = new_config.clone();
            // O config novo vale também para a sessão de edição já aberta
            // (issue #4) — teclas e papel da roda são snapshots simples.
            if let Flow::Editing(session) = &mut shared.flow {
                session.tool_keys = editor::resolve_tool_keys(&new_config.editor.tool_keys);
                session.ctrl_wheel_zoom =
                    new_config.editor.ctrl_wheel == config::CtrlWheel::Zoom;
            }
            if let Some(settings) = &mut shared.settings {
                settings.last_failures = failure_lines.clone();
                settings.draft = new_config;
            }
        }

        if failures.is_empty() {
            notify::toast("Configurações aplicadas", "Alterações em vigor.");
        } else {
            toast_hotkey_failures(failures);
        }
    }

    // -----------------------------------------------------------------------
    // Declaração dos viewports ativos
    // -----------------------------------------------------------------------

    fn declare_viewports(&self, ctx: &egui::Context) {
        let shared = self.shared.lock().unwrap();

        match &shared.flow {
            Flow::Selecting(session) => {
                for idx in 0..session.monitors.len() {
                    let id = egui::ViewportId::from_hash_of(("overlay", session.serial, idx));
                    let builder = session.viewport_builder(idx);
                    let state = self.shared.clone();
                    ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                        let mut shared = state.lock().unwrap();
                        if let Flow::Selecting(session) = &mut shared.flow {
                            if session.outcome.is_none() && idx < session.monitors.len() {
                                overlay::overlay_ui(ctx, session, idx);
                            }
                        }
                    });
                }
            }
            Flow::Editing(session) => {
                let id = egui::ViewportId::from_hash_of(("editor", session.serial));
                let img_w = session.image.width() as f32;
                let img_h = session.image.height() as f32;
                let size = egui::Vec2::new(
                    (img_w + 24.0).clamp(660.0, 1280.0),
                    (img_h + 110.0).clamp(480.0, 860.0),
                );
                let builder = egui::ViewportBuilder::default()
                    .with_title(editor::WINDOW_TITLE)
                    .with_inner_size(size)
                    .with_min_inner_size(egui::Vec2::new(560.0, 400.0))
                    // Nasce ativa: o usuário deve poder usar Ctrl+C/Ctrl+S
                    // logo após a seleção, sem clicar na janela antes.
                    .with_active(true)
                    .with_icon(self.window_icon.clone());
                let state = self.shared.clone();
                ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                    let mut shared = state.lock().unwrap();
                    let target = SaveTarget::from_config(&shared.config);
                    if let Flow::Editing(session) = &mut shared.flow {
                        editor::ui::show(ctx, session, &target);
                        if session.finished {
                            ctx.request_repaint_of(egui::ViewportId::ROOT);
                        }
                    }
                });
            }
            Flow::Idle => {}
        }

        if shared.settings.is_some() {
            let id = egui::ViewportId::from_hash_of("rustshot_settings");
            let builder = egui::ViewportBuilder::default()
                .with_title("RustShot — Configurações")
                .with_inner_size(egui::Vec2::new(680.0, 640.0))
                .with_min_inner_size(egui::Vec2::new(600.0, 460.0))
                .with_icon(self.window_icon.clone());
            let state = self.shared.clone();
            ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                let mut shared = state.lock().unwrap();
                if let Some(settings) = &mut shared.settings {
                    crate::settings::show(ctx, settings);
                    if settings.close_requested || settings.pending_apply.is_some() {
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
            });
        }
    }
}

impl eframe::App for RustShotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 0a. Mantém a janela-raiz fora do Alt-Tab (o winit pode reescrever o
        // estilo; a checagem é barata e só escreve quando necessário).
        #[cfg(windows)]
        remove_root_from_alt_tab();

        // 0a'. Overlays de seleção como janelas em camadas (alfa 255): uma
        // janela topo-de-tudo que cobre exatamente o monitor é promovida pelo
        // DWM/driver a direct/independent flip, e a troca de modo de scanout
        // apaga o monitor por ~1 s em algumas GPUs (visível a olho, invisível
        // em gravações). Janela layered é sempre composta — nunca promovida.
        #[cfg(windows)]
        if matches!(self.shared.lock().unwrap().flow, Flow::Selecting(_)) {
            make_overlays_layered();
        }

        // 0b. Alt+F4 na janela-raiz (alcançável por engano) encerraria o app
        // inteiro, descartando bandeja e edição em andamento. Só o "Sair" do
        // menu (quit=true) pode fechar o viewport-raiz.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.shared.lock().unwrap().quit
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        // 1. Eventos externos (atalhos e bandeja).
        for event in events::drain() {
            match event {
                AppEvent::Hotkey(id) => {
                    let action = self.hotkeys.action_for(id);
                    if let Some(action) = action {
                        log::info!("atalho global: {action:?}");
                        self.trigger(ctx, action);
                    }
                }
                AppEvent::Menu(id) => self.handle_menu(ctx, id),
            }
        }

        // 2. Transições pedidas pelos viewports.
        self.process_shared();

        // 3. Viewports dos fluxos ativos.
        self.declare_viewports(ctx);

        // 4. Encerramento via menu "Sair" (RF-06/CA-08).
        if self.shared.lock().unwrap().quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

// ---------------------------------------------------------------------------
// Auxiliares
// ---------------------------------------------------------------------------

pub fn app_icon_data() -> egui::IconData {
    let (rgba, width, height) = tray::app_icon_rgba(64);
    egui::IconData { rgba, width, height }
}

fn toast_hotkey_failures(failures: Vec<hotkeys::HotkeyFailure>) {
    for failure in failures {
        notify::toast_error(
            "Atalho não registrado",
            &format!(
                "{} ({}) — possivelmente em uso por outro aplicativo: {}",
                failure.pretty,
                failure.action.label(),
                failure.reason
            ),
        );
    }
}

/// Grava/remove a entrada em `HKCU\...\Run` (§13) via `platform::autostart`.
fn apply_autostart(enabled: bool) -> crate::error::Result<()> {
    let exe = std::env::current_exe()?;
    // Entre aspas: caminho com espaços em entrada não-citada do Run é o
    // clássico unquoted-path.
    let quoted = format!("\"{}\"", exe.display());
    platform::autostart::set(APP_NAME, &quoted, enabled)
}

/// Aplica `WS_EX_LAYERED` (alfa 255, opaco) a toda janela de overlay de
/// seleção. Idempotente: só escreve quando o estilo ainda não está aplicado.
#[cfg(windows)]
fn make_overlays_layered() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW,
        GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED,
    };

    let title: Vec<u16> = "RustShot — seleção de região"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: enumeração de janelas top-level do próprio título; ponteiros
    // válidos pelo escopo de `title`.
    unsafe {
        let mut prev = std::ptr::null_mut();
        loop {
            let hwnd = FindWindowExW(std::ptr::null_mut(), prev, std::ptr::null(), title.as_ptr());
            if hwnd.is_null() {
                break;
            }
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if ex & WS_EX_LAYERED as isize == 0 {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            }
            prev = hwnd;
        }
    }
}

/// Remove a janela-raiz do Alt-Tab: `WS_EX_APPWINDOW` → `WS_EX_TOOLWINDOW`.
/// Idempotente e barata (só escreve quando o estilo está errado): o winit
/// reaplica as flags dele em operações como o `set_visible` pós-1º-frame,
/// então a correção é verificada a cada `update` em vez de uma única vez.
#[cfg(windows)]
fn remove_root_from_alt_tab() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
    };

    let title: Vec<u16> = APP_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: FindWindowW/Get/SetWindowLongPtrW com HWND da janela do próprio
    // processo; ponteiros válidos pelo escopo de `title`.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let wanted = (ex & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize;
            if ex != wanted {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
            }
        }
    }
}

fn open_folder(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(not(windows))]
    let result = std::process::Command::new("xdg-open").arg(dir).spawn();
    if let Err(err) = result {
        notify::toast_error(
            "Não foi possível abrir a pasta",
            &format!("{}: {err}", dir.display()),
        );
    }
}
