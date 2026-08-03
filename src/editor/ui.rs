//! Janela do editor: toolbar, canvas com zoom/pan, interações de desenho e
//! atalhos (RF-04, §8).
//!
//! Convenção de espaços: as formas vivem em px da imagem; `zoom` é px físicos
//! de tela por px de imagem; o canvas desenha em pontos do egui
//! (`pontos = px_físicos / pixels_per_point`). Em zoom 100%, 1 px da imagem
//! ocupa exatamente 1 px físico do monitor.

use egui::{
    Align2, Color32, ColorImage, CornerRadius, CursorIcon, FontId, Key, Modifiers, PointerButton,
    Pos2, Rect, Sense, Stroke, StrokeKind, TextureOptions, Vec2,
};

use crate::clipboard;
use crate::notify;
use crate::storage::{self, SaveTarget};

use super::shapes::{arrow_geometry, shape_from_drag, Point, Shape, Tool};
use super::{
    DragPreview, EditorSession, TextInput, FONT_MAX, FONT_MIN, PALETTE, STROKE_MAX, STROKE_MIN,
    ZOOM_MAX, ZOOM_MIN,
};

/// Transformação imagem → tela (pontos do egui).
#[derive(Clone, Copy)]
struct ToScreen {
    origin: Pos2,
    /// Pontos por px da imagem.
    scale: f32,
}

impl ToScreen {
    fn pos(&self, p: Point) -> Pos2 {
        Pos2::new(self.origin.x + p.x * self.scale, self.origin.y + p.y * self.scale)
    }

    fn len(&self, l: f32) -> f32 {
        l * self.scale
    }

    fn inverse(&self, pos: Pos2) -> Point {
        Point::new((pos.x - self.origin.x) / self.scale, (pos.y - self.origin.y) / self.scale)
    }
}

/// Corpo da janela do editor. Chamado dentro do viewport dedicado; `target`
/// é o snapshot de configuração usado por Ctrl+S/Ctrl+C.
pub fn show(ctx: &egui::Context, session: &mut EditorSession, target: &SaveTarget) {
    claim_focus(ctx, session);

    // Atalhos globais da janela (ficam inativos com a caixa de texto aberta,
    // para não roubar o Ctrl+C/Esc da digitação).
    if session.text_input.is_none() && !session.confirm_discard {
        let (undo, redo, copy, save, cancel) = ctx.input_mut(|i| {
            // O egui-winit converte Ctrl+C em `Event::Copy` (sem emitir
            // `Event::Key`), então o Ctrl+C do editor é detectado pelo
            // evento de cópia — o `consume_key` fica como retaguarda.
            let copy_event = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
            (
                i.consume_key(Modifiers::COMMAND, Key::Z),
                i.consume_key(Modifiers::COMMAND, Key::Y),
                copy_event || i.consume_key(Modifiers::COMMAND, Key::C),
                i.consume_key(Modifiers::COMMAND, Key::S),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if undo {
            session.stack.undo();
        }
        if redo {
            session.stack.redo();
        }
        if copy {
            copy_and_close(session);
        }
        if save {
            save_and_close(session, target);
        }
        if cancel {
            request_close(session);
        }
    }

    // Botão X da janela → mesmo fluxo do Esc (com confirmação se preciso).
    if ctx.input(|i| i.viewport().close_requested()) && !session.finished {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        request_close(session);
    }

    toolbar(ctx, session, target);
    canvas(ctx, session);

    if session.confirm_discard {
        confirm_discard_modal(ctx, session);
    }
}

/// Garante que a janela nasça com o foco do teclado, para que `Ctrl+C` e
/// `Ctrl+S` funcionem sem um clique antes.
///
/// O editor abre no instante em que o overlay de seleção fecha — momento em
/// que o Windows já devolveu o primeiro plano ao app que estava atrás. Nesse
/// estado o `SetForegroundWindow` do winit é recusado pelo foreground lock,
/// então a janela aparece visível porém sem foco. A insistência dura poucos
/// frames e para assim que o foco chega (ou se o usuário sair da janela por
/// conta própria — nunca roubamos o foco de volta depois disso).
fn claim_focus(ctx: &egui::Context, session: &mut EditorSession) {
    if session.focus_frames == 0 {
        return;
    }
    if ctx.input(|i| i.viewport().focused).unwrap_or(false) {
        session.focus_frames = 0;
        return;
    }

    session.focus_frames -= 1;
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    crate::platform::shell::focus_window(super::WINDOW_TITLE);
    ctx.request_repaint();
}

// ---------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------

fn toolbar(ctx: &egui::Context, session: &mut EditorSession, target: &SaveTarget) {
    egui::TopBottomPanel::top("editor_toolbar").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for tool in [Tool::Line, Tool::Arrow, Tool::Rect, Tool::Ellipse, Tool::Text] {
                let selected = session.tool == tool;
                if ui.selectable_label(selected, tool.label()).clicked() {
                    session.tool = tool;
                    // Trocar de ferramenta confirma o texto pendente (ou
                    // apenas fecha a caixa, se estiver vazia).
                    if tool != Tool::Text {
                        commit_text_input(session);
                    }
                }
            }

            ui.separator();

            // Paleta de 8 cores + seletor livre.
            for color in PALETTE {
                let c32 = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                let selected = session.color == color;
                let size = Vec2::splat(18.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                let painter = ui.painter();
                painter.rect_filled(rect.shrink(2.0), 3.0, c32);
                if selected {
                    painter.rect_stroke(
                        rect,
                        4.0,
                        Stroke::new(2.0_f32, ui.visuals().strong_text_color()),
                        StrokeKind::Inside,
                    );
                } else {
                    painter.rect_stroke(
                        rect.shrink(2.0),
                        3.0,
                        Stroke::new(1.0_f32, Color32::from_gray(90)),
                        StrokeKind::Inside,
                    );
                }
                if resp.clicked() {
                    session.color = color;
                }
            }
            let mut rgb = [session.color[0], session.color[1], session.color[2]];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                session.color = [rgb[0], rgb[1], rgb[2], 255];
            }
        });
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Traço");
            let mut stroke = session.stroke_width;
            if ui
                .add(egui::Slider::new(&mut stroke, STROKE_MIN..=STROKE_MAX).step_by(1.0))
                .changed()
            {
                session.stroke_width = stroke.round().clamp(STROKE_MIN, STROKE_MAX);
            }

            ui.label("Fonte");
            let mut font = session.font_size;
            let font_slider = egui::Slider::new(&mut font, FONT_MIN..=FONT_MAX).step_by(1.0);
            if ui
                .add_enabled(session.tool == Tool::Text, font_slider)
                .changed()
            {
                session.font_size = font.round().clamp(FONT_MIN, FONT_MAX);
            }

            ui.separator();

            if ui
                .add_enabled(session.stack.can_undo(), egui::Button::new("Desfazer"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                session.stack.undo();
            }
            if ui
                .add_enabled(session.stack.can_redo(), egui::Button::new("Refazer"))
                .on_hover_text("Ctrl+Y")
                .clicked()
            {
                session.stack.redo();
            }

            ui.separator();

            if ui
                .button("Copiar")
                .on_hover_text("Ctrl+C — copia e fecha")
                .clicked()
            {
                copy_and_close(session);
            }
            if ui
                .button("Salvar")
                .on_hover_text("Ctrl+S — salva e fecha")
                .clicked()
            {
                save_and_close(session, target);
            }
            if ui.button("Cancelar").on_hover_text("Esc").clicked() {
                request_close(session);
            }
        });
        ui.add_space(4.0);
    });
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

fn canvas(ctx: &egui::Context, session: &mut EditorSession) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(Color32::from_gray(28)))
        .show(ctx, |ui| {
            let ppp = ui.ctx().pixels_per_point();
            let canvas_rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(canvas_rect, Sense::click_and_drag());

            let texture = session.texture.get_or_insert_with(|| {
                let img = &session.image;
                let color = ColorImage::from_rgba_unmultiplied(
                    [img.width() as usize, img.height() as usize],
                    img.as_raw(),
                );
                ui.ctx()
                    .load_texture("editor_capture", color, TextureOptions::LINEAR)
            });
            let tex_id = texture.id();

            let img_w = session.image.width() as f32;
            let img_h = session.image.height() as f32;

            // Primeiro frame: "ajustar à janela" (nunca acima de 100%).
            let zoom = *session.zoom.get_or_insert_with(|| {
                let avail_px = canvas_rect.size() * ppp;
                let fit = (avail_px.x / img_w).min(avail_px.y / img_h).min(1.0);
                fit.clamp(ZOOM_MIN, ZOOM_MAX)
            });
            if session.pan == Vec2::ZERO && session.drag.is_none() {
                // Centraliza enquanto o usuário ainda não interagiu.
                let img_pts = Vec2::new(img_w, img_h) * (zoom / ppp);
                let free = canvas_rect.size() - img_pts;
                session.pan = Vec2::new(free.x.max(0.0) / 2.0, free.y.max(0.0) / 2.0);
            }

            let to_screen = ToScreen {
                origin: canvas_rect.min + session.pan,
                scale: zoom / ppp,
            };

            // --- Zoom com Ctrl+scroll, centrado no cursor (25%–400%) ---
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if zoom_delta != 1.0 {
                if let Some(pointer) = response.hover_pos() {
                    let new_zoom = (zoom * zoom_delta).clamp(ZOOM_MIN, ZOOM_MAX);
                    if (new_zoom - zoom).abs() > f32::EPSILON {
                        // Mantém o ponto da imagem sob o cursor fixo na tela.
                        let img_pt = to_screen.inverse(pointer);
                        let new_scale = new_zoom / ppp;
                        session.pan = (pointer - canvas_rect.min)
                            - Vec2::new(img_pt.x * new_scale, img_pt.y * new_scale);
                        session.zoom = Some(new_zoom);
                    }
                }
            }

            // --- Pan com o botão do meio ---
            if response.dragged_by(PointerButton::Middle) {
                session.pan += response.drag_delta();
            }

            // Recalcula a transformação (zoom/pan podem ter mudado).
            let zoom = session.zoom.unwrap_or(zoom);
            let to_screen = ToScreen {
                origin: canvas_rect.min + session.pan,
                scale: zoom / ppp,
            };

            // --- Desenho: imagem + formas confirmadas + preview ---
            let painter = ui.painter_at(canvas_rect);
            let image_rect = Rect::from_min_size(
                to_screen.pos(Point::new(0.0, 0.0)),
                Vec2::new(img_w, img_h) * to_screen.scale,
            );
            painter.image(
                tex_id,
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );

            for shape in session.stack.shapes() {
                paint_shape(&painter, shape, to_screen);
            }
            if let Some(drag) = &session.drag {
                if let Some(preview) = shape_from_drag(
                    session.tool,
                    drag.start,
                    drag.current,
                    drag.shift,
                    session.style(),
                ) {
                    paint_shape(&painter, &preview, to_screen);
                }
            }

            // --- Interação de desenho (botão primário) ---
            let pointer_img = response
                .hover_pos()
                .or_else(|| response.interact_pointer_pos())
                .map(|p| to_screen.inverse(p));
            let clamp_img = |p: Point| Point::new(p.x.clamp(0.0, img_w), p.y.clamp(0.0, img_h));

            if session.text_input.is_none() && !session.confirm_discard {
                match session.tool {
                    Tool::Text => {
                        if response.hovered() {
                            ctx.output_mut(|o| o.cursor_icon = CursorIcon::Text);
                        }
                        if response.clicked_by(PointerButton::Primary) {
                            if let Some(p) = pointer_img {
                                session.text_input = Some(TextInput {
                                    anchor: clamp_img(p),
                                    buffer: String::new(),
                                    focus_requested: false,
                                });
                            }
                        }
                    }
                    _ => {
                        if response.hovered() {
                            ctx.output_mut(|o| o.cursor_icon = CursorIcon::Crosshair);
                        }
                        let shift = ui.input(|i| i.modifiers.shift);
                        if response.drag_started_by(PointerButton::Primary) {
                            if let Some(p) = pointer_img {
                                let p = clamp_img(p);
                                session.drag =
                                    Some(DragPreview { start: p, current: p, shift });
                            }
                        }
                        if response.dragged_by(PointerButton::Primary) {
                            if let (Some(drag), Some(p)) = (&mut session.drag, pointer_img) {
                                drag.current = clamp_img(p);
                                drag.shift = shift;
                            }
                        }
                        if response.drag_stopped_by(PointerButton::Primary) {
                            if let Some(drag) = session.drag.take() {
                                let dx = (drag.current.x - drag.start.x).abs();
                                let dy = (drag.current.y - drag.start.y).abs();
                                // Ignora cliques sem arrasto real.
                                if dx >= 2.0 || dy >= 2.0 {
                                    if let Some(shape) = shape_from_drag(
                                        session.tool,
                                        drag.start,
                                        drag.current,
                                        drag.shift,
                                        session.style(),
                                    ) {
                                        session.stack.push(shape);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // --- Caixa de texto inline (ferramenta Texto) ---
            if session.text_input.is_some() {
                text_input_overlay(ctx, session, to_screen);
            }

            // Rodapé discreto com o zoom atual.
            let label = format!("{:.0}%", zoom * 100.0);
            painter.text(
                canvas_rect.right_bottom() - Vec2::new(8.0, 6.0),
                Align2::RIGHT_BOTTOM,
                label,
                FontId::proportional(12.0),
                Color32::from_gray(140),
            );
        });
}

/// Converte uma forma para primitivas do egui — geometria idêntica à da
/// exportação (`render.rs`), mudando apenas a escala.
fn paint_shape(painter: &egui::Painter, shape: &Shape, ts: ToScreen) {
    match shape {
        Shape::Line { a, b, style } => {
            painter.line_segment(
                [ts.pos(*a), ts.pos(*b)],
                Stroke::new(ts.len(style.stroke_width), color32(style.color)),
            );
        }
        Shape::Arrow { a, b, style } => {
            let geo = arrow_geometry(*a, *b, style.stroke_width);
            painter.line_segment(
                [ts.pos(geo.shaft_a), ts.pos(geo.shaft_b)],
                Stroke::new(ts.len(style.stroke_width), color32(style.color)),
            );
            painter.add(egui::Shape::convex_polygon(
                geo.head.iter().map(|p| ts.pos(*p)).collect(),
                color32(style.color),
                Stroke::NONE,
            ));
        }
        Shape::Rect { min, max, style } => {
            painter.rect_stroke(
                Rect::from_min_max(ts.pos(*min), ts.pos(*max)),
                CornerRadius::ZERO,
                Stroke::new(ts.len(style.stroke_width), color32(style.color)),
                StrokeKind::Middle,
            );
        }
        Shape::Ellipse { center, rx, ry, style } => {
            painter.add(egui::Shape::ellipse_stroke(
                ts.pos(*center),
                Vec2::new(ts.len(*rx), ts.len(*ry)),
                Stroke::new(ts.len(style.stroke_width), color32(style.color)),
            ));
        }
        Shape::Text { anchor, content, style } => {
            painter.text(
                ts.pos(*anchor),
                Align2::LEFT_TOP,
                content,
                FontId::new(ts.len(style.font_size), egui::FontFamily::Name(crate::theme::INTER.into())),
                color32(style.color),
            );
        }
    }
}

fn color32(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

// ---------------------------------------------------------------------------
// Texto inline
// ---------------------------------------------------------------------------

fn text_input_overlay(ctx: &egui::Context, session: &mut EditorSession, ts: ToScreen) {
    let Some(input) = &mut session.text_input else { return };
    let pos = ts.pos(input.anchor);
    let font_pts = ts.len(session.font_size).max(8.0);
    let color = color32(session.color);

    let mut commit = false;
    let mut cancel = false;

    egui::Area::new(egui::Id::new(("editor_text_input", session.serial)))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let edit = egui::TextEdit::singleline(&mut input.buffer)
                .font(FontId::new(font_pts, egui::FontFamily::Name(crate::theme::INTER.into())))
                .text_color(color)
                .hint_text("Texto…")
                .desired_width(280.0_f32.max(font_pts * 6.0));
            let response = ui.add(edit);
            if !input.focus_requested {
                response.request_focus();
                input.focus_requested = true;
            }
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            if esc {
                cancel = true;
            } else if enter || (response.lost_focus() && !response.has_focus()) {
                commit = true;
            }
        });

    if cancel {
        session.text_input = None;
    } else if commit {
        commit_text_input(session);
    }
}

/// Confirma o texto pendente como forma (caixa vazia é apenas fechada).
fn commit_text_input(session: &mut EditorSession) {
    let Some(input) = &session.text_input else { return };
    if input.buffer.trim().is_empty() {
        session.text_input = None;
        return;
    }
    let shape = Shape::Text {
        anchor: input.anchor,
        content: input.buffer.clone(),
        style: session.style(),
    };
    session.stack.push(shape);
    session.text_input = None;
}

// ---------------------------------------------------------------------------
// Ações: copiar, salvar, fechar
// ---------------------------------------------------------------------------

/// Ctrl+C: renderiza, copia para a área de transferência e fecha o editor
/// (v1.2 — antes o editor permanecia aberto). A renderização e a cópia
/// acontecem em thread de trabalho; a janela fecha imediatamente e o toast
/// confirma (ou reporta a falha) na sequência.
fn copy_and_close(session: &mut EditorSession) {
    commit_text_input(session);
    let base = session.image.clone();
    let shapes = session.stack.shapes().to_vec();
    std::thread::spawn(move || match super::render::render(&base, &shapes) {
        Ok(final_image) => match clipboard::copy_image(&final_image) {
            Ok(()) => notify::toast(
                "Copiado para a área de transferência",
                "A imagem anotada está pronta para colar.",
            ),
            Err(err) => notify::toast_error("Falha ao copiar", &format!("{err:#}")),
        },
        Err(err) => notify::toast_error("Falha ao renderizar anotações", &format!("{err:#}")),
    });
    session.finished = true;
}

/// Ctrl+S: renderiza, salva na pasta configurada e fecha o editor (RF-04).
fn save_and_close(session: &mut EditorSession, target: &SaveTarget) {
    commit_text_input(session);
    let base = session.image.clone();
    let shapes = session.stack.shapes().to_vec();
    let target = target.clone();
    std::thread::spawn(move || match super::render::render(&base, &shapes) {
        Ok(final_image) => match storage::write_image(&target, &final_image) {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                log::info!("captura anotada salva em {}", path.display());
                notify::toast("Captura salva", &name);
            }
            Err(err) => notify::toast_error("Falha ao salvar captura", &format!("{err:#}")),
        },
        Err(err) => notify::toast_error("Falha ao renderizar anotações", &format!("{err:#}")),
    });
    session.finished = true;
}

/// Esc / X / Cancelar: fecha, confirmando se houver anotações (RF-04).
fn request_close(session: &mut EditorSession) {
    if session.text_input.is_some() {
        // Primeiro Esc fecha apenas a caixa de texto.
        session.text_input = None;
        return;
    }
    if session.dirty() {
        session.confirm_discard = true;
    } else {
        session.finished = true;
    }
}

fn confirm_discard_modal(ctx: &egui::Context, session: &mut EditorSession) {
    let modal = egui::Modal::new(egui::Id::new(("editor_confirm_discard", session.serial)))
        .show(ctx, |ui| {
            ui.set_max_width(340.0);
            ui.heading("Descartar anotações?");
            ui.add_space(6.0);
            ui.label("A captura tem anotações não salvas. Fechar sem salvar?");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Descartar").clicked() {
                    session.confirm_discard = false;
                    session.finished = true;
                }
                if ui.button("Continuar editando").clicked() {
                    session.confirm_discard = false;
                }
            });
        });
    if modal.should_close() {
        session.confirm_discard = false;
    }
}
