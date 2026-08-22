//! Processo de GUI: executa **uma** tarefa recebida na linha de comando e
//! encerra (§5, `Selecting → Editing → fim`).
//!
//! Aqui vivem o eframe, o wgpu e o device D3D12 — tudo o que o processo
//! residente deixou de carregar. O preço é pago só enquanto há janela na tela:
//! quando o último viewport fecha, o processo morre e devolve a memória ao SO.
//!
//! Sem bandeja, sem atalhos globais e sem fila de eventos externos: a única
//! conversa com o residente é de saída — balões de notificação e o aviso de que
//! o `config.json` foi regravado (`platform::shell::IPC_*`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::capture::MonitorShot;
use crate::config::{self, Config};
use crate::editor::{self, EditorSession};
use crate::overlay::{self, Outcome, Purpose, SelectSession, SelectedAction};
use crate::settings::SettingsState;
use crate::storage::{self, SaveTarget};
use crate::{capture, jobs, notify, tray};

/// O que este processo foi lançado para fazer.
pub enum Task {
    /// Overlay de seleção sobre as capturas recebidas do residente.
    Select { shots: Vec<MonitorShot>, purpose: Purpose },
    /// Janela de configurações.
    Settings,
}

fn next_serial() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Máquina de estados do fluxo de captura (§5).
pub enum Flow {
    Idle,
    Selecting(SelectSession),
    /// Em caixa: a sessão de edição é de longe a maior variante (o documento
    /// carrega imagem base e log de operações), e sem a indireção todo `Flow`
    /// — inclusive o `Idle` — pagaria esse tamanho.
    Editing(Box<EditorSession>),
}

pub struct AppShared {
    pub config: Config,
    pub flow: Flow,
    pub settings: Option<SettingsState>,
    pub quit: bool,
}

pub struct GuiApp {
    shared: Arc<Mutex<AppShared>>,
    window_icon: Arc<egui::IconData>,
}

impl GuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config, task: Task) -> Self {
        crate::theme::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx);

        // A janela-raiz é visível para o SO (ver main.rs), mas não deve
        // aparecer no Alt-Tab/Win-Tab: troca WS_EX_APPWINDOW por
        // WS_EX_TOOLWINDOW — o `with_taskbar(false)` do winit remove apenas o
        // botão da barra de tarefas (ITaskbarList::DeleteTab), não o switcher.
        #[cfg(windows)]
        remove_root_from_alt_tab();

        let (flow, settings) = match task {
            Task::Select { shots, purpose } => {
                let session =
                    SelectSession::new(&cc.egui_ctx, next_serial(), shots, purpose);
                (Flow::Selecting(session), None)
            }
            Task::Settings => (Flow::Idle, Some(SettingsState::new(config.clone()))),
        };

        Self {
            shared: Arc::new(Mutex::new(AppShared { config, flow, settings, quit: false })),
            window_icon: Arc::new(app_icon_data()),
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
                            jobs::spawn(move || match crate::clipboard::copy_image(&cropped) {
                                Ok(()) => notify::toast(
                                    "Copiado para a área de transferência",
                                    "A região selecionada está pronta para colar.",
                                ),
                                Err(err) => {
                                    notify::toast_error("Falha ao copiar", &format!("{err:#}"))
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
                            shared.flow = Flow::Editing(Box::new(EditorSession::new(
                                next_serial(),
                                cropped,
                                &defaults,
                            )));
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

        // Janela de configurações: publicar o rascunho e/ou fechar.
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
            self.publish_config(new_config);
        }

        // Tarefa cumprida: o processo de GUI existe só enquanto há janela.
        let mut shared = self.shared.lock().unwrap();
        if matches!(shared.flow, Flow::Idle) && shared.settings.is_none() {
            shared.quit = true;
        }
    }

    /// Grava o `config.json` e avisa o residente, que é o dono dos atalhos e do
    /// registro do Windows (RF-05: efeito imediato, sem reiniciar).
    fn publish_config(&mut self, new_config: Config) {
        if let Err(err) = config::save(&new_config) {
            notify::toast_error("Falha ao gravar config.json", &format!("{err:#}"));
            return;
        }
        crate::resident_link::config_changed();

        let mut shared = self.shared.lock().unwrap();
        shared.config = new_config.clone();
        // O config novo vale também para a sessão de edição já aberta
        // (issue #4) — teclas e papel da roda são snapshots simples.
        if let Flow::Editing(session) = &mut shared.flow {
            session.tool_keys = editor::resolve_tool_keys(&new_config.editor.tool_keys);
            session.ctrl_wheel_zoom = new_config.editor.ctrl_wheel == config::CtrlWheel::Zoom;
        }
        if let Some(settings) = &mut shared.settings {
            settings.draft = new_config;
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
                let img_w = session.doc.image().width() as f32;
                let img_h = session.doc.image().height() as f32;
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
                .with_title(crate::settings::WINDOW_TITLE)
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

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 0a. Mantém a janela-raiz fora do Alt-Tab (o winit pode reescrever o
        // estilo; a checagem é barata e só escreve quando necessário).
        #[cfg(windows)]
        remove_root_from_alt_tab();

        // 0b. Overlays de seleção como janelas em camadas (alfa 255): uma
        // janela topo-de-tudo que cobre exatamente o monitor é promovida pelo
        // DWM/driver a direct/independent flip, e a troca de modo de scanout
        // apaga o monitor por ~1 s em algumas GPUs (visível a olho, invisível
        // em gravações). Janela layered é sempre composta — nunca promovida.
        #[cfg(windows)]
        if matches!(self.shared.lock().unwrap().flow, Flow::Selecting(_)) {
            make_overlays_layered();
        }

        // 1. Transições pedidas pelos viewports.
        self.process_shared();

        // 2. Viewports dos fluxos ativos.
        self.declare_viewports(ctx);

        // 3. Tarefa concluída: encerra o processo.
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

    let title: Vec<u16> = ROOT_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
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

/// Título do viewport-raiz deste processo. Distinto do nome do app: o residente
/// procura janelas por título e não deve encontrar a raiz do filho.
pub const ROOT_TITLE: &str = "RustShot GUI";
