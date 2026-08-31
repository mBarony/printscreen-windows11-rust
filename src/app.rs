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
use crate::storage::SaveTarget;
use crate::{capture, jobs, notify, tray};

/// O que este processo foi lançado para fazer.
pub enum Task {
    /// Overlay de seleção sobre as capturas recebidas do residente.
    Select {
        shots: Vec<MonitorShot>,
        /// Janelas visíveis no instante da captura, para o modo janela.
        windows: Vec<crate::platform::window_list::WindowTarget>,
        purpose: Purpose,
    },
    /// Janela de configurações.
    Settings,
    /// Editor retomando uma edição gravada — com o histórico inteiro.
    Recover(Box<crate::editor::document::Document>),
    /// Editor aberto sobre uma imagem que veio do disco.
    ///
    /// Em caixa porque a imagem é de longe o maior campo do enum, e todas as
    /// outras variantes pagariam por ele.
    EditImage(Box<crate::imgbuf::RgbaImage>),
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
    /// Reconhecimento em curso numa thread de trabalho.
    ///
    /// Sem isto o processo encerraria antes de o OCR terminar: quando o
    /// overlay resolve, o fluxo vira `Idle` e nada mais segura a janela — o
    /// texto até chegava à área de transferência, porque a saída espera as
    /// threads, mas o aviso nunca nascia.
    pub ocr_running: bool,
    /// Aviso do reconhecimento de texto, enquanto estiver na tela.
    pub ocr_popup: Option<crate::ocr_popup::OcrPopup>,
    /// Captura fixada na tela, enquanto o usuário não a fechar.
    pub pinned: Option<crate::pinned::PinnedShot>,
    pub quit: bool,
}

pub struct GuiApp {
    shared: Arc<Mutex<AppShared>>,
    /// Cópia do contexto para acordar a janela a partir de thread de
    /// trabalho — é assim que o OCR avisa que terminou.
    ctx: egui::Context,
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
            Task::Select { shots, windows, purpose } => {
                let session =
                    SelectSession::new(&cc.egui_ctx, next_serial(), shots, windows, purpose);
                (Flow::Selecting(session), None)
            }
            Task::Settings => (Flow::Idle, Some(SettingsState::new(config.clone()))),
            Task::Recover(doc) => {
                let session =
                    EditorSession::from_document(next_serial(), *doc, &config.editor);
                (Flow::Editing(Box::new(session)), None)
            }
            Task::EditImage(image) => {
                let session =
                    EditorSession::new(next_serial(), *image, &config.editor);
                (Flow::Editing(Box::new(session)), None)
            }
        };

        Self {
            shared: Arc::new(Mutex::new(AppShared {
                config,
                flow,
                settings,
                ocr_running: false,
                ocr_popup: None,
                pinned: None,
                quit: false,
            })),
            ctx: cc.egui_ctx.clone(),
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
                    Some((sel, outcome))
                }
                _ => None,
            }
        };
        if let Some((session, outcome)) = outcome_step {
            match outcome {
                Outcome::Cancelled => {
                    log::info!("seleção de região cancelada");
                }
                Outcome::Selected { monitor, rect: (x, y, w, h), action } => {
                    let shot = &session.monitors[monitor].shot;
                    let cropped = capture::crop(&shot.image, x, y, w, h);
                    // Em coordenadas do desktop virtual, para o residente
                    // poder repetir a região mesmo com outra lista de
                    // monitores.
                    crate::last_region::save((shot.x + x as i32, shot.y + y as i32, w, h));
                    match action {
                        // Capturar região: salva e/ou copia, conforme as
                        // Configurações, e encerra sem passar pelo editor.
                        SelectedAction::CopyToClipboard => {
                            let (destino, alvo) = {
                                let shared = self.shared.lock().unwrap();
                                (
                                    shared.config.after_region,
                                    crate::storage::SaveTarget::from_config(&shared.config),
                                )
                            };
                            if destino.copies() {
                                let copia = cropped.clone();
                                jobs::spawn(move || match crate::clipboard::copy_image(&copia) {
                                    Ok(()) => notify::toast(
                                        "Copiado para a área de transferência",
                                        "A região selecionada está pronta para colar.",
                                    ),
                                    Err(err) => {
                                        notify::toast_error("Falha ao copiar", &format!("{err:#}"))
                                    }
                                });
                            }
                            if destino.saves() {
                                crate::storage::save_in_background(alvo, cropped);
                            }
                        }
                        SelectedAction::OpenEditor => {
                            // A partir daqui há trabalho do usuário nesta
                            // janela: o residente precisa saber, para o
                            // atalho não a encerrar como faz com o overlay.
                            crate::resident_link::editor_opened();
                            let mut shared = self.shared.lock().unwrap();
                            let defaults = shared.config.editor.clone();
                            shared.flow = Flow::Editing(Box::new(EditorSession::new(
                                next_serial(),
                                cropped,
                                &defaults,
                            )));
                        }
                        // Reconhecer texto: o OCR bloqueia a thread que o
                        // chama (é assim que a API WinRT funciona) e uma tela
                        // cheia leva centenas de milissegundos, então vai
                        // para thread de trabalho. O texto é copiado lá, e o
                        // aviso só aparece depois — a cópia não espera por
                        // janela nenhuma.
                        SelectedAction::RecognizeText => {
                            // Alto e centro do monitor onde a seleção
                            // aconteceu: é a tela para onde o usuário está
                            // olhando, e sai de graça porque já a temos aqui.
                            let anchor = {
                                let scale = shot.scale.max(0.5);
                                (
                                    shot.x as f32 / scale + shot.width as f32 / scale / 2.0
                                        - crate::ocr_popup::SIZE.0 / 2.0,
                                    shot.y as f32 / scale + crate::ocr_popup::TOP_MARGIN,
                                )
                            };
                            let shared = self.shared.clone();
                            let ctx = self.ctx.clone();
                            shared.lock().unwrap().ocr_running = true;
                            jobs::spawn(move || {
                                recognize_and_copy(&cropped, &shared, anchor, &ctx)
                            });
                        }
                    }
                }
            }
        }

        // Pedido de reconhecimento vindo da barra do editor. Termina no mesmo
        // lugar que o do atalho — mesma cópia, mesmo aviso.
        {
            let pedido = {
                let mut shared = self.shared.lock().unwrap();
                match &mut shared.flow {
                    Flow::Editing(session) if session.ocr_requested => {
                        session.ocr_requested = false;
                        log::info!("reconhecendo o texto da imagem do editor");
                        Some(session.doc.visible_image().clone())
                    }
                    _ => None,
                }
            };
            if let Some(image) = pedido {
                // O editor não sabe em que monitor está, e descobrir custaria
                // uma captura. A largura do monitor da raiz basta para o caso
                // comum de uma tela só; com várias, o aviso sai centrado na
                // principal em vez da que tem o editor.
                let anchor = match self.ctx.input(|i| i.viewport().monitor_size) {
                    Some(size) => (
                        size.x / 2.0 - crate::ocr_popup::SIZE.0 / 2.0,
                        crate::ocr_popup::TOP_MARGIN,
                    ),
                    None => (crate::ocr_popup::TOP_MARGIN, crate::ocr_popup::TOP_MARGIN),
                };
                let shared = self.shared.clone();
                let ctx = self.ctx.clone();
                shared.lock().unwrap().ocr_running = true;
                jobs::spawn(move || recognize_and_copy(&image, &shared, anchor, &ctx));
            }
        }

        // Ocultar só as palavras de uma região: o OCR bloqueia, então o
        // retângulo arrastado vira redações depois, de volta nesta thread.
        {
            let pedido = {
                let mut shared = self.shared.lock().unwrap();
                match &mut shared.flow {
                    Flow::Editing(session) => session.redact_text.take().map(|regiao| {
                        (regiao, session.doc.content_image().clone(), session.style())
                    }),
                    _ => None,
                }
            };
            if let Some(((min, max), image, style)) = pedido {
                let shared = self.shared.clone();
                let ctx = self.ctx.clone();
                shared.lock().unwrap().ocr_running = true;
                jobs::spawn(move || redact_words(&image, (min, max), style, &shared, &ctx));
            }
        }

        // Pedido de fixar vindo da barra do editor: a imagem visível vira uma
        // janela sempre no topo, e o editor fecha — como copiar e salvar.
        {
            let mut shared = self.shared.lock().unwrap();
            let pedido = match &mut shared.flow {
                Flow::Editing(session) if session.pin_requested => {
                    session.pin_requested = false;
                    Some(session.doc.visible_image().clone())
                }
                _ => None,
            };
            if let Some(image) = pedido {
                log::info!("fixando a imagem do editor na tela");
                // Um canto qualquer perto do alto: a janela é arrastável, e
                // adivinhar melhor exigiria saber em que monitor o editor
                // está — que é a mesma limitação do aviso do OCR.
                let anchor = (120.0, 120.0);
                shared.pinned = Some(crate::pinned::PinnedShot::new(image, anchor));
                shared.flow = Flow::Idle;
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

        // Aviso do reconhecimento: some quando o tempo acaba ou quando fecham.
        // A captura fixada só sai quando fecham — é o ponto dela.
        {
            let mut shared = self.shared.lock().unwrap();
            if shared.ocr_popup.as_ref().is_some_and(|popup| popup.closed) {
                shared.ocr_popup = None;
            }
            if shared.pinned.as_ref().is_some_and(|pin| pin.closed) {
                shared.pinned = None;
            }
        }
        if let Some(new_config) = pending {
            self.publish_config(new_config);
        }

        // Tarefa cumprida: o processo de GUI existe só enquanto há janela.
        let mut shared = self.shared.lock().unwrap();
        if matches!(shared.flow, Flow::Idle)
            && shared.settings.is_none()
            && !shared.ocr_running
            && shared.ocr_popup.is_none()
            && shared.pinned.is_none()
        {
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
                let img_w = session.doc.visible_image().width() as f32;
                let img_h = session.doc.visible_image().height() as f32;
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

            // A raiz é quem destrói o viewport, e ela só o faz num quadro
            // seu. O `request_repaint_of` acima é o caminho rápido; este é a
            // rede: enquanto houver janela de configurações a raiz não dorme
            // mais que isto, então o processo nunca fica vivo com a janela já
            // escondida. Uma janela de 1×1 a 20 Hz não custa nada, e só
            // enquanto as configurações estão abertas.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        if let Some(popup) = &shared.ocr_popup {
            let id = egui::ViewportId::from_hash_of("rustshot_ocr_popup");
            let (w, h) = crate::ocr_popup::SIZE;
            let builder = egui::ViewportBuilder::default()
                .with_title(crate::ocr_popup::WINDOW_TITLE)
                // Sem moldura, sem barra de tarefas e sem foco: é um aviso,
                // não uma janela de trabalho. Roubar o foco de quem acabou de
                // capturar seria pior que não aparecer.
                .with_decorations(false)
                .with_taskbar(false)
                .with_active(false)
                .with_resizable(false)
                .with_always_on_top()
                .with_position(egui::Pos2::new(popup.anchor.0, popup.anchor.1))
                .with_inner_size(egui::Vec2::new(w, h));
            let state = self.shared.clone();
            ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                let mut shared = state.lock().unwrap();
                if let Some(popup) = &mut shared.ocr_popup {
                    crate::ocr_popup::show(ctx, popup);
                    if popup.closed {
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
            });

            // Mesma rede das configurações: quem destrói o viewport é a raiz,
            // e ela dorme. Sem isto o aviso ficaria na tela depois de o seu
            // tempo acabar, esperando alguém mexer o mouse.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        if let Some(pinned) = &shared.pinned {
            let id = egui::ViewportId::from_hash_of("rustshot_pinned");
            let (w, h) = pinned.size();
            let builder = egui::ViewportBuilder::default()
                .with_title(crate::pinned::WINDOW_TITLE)
                // Sem moldura e sempre no topo, como o aviso do OCR — mas
                // esta recebe foco e aparece na barra de tarefas: é uma
                // janela de trabalho, que fica até o usuário fechá-la, e
                // precisa ser alcançável pelo Alt-Tab.
                .with_decorations(false)
                .with_always_on_top()
                .with_resizable(false)
                .with_position(egui::Pos2::new(pinned.anchor.0, pinned.anchor.1))
                .with_inner_size(egui::Vec2::new(w, h));
            let state = self.shared.clone();
            ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                let mut shared = state.lock().unwrap();
                if let Some(pinned) = &mut shared.pinned {
                    crate::pinned::show(ctx, pinned);
                    if pinned.closed {
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
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
///
/// Chamada de dois lugares, e os dois são necessários: daqui, a cada quadro da
/// raiz, e do próprio `overlay_ui`, porque exibir a janela faz o winit
/// reescrever o `GWL_EXSTYLE` inteiro e a raiz dorme durante a seleção.
#[cfg(windows)]
pub(crate) fn make_overlays_layered() {
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

/// Lê o recorte e põe o resultado na área de transferência.
///
/// **QR primeiro, texto depois.** Quem seleciona um QR quer o endereço que ele
/// carrega, não o OCR dos quadradinhos, e a tentativa é barata: sem os três
/// padrões localizadores ela desiste em milissegundos. O QR não depende da
/// feature `ocr` — é código próprio, sem WinRT —, então este caminho funciona
/// mesmo numa build compilada sem reconhecimento de texto.
///
/// Roda em thread de trabalho por contrato do módulo de OCR: a API do WinRT é
/// assíncrona e `ocr::recognize` espera o resultado, então chamá-la da thread
/// da interface congelaria a janela por centenas de milissegundos.
fn recognize_and_copy(
    image: &crate::imgbuf::RgbaImage,
    shared: &Arc<Mutex<AppShared>>,
    anchor: (f32, f32),
    ctx: &egui::Context,
) {
    if let Some(conteudo) = crate::qr::decode(image) {
        entregar(conteudo, shared, anchor, ctx);
        return;
    }
    recognize_text(image, shared, anchor, ctx);
}

/// Copia o que foi lido e abre o aviso. É o desfecho comum do QR e do OCR.
fn entregar(
    texto: String,
    shared: &Arc<Mutex<AppShared>>,
    anchor: (f32, f32),
    ctx: &egui::Context,
) {
    if let Err(err) = crate::clipboard::copy_text(&texto) {
        notify::toast_error("Falha ao copiar o texto", &format!("{err:#}"));
        shared.lock().unwrap().ocr_running = false;
        ctx.request_repaint();
        return;
    }

    // Copiado. Só agora o aviso aparece, e ele é opcional por natureza: se
    // esta janela falhasse em nascer, o texto já estaria na área de
    // transferência do mesmo jeito.
    {
        let mut shared = shared.lock().unwrap();
        shared.ocr_popup = Some(crate::ocr_popup::OcrPopup::new(texto, anchor));
        shared.ocr_running = false;
    }
    // A raiz está dormindo — esta thread é a única que sabe que há janela nova.
    ctx.request_repaint();
}

/// Reconhece o texto do recorte pelo motor do Windows.
///
/// Nenhuma janela se abre quando não há o que reconhecer — o aviso de sistema
/// é toda a resposta que esse desfecho dá.
#[cfg(feature = "ocr")]
fn recognize_text(
    image: &crate::imgbuf::RgbaImage,
    shared: &Arc<Mutex<AppShared>>,
    anchor: (f32, f32),
    ctx: &egui::Context,
) {
    let text = match crate::platform::ocr::recognize(image, None) {
        Ok(text) => text,
        Err(err) => {
            // Cobre tanto a falha do motor quanto o caso comum de não haver
            // texto nenhum na região escolhida.
            notify::toast_error("Nada reconhecido", &format!("{err}"));
            shared.lock().unwrap().ocr_running = false;
            ctx.request_repaint();
            return;
        }
    };
    entregar(text, shared, anchor, ctx);
}

/// Folga em volta de cada palavra, em px da imagem.
///
/// A caixa que o motor devolve encosta nos glifos; sem folga sobram fiapos de
/// letra nas bordas, e um fiapo de letra ainda é informação.
#[cfg(feature = "ocr")]
const WORD_PADDING: f32 = 2.0;

/// Reconhece as palavras da região e vira uma redação por palavra.
///
/// É **melhor-esforço**: onde o OCR não reconhece, nada é ocultado. A
/// mensagem final diz quantas palavras foram apagadas justamente para o
/// usuário poder conferir em vez de confiar.
#[cfg(feature = "ocr")]
fn redact_words(
    image: &crate::imgbuf::RgbaImage,
    regiao: (crate::editor::shapes::Point, crate::editor::shapes::Point),
    style: crate::editor::shapes::Style,
    shared: &Arc<Mutex<AppShared>>,
    ctx: &egui::Context,
) {
    use crate::editor::shapes::{Layer, Point, Shape};

    let terminar = |shared: &Arc<Mutex<AppShared>>, ctx: &egui::Context| {
        shared.lock().unwrap().ocr_running = false;
        ctx.request_repaint();
    };

    let (min, max) = regiao;
    let (x, y) = (min.x.max(0.0) as u32, min.y.max(0.0) as u32);
    let w = (max.x - min.x).max(0.0) as u32;
    let h = (max.y - min.y).max(0.0) as u32;
    if w == 0 || h == 0 {
        terminar(shared, ctx);
        return;
    }
    let recorte = image.crop(x, y, w, h);

    let caixas = match crate::platform::ocr::recognize_boxes(&recorte, None) {
        Ok(caixas) if !caixas.is_empty() => caixas,
        Ok(_) => {
            notify::toast_error(
                "Nenhuma palavra reconhecida",
                "Nada foi ocultado nessa região.",
            );
            terminar(shared, ctx);
            return;
        }
        Err(err) => {
            notify::toast_error("Nada reconhecido", &format!("{err}"));
            terminar(shared, ctx);
            return;
        }
    };

    // De volta às coordenadas da imagem: as caixas vêm relativas ao recorte.
    let quantas = caixas.len();
    let layers: Vec<Layer> = caixas
        .into_iter()
        .map(|c| Layer {
            id: 0, // `Document::paste` dá o id e a semente do mosaico.
            shape: Shape::Redaction {
                min: Point::new(
                    x as f32 + c.x - WORD_PADDING,
                    y as f32 + c.y - WORD_PADDING,
                ),
                max: Point::new(
                    x as f32 + c.x + c.w + WORD_PADDING,
                    y as f32 + c.y + c.h + WORD_PADDING,
                ),
                seed: 0,
            },
            style,
        })
        .collect();

    {
        let mut shared = shared.lock().unwrap();
        // O editor pode ter fechado enquanto o motor trabalhava.
        if let Flow::Editing(session) = &mut shared.flow {
            session.doc.paste(layers);
        }
        shared.ocr_running = false;
    }
    notify::toast(
        "Texto ocultado",
        &format!("{quantas} palavras apagadas. Confira: o que o OCR não reconhece fica visível."),
    );
    ctx.request_repaint();
}

/// Sem a feature `ocr`, o modo avisa em vez de fingir que ocultou.
#[cfg(not(feature = "ocr"))]
fn redact_words(
    _image: &crate::imgbuf::RgbaImage,
    _regiao: (crate::editor::shapes::Point, crate::editor::shapes::Point),
    _style: crate::editor::shapes::Style,
    shared: &Arc<Mutex<AppShared>>,
    ctx: &egui::Context,
) {
    notify::toast_error(
        "Reconhecimento de texto indisponível",
        "Esta build foi compilada sem a feature `ocr`.",
    );
    shared.lock().unwrap().ocr_running = false;
    ctx.request_repaint();
}

/// Sem a feature `ocr` o atalho continua existindo — e continua lendo QR, que
/// é código próprio —, mas avisa em vez de fingir que reconheceu texto:
/// silêncio aqui seria pior que a mensagem.
#[cfg(not(feature = "ocr"))]
fn recognize_text(
    _image: &crate::imgbuf::RgbaImage,
    shared: &Arc<Mutex<AppShared>>,
    _anchor: (f32, f32),
    ctx: &egui::Context,
) {
    notify::toast_error(
        "Reconhecimento de texto indisponível",
        "Esta build foi compilada sem a feature `ocr`.",
    );
    // Baixar a bandeira aqui também: sem isto o processo ficaria vivo para
    // sempre esperando um reconhecimento que nunca vai acontecer.
    shared.lock().unwrap().ocr_running = false;
    ctx.request_repaint();
}
