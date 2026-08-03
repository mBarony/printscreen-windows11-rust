//! Overlay de seleção de região (RF-02, §10): uma janela por monitor, sem
//! bordas, sempre no topo, fora do Alt-Tab, exibindo a captura congelada
//! daquele monitor com véu escuro (~60%).
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

/// Véu preto ~60% (RF-02).
const VEIL_ALPHA: u8 = 153;
/// Seleções menores que isso (px) são ignoradas — clique acidental.
const MIN_SELECTION_PX: f32 = 3.0;

/// O que fazer com a região confirmada.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// RF-02: recorta e salva direto.
    SaveDirect,
    /// RF-03: recorta e abre o editor.
    Edit,
}

/// Resultado terminal de uma sessão de seleção.
pub enum Outcome {
    Cancelled,
    Selected {
        monitor: usize,
        /// (x, y, largura, altura) em px da imagem do monitor.
        rect: (u32, u32, u32, u32),
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
    drag: Option<Drag>,
    pub outcome: Option<Outcome>,
}

impl SelectSession {
    pub fn new(serial: u64, shots: Vec<MonitorShot>, purpose: Purpose) -> Self {
        Self {
            serial,
            purpose,
            monitors: shots
                .into_iter()
                .map(|shot| OverlayMonitor { shot, texture: None })
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
            painter.image(
                texture.id(),
                full,
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

            let (primary_pressed, primary_down, primary_released) = ctx.input(|i| {
                (
                    i.pointer.primary_pressed(),
                    i.pointer.primary_down(),
                    i.pointer.primary_released(),
                )
            });

            if primary_pressed && session.drag.is_none() {
                if let Some(p) = pointer_px {
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
                            drag.current = clamp(p);
                        }
                    }
                    if primary_released {
                        let drag = session.drag.take().expect("drag presente");
                        let (x0, x1) = ordered(drag.start.0, drag.current.0);
                        let (y0, y1) = ordered(drag.start.1, drag.current.1);
                        if x1 - x0 >= MIN_SELECTION_PX && y1 - y0 >= MIN_SELECTION_PX {
                            let x = x0.floor().max(0.0) as u32;
                            let y = y0.floor().max(0.0) as u32;
                            let w = ((x1 - x0).round() as u32).max(1).min(img_w as u32 - x);
                            let h = ((y1 - y0).round() as u32).max(1).min(img_h as u32 - y);
                            session.outcome =
                                Some(Outcome::Selected { monitor: idx, rect: (x, y, w, h) });
                            ctx.request_repaint_of(egui::ViewportId::ROOT);
                            return;
                        }
                        // Clique sem arrasto: continua selecionando.
                    }
                }
            }

            // --- Guias em cruz sob o cursor ---
            if let Some(p) = pointer_pts {
                if session.drag.is_none() {
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
                    let sel_pts = Rect::from_min_max(
                        Pos2::new(full.min.x + x0 / ppp, full.min.y + y0 / ppp),
                        Pos2::new(full.min.x + x1 / ppp, full.min.y + y1 / ppp),
                    );

                    // Reexibe o trecho selecionado sem véu.
                    let uv = Rect::from_min_max(
                        Pos2::new(x0 / img_w, y0 / img_h),
                        Pos2::new(x1 / img_w, y1 / img_h),
                    );
                    painter.image(texture.id(), sel_pts, uv, Color32::WHITE);

                    // Borda de 1 px físico (RF-02).
                    painter.rect_stroke(
                        sel_pts,
                        CornerRadius::ZERO,
                        Stroke::new(1.0 / ppp, Color32::WHITE),
                        StrokeKind::Outside,
                    );

                    // Badge "L × A px" junto ao cursor.
                    let w_px = (x1 - x0).round() as u32;
                    let h_px = (y1 - y0).round() as u32;
                    let text = format!("{w_px} × {h_px} px");
                    let anchor = pointer_pts.unwrap_or(sel_pts.max) + Vec2::new(16.0, 18.0);
                    badge(painter, full, anchor, &text);
                }
            }

            // Dica de uso enquanto nada foi arrastado.
            if session.drag.is_none() {
                let hint = "Arraste para selecionar • Esc ou botão direito cancela";
                let pos = Pos2::new(full.center().x, full.min.y + 32.0);
                badge(painter, full, pos, hint);
            }
        });
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
