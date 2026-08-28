//! Processo residente: bandeja, atalhos globais e captura de tela cheia — tudo
//! em Win32 puro, sem eframe/wgpu (§5, RF-06).
//!
//! É o que fica de pé a sessão inteira, e por isso não pode tocar em nada de
//! GUI: um device D3D12 aberto 24/7 custava ~90 MB de working set. Quando um
//! fluxo precisa de janela (overlay de seleção, editor, configurações), o
//! residente lança `rustshot.exe --gui …` e o filho encerra ao terminar.
//!
//! A tela cheia (RF-01) não abre janela nenhuma: captura, codifica em thread de
//! trabalho e notifica, tudo aqui.
//!
//! Sentido inverso da conversa: o filho manda balões e o aviso de "config.json
//! regravado" por `WM_COPYDATA` (ver `platform::shell::IPC_*`).

// Fora do Windows não há bandeja nem processo de GUI para lançar: o módulo
// compila apenas para os testes de lógica encontrarem o crate inteiro, como já
// acontece em `platform::shell`.
#![cfg_attr(not(windows), allow(dead_code))]

use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crate::config::{self, Config, APP_NAME};
use crate::hotkeys::{HotkeyAction, Hotkeys};
use crate::platform::shell::{
    self, ShellEvent, IPC_BALLOON, IPC_CONFIG_CHANGED, IPC_EDITOR_OPEN,
};
use crate::storage::{self, SaveTarget};
use crate::tray::{self, Tray};
use crate::{capture, hotkeys, notify, platform};

/// Espera da captura com atraso. Três segundos é o bastante para abrir um
/// menu, e pouco o suficiente para não parecer que o app travou.
const CAPTURE_DELAY_SECS: u64 = 3;
/// Teto de passos da captura com rolagem. Uma página que não termina não
/// pode prender a bandeja para sempre.
const SCROLL_MAX_PASSOS: usize = 40;
/// Cliques de roda por passo. Poucos deixam a captura lenta; muitos passam
/// da altura da janela e abrem um buraco que a costura não tem como emendar.
const SCROLL_NOTCHES: i32 = 3;
/// Espera até a página assentar, em ms. Rolagem suave leva um tempo, e
/// capturar no meio dela devolve um quadro borrado.
const SCROLL_SETTLE_MS: u64 = 320;

/// Fila preenchida pelo `WndProc` e drenada fora dele (ver `run_message_loop`).
mod events {
    use super::ShellEvent;
    use std::sync::Mutex;

    static QUEUE: Mutex<Vec<ShellEvent>> = Mutex::new(Vec::new());

    pub fn push(event: ShellEvent) {
        if let Ok(mut queue) = QUEUE.lock() {
            queue.push(event);
        }
    }

    pub fn drain() -> Vec<ShellEvent> {
        QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }
}

/// Um processo de GUI em andamento e o bloco compartilhado que ele está lendo.
struct GuiChild {
    child: Child,
    /// O filho já abriu o editor? Enquanto está só no overlay, acionar o
    /// atalho de novo o encerra; depois do editor, não — haveria trabalho
    /// do usuário lá dentro.
    editing: bool,
    /// Mantido vivo até o filho encerrar: fechar o mapeamento antes destruiria
    /// o objeto e o filho abriria o vazio.
    #[cfg(windows)]
    _shots: Option<platform::ipc::SharedShots>,
}

pub struct Resident {
    config: Config,
    hotkeys: Hotkeys,
    /// Mantém ícone e menu da bandeja vivos (RF-06).
    tray: Option<Tray>,
    /// Overlay de seleção / editor (um por vez, §5).
    capture_gui: Option<GuiChild>,
    /// Janela de configurações (pode coexistir com uma captura).
    settings_gui: Option<GuiChild>,
    last_busy_toast: Option<Instant>,
    /// Há working set a devolver quando o último trabalho terminar.
    trim_pending: bool,
    quit: bool,
}

/// Sobe a bandeja e bombeia mensagens até o "Sair" do menu.
pub fn run(loaded: config::LoadedConfig) {
    let mut resident = Resident::new(loaded);
    shell::run_message_loop(|| resident.process_events());
    resident.shutdown();
}

impl Resident {
    fn new(loaded: config::LoadedConfig) -> Self {
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

        // Uma sessão de edição gravada só sobra quando o editor não fechou
        // por vontade do usuário: é exatamente o caso de oferecer recuperar.
        let recoverable = crate::editor::session_file::exists(&config::state_dir());
        if recoverable {
            notify::toast(
                "Edição não salva encontrada",
                "Use \"Recuperar edição não salva\" no menu da bandeja.",
            );
        }
        let repeatable = crate::last_region::load().is_some();
        let tray = match Tray::new(
            config.start_with_windows,
            recoverable,
            repeatable,
            events::push,
        ) {
            Ok(tray) => Some(tray),
            Err(err) => {
                notify::toast_error(
                    "Bandeja indisponível",
                    &format!("O menu da bandeja não pôde ser criado: {err}"),
                );
                None
            }
        };

        let mut hotkeys = Hotkeys::new();
        toast_hotkey_failures(hotkeys.apply(&config.hotkeys));

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

        Self {
            config,
            hotkeys,
            tray,
            capture_gui: None,
            settings_gui: None,
            last_busy_toast: None,
            trim_pending: false,
            quit: false,
        }
    }

    /// Drena a fila do `WndProc` e recolhe processos de GUI encerrados.
    fn process_events(&mut self) {
        for event in events::drain() {
            match event {
                ShellEvent::Hotkey(id) => {
                    if let Some(action) = self.hotkeys.action_for(id) {
                        log::info!("atalho global: {action:?}");
                        self.trigger(action);
                    }
                }
                ShellEvent::Menu(id) => self.handle_menu(id),
                ShellEvent::Ipc { kind, payload } => self.handle_ipc(kind, &payload),
            }
        }
        self.poll_background();
        if self.quit {
            shell::post_quit();
        }
    }

    fn shutdown(&mut self) {
        // Processos de GUI abertos seguem vivos de propósito: encerrar o
        // residente não pode descartar uma edição em andamento. Os atalhos
        // morrem com o processo; o ícone precisa ser removido explicitamente
        // (é o que o Drop de Tray faz).
        self.tray = None;
        log::info!("residente encerrado");
    }

    // -----------------------------------------------------------------------
    // Fluxos de captura
    // -----------------------------------------------------------------------

    fn trigger(&mut self, action: HotkeyAction) {
        match action {
            HotkeyAction::Fullscreen => self.capture_fullscreen(),
            HotkeyAction::Region => self.launch_select(GuiPurpose::Region),
            HotkeyAction::Edit => self.launch_select(GuiPurpose::Edit),
            HotkeyAction::Ocr => self.launch_select(GuiPurpose::Ocr),
        }
    }

    /// RF-01: nenhuma janela envolvida — captura, salva em thread de trabalho e
    /// notifica.
    /// Captura a tela cheia depois de alguns segundos, para dar tempo de
    /// abrir um menu ou posicionar o cursor.
    ///
    /// A espera roda em thread de trabalho e a captura volta para a fila de
    /// eventos: capturar de fora da thread da bandeja mexeria em GDI de outro
    /// contexto, e bloquear a thread de mensagens congelaria o ícone.
    fn capture_after_delay(&mut self) {
        notify::toast("Capturando em 3 segundos", "Prepare a tela.");
        crate::jobs::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(CAPTURE_DELAY_SECS));
            events::push(shell::ShellEvent::Menu(tray::MENU_CAPTURE_FULLSCREEN));
        });
    }

    /// Recaptura o mesmo retângulo da última região, sem passar pelo overlay.
    fn repeat_last_region(&mut self) {
        let Some((x, y, w, h)) = crate::last_region::load() else {
            notify::toast_error("Nada a repetir", "Capture uma região primeiro.");
            return;
        };
        let shots = match capture::capture_all_monitors() {
            Ok(shots) => shots,
            Err(err) => {
                notify::toast_error("Falha na captura", &format!("{err:#}"));
                return;
            }
        };
        // O monitor que contém o canto superior esquerdo manda: um retângulo
        // que atravessa dois monitores já não era representável na seleção,
        // então aqui também não precisa ser.
        let dono = shots.iter().find(|s| {
            x >= s.x && y >= s.y && x < s.x + s.width as i32 && y < s.y + s.height as i32
        });
        let Some(shot) = dono else {
            notify::toast_error(
                "A região não cabe mais na tela",
                "Os monitores mudaram desde a última captura.",
            );
            return;
        };
        let local_x = (x - shot.x) as u32;
        let local_y = (y - shot.y) as u32;
        let w = w.min(shot.width - local_x);
        let h = h.min(shot.height - local_y);
        let image = capture::crop(&shot.image, local_x, local_y, w, h);
        let destino = self.config.after_region;
        if destino.copies() {
            let copia = image.clone();
            crate::jobs::spawn(move || match crate::clipboard::copy_image(&copia) {
                Ok(()) => notify::toast(
                    "Copiado para a área de transferência",
                    "A mesma região da vez anterior.",
                ),
                Err(err) => notify::toast_error("Falha ao copiar", &format!("{err:#}")),
            });
        }
        if destino.saves() {
            storage::save_in_background(SaveTarget::from_config(&self.config), image);
        }
        shell::set_poll_timer(true);
    }

    /// Captura com rolagem: costura uma página mais alta que a tela.
    ///
    /// Roda **na thread da bandeja**, com pausas entre os passos. Ela fica
    /// travada por alguns segundos, e isso é aceitável: a captura precisa da
    /// GDI desta thread, e o programa que rola tem a fila de mensagens dele,
    /// que continua andando. Um aviso avisa antes, e outro conta o resultado.
    #[cfg(windows)]
    fn capture_scrolling(&mut self) {
        use crate::stitch::Stitcher;

        let Some((x, y)) = crate::platform::scroll::cursor_pos() else {
            notify::toast_error("Captura com rolagem", "Não foi possível ler o cursor.");
            return;
        };
        let janelas = platform::window_list::visible_windows();
        let Some(alvo) = platform::window_list::window_at(&janelas, x, y)
            .map(|i| &janelas[i])
            .map(|j| (j.x, j.y, j.width, j.height))
        else {
            notify::toast_error(
                "Captura com rolagem",
                "Aponte o cursor para a janela que deve rolar.",
            );
            return;
        };

        notify::toast(
            "Capturando com rolagem",
            "Não mexa no mouse nem no teclado até o aviso do fim.",
        );

        let recorte = |shots: &[capture::MonitorShot]| -> Option<crate::imgbuf::RgbaImage> {
            // A janela pode atravessar monitores; vale o que contém o cursor.
            let shot = shots.iter().find(|s| {
                x >= s.x && y >= s.y && x < s.x + s.width as i32 && y < s.y + s.height as i32
            })?;
            let (lx, ly) = ((alvo.0 - shot.x).max(0) as u32, (alvo.1 - shot.y).max(0) as u32);
            let w = (alvo.2.min(shot.width.saturating_sub(lx))).min(shot.width);
            let h = (alvo.3.min(shot.height.saturating_sub(ly))).min(shot.height);
            (w > 0 && h > 0).then(|| capture::crop(&shot.image, lx, ly, w, h))
        };

        let Some(primeiro) = capture::capture_all_monitors().ok().and_then(|s| recorte(&s)) else {
            notify::toast_error("Captura com rolagem", "Falha ao capturar a janela.");
            return;
        };
        let mut costura = Stitcher::new(primeiro);

        for _ in 0..SCROLL_MAX_PASSOS {
            if !crate::platform::scroll::wheel_at(x, y, -SCROLL_NOTCHES) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(SCROLL_SETTLE_MS));
            let Some(quadro) = capture::capture_all_monitors().ok().and_then(|s| recorte(&s))
            else {
                break;
            };
            // Zero px novos: a página parou, ou o quadro saiu borrado no meio
            // de uma rolagem suave. Nos dois casos não há o que emendar.
            if costura.push(&quadro) == 0 {
                break;
            }
        }

        let altura = costura.height();
        let imagem = costura.finish();
        storage::save_in_background(SaveTarget::from_config(&self.config), imagem);
        notify::toast("Captura com rolagem", &format!("{altura} px de altura."));
        shell::set_poll_timer(true);
    }

    #[cfg(not(windows))]
    fn capture_scrolling(&mut self) {}

    fn capture_fullscreen(&mut self) {
        let target = SaveTarget::from_config(&self.config);
        match capture::capture_fullscreen(self.config.fullscreen_scope) {
            Ok(image) => {
                let destino = self.config.after_fullscreen;
                if destino.copies() {
                    // A cópia primeiro: ela é rápida e é o que o usuário está
                    // esperando para colar. O arquivo pode demorar a codificar.
                    let copia = image.clone();
                    crate::jobs::spawn(move || {
                        if let Err(err) = crate::clipboard::copy_image(&copia) {
                            notify::toast_error("Falha ao copiar", &format!("{err:#}"));
                        } else if !destino.saves() {
                            notify::toast(
                                "Copiado para a área de transferência",
                                "A tela cheia está pronta para colar.",
                            );
                        }
                    });
                }
                if destino.saves() {
                    storage::save_in_background(target, image);
                }
                // O trim fica para depois: a codificação ainda vai percorrer a
                // imagem inteira, e tirar essas páginas agora só geraria falta
                // de página. Quem enxuga é o `poll_background`.
                shell::set_poll_timer(true);
            }
            Err(err) => notify::toast_error("Falha na captura", &format!("{err:#}")),
        }
    }

    /// RF-02/RF-03: a captura acontece **aqui**, antes de qualquer janela, para
    /// a tela ficar congelada no instante do atalho; os pixels vão ao filho por
    /// memória compartilhada.
    #[cfg(windows)]
    fn launch_select(&mut self, purpose: GuiPurpose) {
        // Acionar o atalho com o overlay na tela fecha o overlay: é o mesmo
        // gesto para abrir e para desistir. Com o editor já aberto, não —
        // ali existe trabalho que um atalho não pode jogar fora.
        if let Some(gui) = &mut self.capture_gui {
            if gui.editing {
                self.busy_toast();
            } else {
                let _ = gui.child.kill();
                self.capture_gui = None;
                log::info!("overlay dispensado pelo mesmo atalho");
            }
            return;
        }

        // As janelas são listadas aqui, no mesmo instante da captura: se o
        // processo de GUI as enumerasse ao subir, uma janela movida nesse
        // intervalo apareceria fora de lugar sobre os pixels congelados.
        let windows = platform::window_list::visible_windows();
        let published = match capture::capture_all_monitors()
            .and_then(|shots| platform::ipc::publish(&shots, &windows))
        {
            Ok(published) => published,
            Err(err) => {
                notify::toast_error("Falha na captura", &format!("{err:#}"));
                return;
            }
        };

        let args = [
            "--gui",
            "select",
            "--shots",
            published.name(),
            "--len",
            &published.len().to_string(),
            "--purpose",
            purpose.as_arg(),
            "--parent",
            &shell::hwnd_value().to_string(),
        ];
        match self.spawn_gui(&args) {
            Ok(child) => {
                self.capture_gui = Some(GuiChild { child, editing: false, _shots: Some(published) });
                shell::set_poll_timer(true);
            }
            Err(err) => notify::toast_error("Falha ao abrir a seleção", &format!("{err}")),
        }
    }

    #[cfg(not(windows))]
    fn launch_select(&mut self, _purpose: GuiPurpose) {
        notify::toast_error("Indisponível", "Seleção de região só existe no Windows.");
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

    /// `rustshot.exe --gui …` — o mesmo executável, no modo de GUI.
    fn spawn_gui(&self, args: &[&str]) -> std::io::Result<Child> {
        let exe = std::env::current_exe()?;
        log::info!("abrindo processo de GUI: {}", args.join(" "));
        Command::new(exe).args(args).spawn()
    }

    /// Recolhe processos de GUI encerrados — o que libera o bloco compartilhado,
    /// dezenas de MB — e, quando nada mais está pendente, enxuga o working set e
    /// desliga o despertador (idle volta a ser event-driven).
    fn poll_background(&mut self) {
        let mut finished = false;
        for slot in [&mut self.capture_gui, &mut self.settings_gui] {
            let done = slot
                .as_mut()
                .is_some_and(|gui| matches!(gui.child.try_wait(), Ok(Some(_)) | Err(_)));
            if done {
                *slot = None;
                finished = true;
            }
        }

        let idle = self.capture_gui.is_none()
            && self.settings_gui.is_none()
            && crate::jobs::pending() == 0;
        if idle {
            if finished || self.trim_pending {
                platform::memory::trim_working_set();
            }
            self.trim_pending = false;
            shell::set_poll_timer(false);
        } else {
            // Algo ainda está em curso; quando acabar, o working set precisa
            // voltar ao tamanho de bandeja.
            self.trim_pending = true;
        }
    }

    // -----------------------------------------------------------------------
    // Menu da bandeja
    // -----------------------------------------------------------------------

    fn handle_menu(&mut self, id: u16) {
        match id {
            tray::MENU_CAPTURE_FULLSCREEN => self.trigger(HotkeyAction::Fullscreen),
            tray::MENU_CAPTURE_SCROLL => self.capture_scrolling(),
            tray::MENU_CAPTURE_REGION => self.trigger(HotkeyAction::Region),
            tray::MENU_CAPTURE_EDIT => self.trigger(HotkeyAction::Edit),
            tray::MENU_CAPTURE_DELAYED => self.capture_after_delay(),
            tray::MENU_REPEAT_REGION => self.repeat_last_region(),
            tray::MENU_OPEN_FOLDER => open_folder(&self.config.effective_output_dir()),
            tray::MENU_SETTINGS => self.open_settings(),
            tray::MENU_AUTOSTART => self.toggle_autostart(),
            tray::MENU_RECOVER => self.launch_recover(),
            tray::MENU_QUIT => self.quit = true,
            other => log::debug!("item de menu desconhecido: {other:#06x}"),
        }
    }

    fn open_settings(&mut self) {
        // Já aberta: traz para frente em vez de abrir uma segunda janela.
        if self.settings_gui.is_some() {
            shell::focus_window(crate::settings::WINDOW_TITLE);
            return;
        }
        let args = ["--gui", "settings", "--parent", &shell::hwnd_value().to_string()];
        match self.spawn_gui(&args) {
            Ok(child) => {
                self.settings_gui = Some(GuiChild {
                    editing: true,
                    child,
                    #[cfg(windows)]
                    _shots: None,
                });
                shell::set_poll_timer(true);
            }
            Err(err) => {
                notify::toast_error("Falha ao abrir as configurações", &format!("{err}"));
            }
        }
    }

    fn toggle_autostart(&mut self) {
        let wanted = !self.config.start_with_windows;
        match apply_autostart(wanted) {
            Ok(()) => {
                self.config.start_with_windows = wanted;
                if let Err(err) = config::save(&self.config) {
                    log::warn!("falha ao salvar config: {err:#}");
                }
                notify::toast(
                    "Iniciar com o Windows",
                    if wanted { "Ativado." } else { "Desativado." },
                );
            }
            Err(err) => notify::toast_error(
                "Falha ao alterar inicialização automática",
                &format!("{err:#}"),
            ),
        }
        if let Some(tray) = &self.tray {
            tray.set_autostart_checked(self.config.start_with_windows);
        }
    }

    // -----------------------------------------------------------------------
    // Mensagens dos processos de GUI
    // -----------------------------------------------------------------------

    fn handle_ipc(&mut self, kind: u32, payload: &str) {
        match kind {
            IPC_BALLOON => {
                let (title, text) = payload.split_once('\n').unwrap_or((payload, ""));
                notify::toast(title, text);
            }
            IPC_CONFIG_CHANGED => self.reload_config(),
            IPC_EDITOR_OPEN => {
                if let Some(gui) = &mut self.capture_gui {
                    gui.editing = true;
                }
            }
            other => log::debug!("mensagem IPC desconhecida: {other}"),
        }
    }

    /// Abre o editor sobre a sessão gravada.
    #[cfg(windows)]
    fn launch_recover(&mut self) {
        if self.capture_gui.is_some() {
            self.busy_toast();
            return;
        }
        let args = ["--recover", "--parent", &shell::hwnd_value().to_string()];
        match self.spawn_gui(&args) {
            Ok(child) => {
                // Já entra como edição: há trabalho do usuário dentro dela
                // desde o primeiro quadro.
                self.capture_gui = Some(GuiChild { child, editing: true, _shots: None });
            }
            Err(err) => notify::toast_error("Falha ao recuperar", &format!("{err:#}")),
        }
    }

    #[cfg(not(windows))]
    fn launch_recover(&mut self) {}

    /// A janela de configurações gravou o `config.json`: o residente é o dono
    /// dos atalhos e do registro, então relê e reaplica (RF-05, sem reiniciar).
    fn reload_config(&mut self) {
        let loaded = config::load();
        self.config = loaded.config;

        let failures = self.hotkeys.apply(&self.config.hotkeys);
        if let Err(err) = apply_autostart(self.config.start_with_windows) {
            notify::toast_error(
                "Falha ao alterar inicialização automática",
                &format!("{err:#}"),
            );
        }
        if let Some(tray) = &self.tray {
            tray.set_autostart_checked(self.config.start_with_windows);
        }

        if failures.is_empty() {
            notify::toast("Configurações aplicadas", "Alterações em vigor.");
        } else {
            toast_hotkey_failures(failures);
        }
    }
}

/// Qual fluxo o processo de GUI deve executar com as capturas recebidas.
#[derive(Clone, Copy)]
enum GuiPurpose {
    Region,
    Edit,
    Ocr,
}

impl GuiPurpose {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::Edit => "edit",
            Self::Ocr => "ocr",
        }
    }
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

fn open_folder(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(windows)]
    let result = Command::new("explorer").arg(dir).spawn();
    #[cfg(not(windows))]
    let result = Command::new("xdg-open").arg(dir).spawn();
    if let Err(err) = result {
        notify::toast_error(
            "Não foi possível abrir a pasta",
            &format!("{}: {err}", dir.display()),
        );
    }
}
