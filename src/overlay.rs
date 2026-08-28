//! Overlay de seleção de região (RF-02, §10): uma janela por monitor, sem
//! bordas, sempre no topo, fora do Alt-Tab, exibindo a captura congelada
//! daquele monitor com véu escuro (~60%).
//!
//! Fluxo "Capturar região": soltar o arrasto copia a região para a área de
//! transferência e encerra. Não há passo de escolha — capturar uma região é
//! quase sempre para colar em seguida, e perguntar o destino cobrava uma
//! tecla a mais de todo mundo para servir ao caso raro. `Esc`/botão direito
//! cancelam. Salvar em arquivo continua no fluxo de tela cheia e no editor.
//! Fluxo "Capturar e editar": soltar o arrasto abre o editor imediatamente.
//!
//! Coordenadas: cada janela cobre exatamente o seu monitor, então
//! `pontos × pixels_per_point` == px físicos do monitor == px da imagem
//! congelada. A seleção é armazenada em px da imagem do monitor onde o
//! arrasto começou e fica contida nele (limitação v1, §7).
//!
//! Posicionamento: o `ViewportBuilder` inicial usa coordenadas lógicas do
//! winit (imprecisas com escalas mistas); a cada frame o viewport confere sua
//! posição/tamanho físicos reais e se auto-corrige via
//! `ViewportCommand::OuterPosition`/`InnerSize` — que o egui converte para
//! físico multiplicando pelo `pixels_per_point` atual da própria janela.

use egui::{
    Color32, ColorImage, CornerRadius, CursorIcon, FontId, Pos2, Rect, Stroke, StrokeKind,
    TextureOptions, Vec2, ViewportCommand,
};

use crate::capture::MonitorShot;
use crate::platform::window_list::{self, WindowTarget};

/// Véu preto ~60% (RF-02).
const VEIL_ALPHA: u8 = 153;
/// Seleções menores que isso (px) são ignoradas — clique acidental.
const MIN_SELECTION_PX: f32 = 3.0;

/// O que fazer com a região confirmada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    /// RF-02: copia a região ao soltar o arrasto, sem perguntar o destino.
    CopyDirect,
    /// RF-03: recorta e abre o editor ao soltar o arrasto.
    Edit,
    /// Reconhece o texto do recorte e o copia, sem abrir janela nenhuma.
    Ocr,
}

/// Destino escolhido para a região confirmada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectedAction {
    /// Fluxo "capturar região": copiar o recorte para a área de transferência.
    CopyToClipboard,
    /// Fluxo "capturar e editar": abrir o editor com o recorte.
    OpenEditor,
    /// Fluxo "reconhecer texto": o recorte vai para o OCR, e o texto para a
    /// área de transferência.
    RecognizeText,
}

/// Resultado terminal de uma sessão de seleção.
pub enum Outcome {
    Cancelled,
    Selected {
        monitor: usize,
        /// (x, y, largura, altura) em px da imagem do monitor.
        rect: (u32, u32, u32, u32),
        action: SelectedAction,
    },
}

/// Arrasto em andamento, em px da imagem do monitor `monitor`.
struct Drag {
    monitor: usize,
    start: (f32, f32),
    current: (f32, f32),
}

/// Um monitor participando do overlay.
pub struct OverlayMonitor {
    pub shot: MonitorShot,
    texture: Option<egui::TextureHandle>,
}

/// Sessão de seleção de região (estado compartilhado entre os viewports).
pub struct SelectSession {
    pub serial: u64,
    pub purpose: Purpose,
    pub monitors: Vec<OverlayMonitor>,
    /// Janelas visíveis no instante da captura, em px do desktop virtual.
    windows: Vec<WindowTarget>,
    /// `true` = escolher uma janela; `false` = arrastar uma região.
    window_mode: bool,
    /// Janela em destaque (índice em `windows`).
    hovered_window: Option<usize>,
    drag: Option<Drag>,
    pub outcome: Option<Outcome>,
}

impl SelectSession {
    /// Cria a sessão e já sobe as texturas das capturas para a GPU pelo
    /// contexto compartilhado: o primeiro frame de cada overlay só desenha,
    /// eliminando a janela preta entre a criação da janela e a primeira
    /// pintura (o upload de um 4K leva dezenas de ms).
    pub fn new(
        ctx: &egui::Context,
        serial: u64,
        shots: Vec<MonitorShot>,
        windows: Vec<WindowTarget>,
        purpose: Purpose,
    ) -> Self {
        Self {
            serial,
            purpose,
            windows,
            window_mode: false,
            hovered_window: None,
            monitors: shots
                .into_iter()
                .enumerate()
                .map(|(idx, shot)| {
                    let color = ColorImage::from_rgba_unmultiplied(
                        [shot.image.width() as usize, shot.image.height() as usize],
                        shot.image.as_raw(),
                    );
                    let texture = ctx.load_texture(
                        format!("overlay_{serial}_{idx}"),
                        color,
                        TextureOptions::NEAREST,
                    );
                    OverlayMonitor { shot, texture: Some(texture) }
                })
                .collect(),
            drag: None,
            outcome: None,
        }
    }

    /// Builder do viewport do monitor `idx` (posição inicial aproximada; a
    /// correção fina acontece no primeiro frame do próprio viewport).
    pub fn viewport_builder(&self, idx: usize) -> egui::ViewportBuilder {
        let shot = &self.monitors[idx].shot;
        let scale = shot.scale.max(0.5);
        egui::ViewportBuilder::default()
            .with_title("RustShot — seleção de região")
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
            .with_always_on_top()
            .with_position(Pos2::new(shot.x as f32 / scale, shot.y as f32 / scale))
            .with_inner_size(Vec2::new(
                shot.width as f32 / scale,
                shot.height as f32 / scale,
            ))
            .with_active(idx == 0)
    }
}

/// Confirma uma região do monitor `idx`.
///
/// Os três fluxos resolvem ao soltar o arrasto: o destino já está decidido
/// pelo atalho que abriu o overlay, e não há o que perguntar depois.
fn confirm_region(session: &mut SelectSession, idx: usize, rect: (u32, u32, u32, u32)) {
    let action = match session.purpose {
        Purpose::Edit => SelectedAction::OpenEditor,
        Purpose::Ocr => SelectedAction::RecognizeText,
        Purpose::CopyDirect => SelectedAction::CopyToClipboard,
    };
    session.outcome = Some(Outcome::Selected { monitor: idx, rect, action });
}

/// Retângulo da janela `i` em coordenadas do monitor `idx`, recortado a ele.
/// `None` quando a janela não encosta neste monitor.
fn window_rect_on(
    session: &SelectSession,
    idx: usize,
    window: usize,
) -> Option<(u32, u32, u32, u32)> {
    let shot = &session.monitors.get(idx)?.shot;
    let w = session.windows.get(window)?;
    let x0 = (w.x - shot.x).max(0);
    let y0 = (w.y - shot.y).max(0);
    let x1 = (w.x - shot.x + w.width as i32).min(shot.width as i32);
    let y1 = (w.y - shot.y + w.height as i32).min(shot.height as i32);
    (x1 > x0 && y1 > y0).then(|| (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// UI de um viewport de overlay (um monitor). Deve ser chamada com a sessão
/// ativa; define `outcome` quando o usuário confirma ou cancela.
pub fn overlay_ui(ctx: &egui::Context, session: &mut SelectSession, idx: usize) {
    let ppp = ctx.input(|i| i.pixels_per_point());

    ensure_geometry(ctx, session, idx, ppp);

    let (img_w, img_h) = {
        let shot = &session.monitors[idx].shot;
        (shot.width as f32, shot.height as f32)
    };

    // Cancelamento: Esc, botão direito, ou fechamento externo da janela.
    let (esc, secondary, close_req) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Escape),
            i.pointer.secondary_pressed(),
            i.viewport().close_requested(),
        )
    });
    if esc || secondary || close_req {
        session.outcome = Some(Outcome::Cancelled);
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        return;
    }

    // --- Modo janela: Space alterna, setas navegam, Enter confirma ---
    let (space, select_all, enter, arrow) = ctx.input_mut(|i| {
        let arrow = [
            (egui::Key::ArrowLeft, (-1, 0)),
            (egui::Key::ArrowRight, (1, 0)),
            (egui::Key::ArrowUp, (0, -1)),
            (egui::Key::ArrowDown, (0, 1)),
        ]
        .into_iter()
        .find(|(key, _)| i.consume_key(egui::Modifiers::NONE, *key))
        .map(|(_, delta)| delta);
        (
            i.consume_key(egui::Modifiers::NONE, egui::Key::Space),
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            arrow,
        )
    });
    if space && !session.windows.is_empty() {
        session.window_mode = !session.window_mode;
        session.drag = None;
        session.hovered_window = None;
    }
    if select_all {
        // O monitor inteiro, sem precisar arrastar de canto a canto.
        confirm_region(session, idx, (0, 0, img_w as u32, img_h as u32));
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        return;
    }
    if session.window_mode {
        if let Some((dx, dy)) = arrow {
            session.hovered_window =
                window_list::window_in_direction(&session.windows, session.hovered_window, dx, dy)
                    .or(session.hovered_window);
        }
        if enter {
            if let Some(rect) = session
                .hovered_window
                .and_then(|w| window_rect_on(session, idx, w))
            {
                confirm_region(session, idx, rect);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
                return;
            }
        }
    }

    ctx.output_mut(|o| o.cursor_icon = CursorIcon::Crosshair);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            let full = ui.max_rect();
            let painter = ui.painter();

            // Captura congelada do monitor, 1:1 em px físicos.
            let texture = {
                let monitor = &mut session.monitors[idx];
                monitor
                    .texture
                    .get_or_insert_with(|| {
                        let img = &monitor.shot.image;
                        let color = ColorImage::from_rgba_unmultiplied(
                            [img.width() as usize, img.height() as usize],
                            img.as_raw(),
                        );
                        ctx.load_texture(
                            format!("overlay_{}_{idx}", session.serial),
                            color,
                            TextureOptions::NEAREST,
                        )
                    })
                    .clone()
            };
            // 1:1 exato: destino com o tamanho da imagem (independe de o rect
            // da janela divergir por arredondamento).
            let image_rect = Rect::from_min_size(
                full.min,
                Vec2::new(img_w / ppp, img_h / ppp),
            );
            painter.image(
                texture.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.rect_filled(full, 0.0, Color32::from_black_alpha(VEIL_ALPHA));

            // --- Entrada do mouse (px físicos == px da imagem do monitor) ---
            let pointer_pts = ctx.input(|i| i.pointer.latest_pos());
            let pointer_px = pointer_pts.map(|p| {
                (
                    (p.x - full.min.x) * ppp,
                    (p.y - full.min.y) * ppp,
                )
            });
            let clamp = |(x, y): (f32, f32)| (x.clamp(0.0, img_w), y.clamp(0.0, img_h));

            let (primary_pressed, primary_down, primary_released, press_origin) = ctx.input(|i| {
                (
                    i.pointer.primary_pressed(),
                    i.pointer.primary_down(),
                    i.pointer.primary_released(),
                    i.pointer.press_origin(),
                )
            });

            // No modo janela o ponteiro destaca, e o clique confirma: não
            // há arrasto.
            if session.window_mode {
                if let Some((px, py)) = pointer_px {
                    let shot = &session.monitors[idx].shot;
                    let (gx, gy) = (shot.x + px as i32, shot.y + py as i32);
                    session.hovered_window = window_list::window_at(&session.windows, gx, gy);
                }
                if primary_pressed {
                    if let Some(rect) = session
                        .hovered_window
                        .and_then(|w| window_rect_on(session, idx, w))
                    {
                        confirm_region(session, idx, rect);
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                        return;
                    }
                }
            }

            if !session.window_mode && primary_pressed && session.drag.is_none() {
                // Origem do próprio clique — `latest_pos()` pode estar
                // obsoleto no frame do press logo após a janela nascer
                // (cursor "teleportado" sem WM_MOUSEMOVE intermediário).
                let origin = press_origin.or(pointer_pts).map(|p| {
                    ((p.x - full.min.x) * ppp, (p.y - full.min.y) * ppp)
                });
                if let Some(p) = origin {
                    log::debug!(
                        "overlay {idx}: press origin={press_origin:?} pts={pointer_pts:?} \
                         px={p:?} ppp={ppp} full={full:?} img={img_w}x{img_h}"
                    );
                    if p.0 >= 0.0 && p.1 >= 0.0 && p.0 <= img_w && p.1 <= img_h {
                        let p = clamp(p);
                        session.drag = Some(Drag { monitor: idx, start: p, current: p });
                    }
                }
            }
            if let Some(drag) = &mut session.drag {
                if drag.monitor == idx {
                    if primary_down {
                        if let Some(p) = pointer_px {
                            let mut p = clamp(p);
                            // `Shift` trava a seleção num quadrado, pelo eixo
                            // mais esticado — o editor já se comporta assim
                            // nas formas, e a mão espera o mesmo aqui.
                            if ctx.input(|i| i.modifiers.shift) {
                                let lado = (p.0 - drag.start.0)
                                    .abs()
                                    .max((p.1 - drag.start.1).abs());
                                p.0 = drag.start.0 + lado.copysign(p.0 - drag.start.0);
                                p.1 = drag.start.1 + lado.copysign(p.1 - drag.start.1);
                                p = clamp(p);
                            }
                            drag.current = p;
                        }
                    }
                    if primary_released {
                        let drag = session.drag.take().expect("drag presente");
                        log::debug!(
                            "overlay {idx}: release start={:?} current={:?} ppp={ppp}",
                            drag.start,
                            drag.current
                        );
                        let (x0, x1) = ordered(drag.start.0, drag.current.0);
                        let (y0, y1) = ordered(drag.start.1, drag.current.1);
                        if x1 - x0 >= MIN_SELECTION_PX && y1 - y0 >= MIN_SELECTION_PX {
                            let x = x0.floor().max(0.0) as u32;
                            let y = y0.floor().max(0.0) as u32;
                            let w = ((x1 - x0).round() as u32).max(1).min(img_w as u32 - x);
                            let h = ((y1 - y0).round() as u32).max(1).min(img_h as u32 - y);
                            // Os três fluxos resolvem aqui: o destino veio do
                            // atalho que abriu o overlay.
                            confirm_region(session, idx, (x, y, w, h));
                            ctx.request_repaint_of(egui::ViewportId::ROOT);
                            return;
                        }
                        // Clique sem arrasto: continua selecionando.
                    }
                }
            }

            // --- Modo janela: contorno de todas e destaque da escolhida ---
            if session.window_mode {
                let faint = Stroke::new(1.0_f32, Color32::from_white_alpha(72));
                for i in 0..session.windows.len() {
                    if Some(i) == session.hovered_window {
                        continue;
                    }
                    if let Some((x, y, w, h)) = window_rect_on(session, idx, i) {
                        let rect = Rect::from_min_size(
                            full.min + Vec2::new(x as f32 / ppp, y as f32 / ppp),
                            Vec2::new(w as f32 / ppp, h as f32 / ppp),
                        );
                        painter.rect_stroke(rect, 0.0, faint, egui::StrokeKind::Inside);
                    }
                }
                if let Some((x, y, w, h)) =
                    session.hovered_window.and_then(|i| window_rect_on(session, idx, i))
                {
                    // A janela escolhida aparece sem véu, como a região
                    // selecionada — é o mesmo gesto, outro jeito de mirar.
                    let rect = draw_selection(
                        painter,
                        texture.id(),
                        full,
                        ppp,
                        img_w,
                        img_h,
                        x as f32,
                        y as f32,
                        (x + w) as f32,
                        (y + h) as f32,
                    );
                    let title = session
                        .hovered_window
                        .and_then(|i| session.windows.get(i))
                        .map(|target| target.title.as_str())
                        .unwrap_or("");
                    badge(
                        painter,
                        full,
                        Pos2::new(rect.center().x, rect.min.y - 12.0),
                        &format!("{title}  ·  {w} × {h} px"),
                    );
                }
            }

            // --- Posição do ponteiro, enquanto não há nada selecionado ---
            if let Some(p) = pointer_pts {
                let idle = session.drag.is_none() && !session.window_mode;
                if idle {
                    if let Some((px, py)) = pointer_px {
                        badge(
                            painter,
                            full,
                            Pos2::new(p.x + 18.0, p.y + 18.0),
                            &format!("{}, {}", px.round() as i32, py.round() as i32),
                        );
                    }
                }
            }

            // --- Guias em cruz sob o cursor (só antes de haver seleção) ---
            if let Some(p) = pointer_pts {
                if session.drag.is_none() && !session.window_mode {
                    let guide = Stroke::new(1.0_f32, Color32::from_white_alpha(70));
                    painter.line_segment(
                        [Pos2::new(full.min.x, p.y), Pos2::new(full.max.x, p.y)],
                        guide,
                    );
                    painter.line_segment(
                        [Pos2::new(p.x, full.min.y), Pos2::new(p.x, full.max.y)],
                        guide,
                    );
                }
            }

            // --- Seleção ativa neste monitor: área clara + borda + badge ---
            if let Some(drag) = &session.drag {
                if drag.monitor == idx {
                    let (x0, x1) = ordered(drag.start.0, drag.current.0);
                    let (y0, y1) = ordered(drag.start.1, drag.current.1);
                    let sel_pts =
                        draw_selection(painter, texture.id(), full, ppp, img_w, img_h, x0, y0, x1, y1);

                    // Badge "L × A px" junto ao cursor.
                    let w_px = (x1 - x0).round() as u32;
                    let h_px = (y1 - y0).round() as u32;
                    let text = format!("{w_px} × {h_px} px");
                    let anchor = pointer_pts.unwrap_or(sel_pts.max) + Vec2::new(16.0, 18.0);
                    badge(painter, full, anchor, &text);
                }
            }

            // Dica de uso (fora do arrasto).
            if session.drag.is_none() {
                let hint = match session.purpose {
                    Purpose::CopyDirect => {
                        "Arraste para copiar a região • Esc ou botão direito cancela"
                    }
                    Purpose::Edit => "Arraste para editar a região • Esc ou botão direito cancela",
                    Purpose::Ocr => {
                        "Arraste para reconhecer o texto • Esc ou botão direito cancela"
                    }
                };
                let pos = Pos2::new(full.center().x, full.min.y + 32.0);
                badge(painter, full, pos, hint);
            }
        });
}

/// Reexibe o trecho selecionado sem véu, com borda de 1 px físico (RF-02).
/// Coordenadas em px da imagem do monitor; retorna o retângulo em pontos.
#[allow(clippy::too_many_arguments)]
fn draw_selection(
    painter: &egui::Painter,
    texture: egui::TextureId,
    full: Rect,
    ppp: f32,
    img_w: f32,
    img_h: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
) -> Rect {
    let sel_pts = Rect::from_min_max(
        Pos2::new(full.min.x + x0 / ppp, full.min.y + y0 / ppp),
        Pos2::new(full.min.x + x1 / ppp, full.min.y + y1 / ppp),
    );

    let uv = Rect::from_min_max(
        Pos2::new(x0 / img_w, y0 / img_h),
        Pos2::new(x1 / img_w, y1 / img_h),
    );
    painter.image(texture, sel_pts, uv, Color32::WHITE);

    painter.rect_stroke(
        sel_pts,
        CornerRadius::ZERO,
        Stroke::new(1.0 / ppp, Color32::WHITE),
        StrokeKind::Outside,
    );
    sel_pts
}

/// Confere posição/tamanho físicos e corrige via comandos de viewport.
fn ensure_geometry(ctx: &egui::Context, session: &SelectSession, idx: usize, ppp: f32) {
    let shot = &session.monitors[idx].shot;
    let target_pos = (shot.x as f32, shot.y as f32);
    let target_size = (shot.width as f32, shot.height as f32);

    let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));

    let pos_ok = outer.is_some_and(|r| {
        (r.min.x * ppp - target_pos.0).abs() <= 1.5 && (r.min.y * ppp - target_pos.1).abs() <= 1.5
    });
    if !pos_ok {
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
            target_pos.0 / ppp,
            target_pos.1 / ppp,
        )));
        ctx.request_repaint();
    }

    let size_ok = inner.is_some_and(|r| {
        (r.width() * ppp - target_size.0).abs() <= 1.5
            && (r.height() * ppp - target_size.1).abs() <= 1.5
    });
    if !size_ok {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
            target_size.0 / ppp,
            target_size.1 / ppp,
        )));
        ctx.request_repaint();
    }
}

fn ordered(a: f32, b: f32) -> (f32, f32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Etiqueta com fundo escuro, contida no retângulo `full`.
fn badge(painter: &egui::Painter, full: Rect, wanted_pos: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        FontId::proportional(13.0),
        Color32::WHITE,
    );
    let padding = Vec2::new(8.0, 5.0);
    let size = galley.size() + padding * 2.0;
    let mut rect = Rect::from_min_size(wanted_pos, size);
    // Mantém dentro da tela.
    let dx = (full.max.x - rect.max.x).min(0.0) + (full.min.x - rect.min.x).max(0.0);
    let dy = (full.max.y - rect.max.y).min(0.0) + (full.min.y - rect.min.y).max(0.0);
    rect = rect.translate(Vec2::new(dx, dy));

    painter.rect_filled(rect, 5.0, Color32::from_black_alpha(200));
    painter.galley(rect.min + padding, galley, Color32::WHITE);
}
