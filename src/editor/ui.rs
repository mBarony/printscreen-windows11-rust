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

use super::icons::{self, Icon};
use super::shapes::{arrow_geometry, normalize, shape_from_drag, Layer, Point, Shape, Tool};
use super::{
    DragPreview, EditorSession, MoveDrag, TextInput, CROP_MIN_SIDE, FONT_MAX, FONT_MIN,
    HIT_TOLERANCE_PTS, PALETTE, STROKE_MAX, STROKE_MIN, ZOOM_MAX, ZOOM_MIN,
};

/// Lado do botão de ícone da toolbar, em pontos.
const ICON_BUTTON: f32 = 26.0;
/// Lado da amostra de cor da paleta, em pontos.
const SWATCH: f32 = 20.0;

/// Dica de hover de uma ferramenta da toolbar: a tecla configurada (issue
/// #1/#4) e, para Mover/Recortar, o que a ferramenta faz.
fn tool_hint(tool: Tool, key: Option<Key>) -> String {
    let key_name = match key {
        Some(key) => key.name().to_string(),
        None => "sem atalho".to_string(),
    };
    match tool {
        Tool::Select => format!("{key_name} — arraste uma anotação para reposicioná-la"),
        Tool::Crop => format!("{key_name} — arraste a área a manter e confirme com Enter"),
        _ => key_name,
    }
}

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
        // Atalhos de letra pura não podem roubar a digitação de campos de
        // texto do egui (ex.: o hex do seletor de cores, o valor dos sliders).
        let typing = ctx.wants_keyboard_input();
        let tool_keys = session.tool_keys;
        let (undo, redo, copy, save, cancel, tool_key, confirm) = ctx.input_mut(|i| {
            // O egui-winit converte Ctrl+C em `Event::Copy` (sem emitir
            // `Event::Key`), então o Ctrl+C do editor é detectado pelo
            // evento de cópia — o `consume_key` fica como retaguarda.
            let copy_event = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
            // Letra pura de verdade: `consume_key(NONE, ..)` aceitaria
            // Shift/Alt extras (ex.: o Shift segurado para restringir uma
            // forma), então o gate exige nenhum modificador ativo.
            let tool_key = (!typing && i.modifiers.is_none())
                .then(|| {
                    tool_keys
                        .into_iter()
                        .find(|&(_, key)| {
                            key.is_some_and(|key| i.consume_key(Modifiers::NONE, key))
                        })
                        .map(|(tool, _)| tool)
                })
                .flatten();
            (
                i.consume_key(Modifiers::COMMAND, Key::Z),
                i.consume_key(Modifiers::COMMAND, Key::Y),
                copy_event || i.consume_key(Modifiers::COMMAND, Key::C),
                i.consume_key(Modifiers::COMMAND, Key::S),
                i.consume_key(Modifiers::NONE, Key::Escape),
                tool_key,
                // Enter confirma o recorte, mas não pode roubar o Enter que
                // fecha a edição de um campo numérico da toolbar.
                !typing && i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        if confirm {
            apply_crop(session);
        }
        if let Some(tool) = tool_key {
            select_tool(session, tool);
        }
        if undo {
            perform_undo(session);
        }
        if redo {
            perform_redo(session);
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
// Ferramenta ativa e undo/redo
// ---------------------------------------------------------------------------

/// Troca a ferramenta ativa (clique na toolbar ou atalho de teclado).
fn select_tool(session: &mut EditorSession, tool: Tool) {
    if session.tool == tool {
        return;
    }
    // Trocar de ferramenta no meio de um arrasto (possível pelos atalhos de
    // teclado) cancela o arrasto — um `drag` órfão viraria forma espúria.
    session.drag = None;
    cancel_move(session);
    session.tool = tool;
    session.selected = None;
    // Sair da ferramenta Recortar descarta a região ainda não confirmada.
    session.crop_pending = None;
    // Trocar de ferramenta confirma o texto pendente (ou apenas fecha a
    // caixa, se estiver vazia).
    if tool != Tool::Text {
        commit_text_input(session);
    }
}

/// Aborta um arrasto de reposicionamento em andamento, restaurando a posição
/// original da forma (o ponto de undo registrado no início sai do histórico).
fn cancel_move(session: &mut EditorSession) {
    if session.move_drag.take().is_some() {
        session.doc.abort_move();
    }
}

/// Ctrl+Z: com um arrasto de reposicionamento em andamento, desfaz só ele;
/// caso contrário desfaz a última edição. A seleção é limpa porque os
/// índices das formas podem mudar.
fn perform_undo(session: &mut EditorSession) {
    if session.move_drag.take().is_some() {
        session.doc.abort_move();
    } else {
        let before = session.doc.image_version();
        session.doc.undo();
        refit_if_image_changed(session, before);
    }
    session.selected = None;
}

fn perform_redo(session: &mut EditorSession) {
    cancel_move(session);
    let before = session.doc.image_version();
    session.doc.redo();
    refit_if_image_changed(session, before);
    session.selected = None;
}

// ---------------------------------------------------------------------------
// Recorte (issue #5)
// ---------------------------------------------------------------------------

/// Aplica a região pendente: a imagem passa a ser só ela e as anotações
/// acompanham o conteúdo. Sem região pendente, é um no-op (o Enter é inócuo
/// nas demais ferramentas).
fn apply_crop(session: &mut EditorSession) {
    let Some((min, max)) = session.crop_pending.take() else { return };
    let (img_w, img_h) = (session.doc.image().width(), session.doc.image().height());

    // A região já vem clampeada à imagem; floor/ceil preferem incluir o
    // pixel de borda a perdê-lo. `x`/`y` param um pixel antes do fim para
    // que a imagem resultante nunca tenha lado zero.
    let x = (min.x.floor().max(0.0) as u32).min(img_w.saturating_sub(1));
    let y = (min.y.floor().max(0.0) as u32).min(img_h.saturating_sub(1));
    let w = ((max.x.ceil().max(0.0) as u32).saturating_sub(x)).clamp(1, img_w - x);
    let h = ((max.y.ceil().max(0.0) as u32).saturating_sub(y)).clamp(1, img_h - y);

    session.doc.crop(x, y, w, h);
    reset_view(session);
    session.selected = None;
}

/// A imagem mudou de enquadramento: recria a textura e reajusta a vista.
///
/// O documento reconstrói a imagem a cada operação, então comparar o `Arc`
/// acusaria mudança sempre que houvesse um recorte no histórico — e desfazer
/// uma anotação jogaria fora o zoom e o pan do usuário. O selo só avança
/// quando os recortes aplicados mudam.
fn refit_if_image_changed(session: &mut EditorSession, before: u64) {
    if before != session.doc.image_version() {
        reset_view(session);
    }
}

/// Textura recriada no próximo frame; zoom volta a "ajustar à janela" e o
/// pan recentraliza a imagem nova.
fn reset_view(session: &mut EditorSession) {
    session.texture = None;
    session.zoom = None;
    session.pan = Vec2::ZERO;
}

// ---------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------

/// Botão quadrado de ícone: fundo só quando ativo ou sob o cursor, para a
/// faixa ficar leve — a identificação vem do ícone e do tooltip.
fn icon_button(ui: &mut egui::Ui, icon: Icon, selected: bool, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(ICON_BUTTON),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let hovered = enabled && response.hovered();
    let visuals = ui.visuals();
    let background = if selected {
        visuals.selection.bg_fill
    } else if hovered {
        visuals.widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let color = if !enabled {
        visuals.weak_text_color()
    } else if selected {
        visuals.selection.stroke.color
    } else if hovered {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    if background != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), background);
    }
    icons::paint(ui.painter(), rect.shrink(ICON_BUTTON * 0.26), icon, color);
    response
}

/// Amostra de cor clicável da paleta.
fn color_swatch(ui: &mut egui::Ui, rgba: [u8; 4], selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SWATCH), Sense::click());
    let painter = ui.painter();
    let fill = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
    painter.rect_filled(rect.shrink(2.0), CornerRadius::same(4), fill);
    let outline = if selected {
        Stroke::new(2.0_f32, ui.visuals().strong_text_color())
    } else {
        Stroke::new(1.0_f32, ui.visuals().widgets.inactive.bg_stroke.color)
    };
    let target = if selected { rect } else { rect.shrink(2.0) };
    painter.rect_stroke(target, CornerRadius::same(4), outline, StrokeKind::Inside);
    response
}

fn toolbar(ctx: &egui::Context, session: &mut EditorSession, target: &SaveTarget) {
    egui::TopBottomPanel::top("editor_toolbar")
        .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(8, 5)))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);

                // --- Ferramentas ---
                for (tool, key) in session.tool_keys {
                    let selected = session.tool == tool;
                    if icon_button(ui, Icon::of(tool), selected, true)
                        .on_hover_text(format!("{} — {}", tool.label(), tool_hint(tool, key)))
                        .clicked()
                    {
                        select_tool(session, tool);
                    }
                }

                // Confirmação do recorte, junto das ferramentas.
                if session.tool == Tool::Crop {
                    let ready = session.crop_pending.is_some();
                    if icon_button(ui, Icon::Check, false, ready)
                        .on_hover_text("Aplicar recorte (Enter)")
                        .clicked()
                    {
                        apply_crop(session);
                    }
                }

                separator(ui);

                // --- Cor ---
                for color in PALETTE {
                    if color_swatch(ui, color, session.color == color).clicked() {
                        session.color = color;
                    }
                }
                let mut rgb = [session.color[0], session.color[1], session.color[2]];
                if ui
                    .color_edit_button_srgb(&mut rgb)
                    .on_hover_text("Cor personalizada")
                    .changed()
                {
                    session.color = [rgb[0], rgb[1], rgb[2], 255];
                }

                separator(ui);

                // --- Espessura do traço: amostra + valor arrastável ---
                stroke_preview(ui, session.stroke_width);
                let mut stroke = session.stroke_width;
                if ui
                    .add(
                        egui::DragValue::new(&mut stroke)
                            .range(STROKE_MIN..=STROKE_MAX)
                            .speed(0.1)
                            .fixed_decimals(0),
                    )
                    .on_hover_text("Espessura do traço (Ctrl+roda no canvas)")
                    .changed()
                {
                    session.stroke_width = stroke.round().clamp(STROKE_MIN, STROKE_MAX);
                }

                // --- Tamanho da fonte (só com a ferramenta Texto) ---
                let text_tool = session.tool == Tool::Text;
                ui.add_enabled_ui(text_tool, |ui| {
                    ui.label(egui::RichText::new("A").size(15.0).strong());
                    let mut font = session.font_size;
                    if ui
                        .add(
                            egui::DragValue::new(&mut font)
                                .range(FONT_MIN..=FONT_MAX)
                                .speed(0.3)
                                .fixed_decimals(0),
                        )
                        .on_hover_text("Tamanho da fonte (Ctrl+roda no canvas)")
                        .changed()
                    {
                        session.font_size = font.round().clamp(FONT_MIN, FONT_MAX);
                    }
                });

                separator(ui);

                // --- Histórico ---
                if icon_button(ui, Icon::Undo, false, session.doc.can_undo())
                    .on_hover_text("Desfazer (Ctrl+Z)")
                    .clicked()
                {
                    perform_undo(session);
                }
                if icon_button(ui, Icon::Redo, false, session.doc.can_redo())
                    .on_hover_text("Refazer (Ctrl+Y)")
                    .clicked()
                {
                    perform_redo(session);
                }

                separator(ui);

                // --- Saída ---
                if icon_button(ui, Icon::Copy, false, true)
                    .on_hover_text("Copiar e fechar (Ctrl+C)")
                    .clicked()
                {
                    copy_and_close(session);
                }
                if icon_button(ui, Icon::Save, false, true)
                    .on_hover_text("Salvar e fechar (Ctrl+S)")
                    .clicked()
                {
                    save_and_close(session, target);
                }
                if icon_button(ui, Icon::Close, false, true)
                    .on_hover_text("Cancelar (Esc)")
                    .clicked()
                {
                    request_close(session);
                }
            });
        });
}

/// Separador vertical discreto entre grupos da toolbar.
fn separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, ICON_BUTTON * 0.6), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::ZERO,
        ui.visuals().widgets.noninteractive.bg_stroke.color,
    );
    ui.add_space(4.0);
}

/// Amostra da espessura atual — o valor numérico ao lado dá a precisão.
fn stroke_preview(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, ICON_BUTTON), Sense::hover());
    let thickness = (width * 0.7).clamp(1.0, 7.0);
    ui.painter().line_segment(
        [
            Pos2::new(rect.left() + 1.0, rect.center().y),
            Pos2::new(rect.right() - 1.0, rect.center().y),
        ],
        Stroke::new(thickness, ui.visuals().text_color()),
    );
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

            let texture = {
                let doc = &session.doc;
                session.texture.get_or_insert_with(|| {
                    let img = doc.image();
                    let color = ColorImage::from_rgba_unmultiplied(
                        [img.width() as usize, img.height() as usize],
                        img.as_raw(),
                    );
                    ui.ctx()
                        .load_texture("editor_capture", color, TextureOptions::LINEAR)
                })
            };
            let tex_id = texture.id();

            let img_w = session.doc.image().width() as f32;
            let img_h = session.doc.image().height() as f32;

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

            // --- Rolagem: uma das funções é o zoom centrado no cursor
            // (25%–400%) e a outra o ajuste da espessura do traço — ou da
            // fonte, com a ferramenta Texto ativa (issue #1). Qual delas fica
            // no Ctrl+roda e qual na roda pura é configurável (issue #4).
            let (scroll_y, ctrl) = ui.input(|i| (i.raw_scroll_delta.y, i.modifiers.command));
            let wheel_adjusts_size = ctrl != session.ctrl_wheel_zoom;
            if !wheel_adjusts_size {
                session.wheel_accum = 0.0;
            }
            if scroll_y != 0.0 && response.hovered() {
                if wheel_adjusts_size {
                    // Um passo por "linha" de rolagem — um notch da roda vale
                    // `line_scroll_speed` = 40 pt no egui nativo. Assim a roda
                    // anda de 1 em 1 por notch (sem perder notches coalescidos
                    // num frame) e os deltas contínuos de touchpad precisam
                    // acumular a mesma distância.
                    const WHEEL_STEP_PTS: f32 = 40.0;
                    session.wheel_accum += scroll_y;
                    let steps = (session.wheel_accum / WHEEL_STEP_PTS).trunc();
                    if steps != 0.0 {
                        session.wheel_accum -= steps * WHEEL_STEP_PTS;
                        if session.tool == Tool::Text {
                            session.font_size =
                                (session.font_size + steps).round().clamp(FONT_MIN, FONT_MAX);
                        } else {
                            session.stroke_width = (session.stroke_width + steps)
                                .round()
                                .clamp(STROKE_MIN, STROKE_MAX);
                        }
                    }
                } else if let Some(pointer) = response.hover_pos() {
                    let new_zoom = (zoom * (scroll_y * 0.002).exp()).clamp(ZOOM_MIN, ZOOM_MAX);
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

            // --- Interação (botão primário) ---
            //
            // O arrasto é rastreado manualmente pelo estado do ponteiro: o
            // `drag_started_by` do egui só dispara após ~6 pt de movimento
            // (desambiguação clique×arrasto do `Sense::click_and_drag`), o
            // que atrasava o preview e fazia a forma nascer deslocada do
            // ponto exato do press (issue #3). `press_origin` preserva esse
            // ponto desde o primeiro frame.
            let clamp_img = |p: Point| Point::new(p.x.clamp(0.0, img_w), p.y.clamp(0.0, img_h));
            let (primary_down, primary_pressed, primary_released, press_origin, latest_pos) =
                ui.input(|i| {
                    (
                        i.pointer.primary_down(),
                        i.pointer.primary_pressed(),
                        i.pointer.primary_released(),
                        i.pointer.press_origin(),
                        i.pointer.latest_pos(),
                    )
                });

            // Arrasto órfão (o release foi engolido por um modal, a ferramenta
            // trocou no meio, etc.): sem botão pressionado nem release a
            // processar neste frame, não há arrasto legítimo — descartar
            // evita um preview fantasma que viraria forma no próximo clique.
            if !primary_down && !primary_released {
                session.drag = None;
                cancel_move(session);
            }

            if session.text_input.is_none() && !session.confirm_discard {
                match session.tool {
                    Tool::Text => {
                        if response.hovered() {
                            ctx.output_mut(|o| o.cursor_icon = CursorIcon::Text);
                        }
                        if response.clicked_by(PointerButton::Primary) {
                            if let Some(p) = response
                                .interact_pointer_pos()
                                .or_else(|| response.hover_pos())
                                .map(|p| to_screen.inverse(p))
                            {
                                session.text_input = Some(TextInput {
                                    anchor: clamp_img(p),
                                    buffer: String::new(),
                                    focus_requested: false,
                                });
                            }
                        }
                    }
                    Tool::Select => {
                        if response.hovered() {
                            let icon = if session.move_drag.is_some() {
                                CursorIcon::Grabbing
                            } else {
                                CursorIcon::Default
                            };
                            ctx.output_mut(|o| o.cursor_icon = icon);
                        }
                        if session.move_drag.is_none() && primary_pressed && response.hovered() {
                            // `press_origin` já foi limpo se o release chegou
                            // no mesmo frame (clique coalescido) — o clique
                            // ainda deve selecionar.
                            if let Some(origin) =
                                press_origin.or_else(|| response.interact_pointer_pos())
                            {
                                let p = to_screen.inverse(origin);
                                let tol = HIT_TOLERANCE_PTS / to_screen.scale;
                                let hit = session
                                    .doc
                                    .layers()
                                    .iter()
                                    .enumerate()
                                    .rev() // a mais recente (pintada por cima) vence
                                    .find(|(_, layer)| hit_test(ctx, layer, p, tol, to_screen))
                                    .map(|(index, _)| index);
                                session.selected = hit;
                                if let Some(index) = hit {
                                    session.doc.begin_move();
                                    session.move_drag =
                                        Some(MoveDrag { index, last: p, travel: 0.0 });
                                }
                            }
                        }
                        if let Some(mv) = &mut session.move_drag {
                            if let Some(pos) = latest_pos {
                                let p = to_screen.inverse(pos);
                                let (dx, dy) = (p.x - mv.last.x, p.y - mv.last.y);
                                if dx != 0.0 || dy != 0.0 {
                                    session.doc.translate(mv.index, dx, dy);
                                    mv.travel += (dx * dx + dy * dy).sqrt();
                                    mv.last = p;
                                }
                            }
                        }
                        if primary_released {
                            if let Some(mv) = session.move_drag.take() {
                                // Clique parado (sem arrasto real) só
                                // seleciona: nem o undo nem o redo mudam.
                                if mv.travel * to_screen.scale < 2.0 {
                                    session.doc.abort_move();
                                } else {
                                    session.doc.end_move();
                                }
                            }
                        }
                    }
                    // Desenho de formas e marcação da área de recorte: mesma
                    // mecânica de arrasto, destinos diferentes no release.
                    _ => {
                        if response.hovered() {
                            ctx.output_mut(|o| o.cursor_icon = CursorIcon::Crosshair);
                        }
                        let shift = ui.input(|i| i.modifiers.shift);
                        if session.drag.is_none() && primary_pressed && response.hovered() {
                            if let Some(origin) =
                                press_origin.or_else(|| response.interact_pointer_pos())
                            {
                                let p = clamp_img(to_screen.inverse(origin));
                                session.drag = Some(DragPreview { start: p, current: p, shift });
                                // Começar uma área nova descarta a anterior.
                                session.crop_pending = None;
                            }
                        }
                        if let Some(drag) = &mut session.drag {
                            if let Some(pos) = latest_pos {
                                drag.current = clamp_img(to_screen.inverse(pos));
                            }
                            drag.shift = shift;
                        }
                        if primary_released {
                            if let Some(drag) = session.drag.take() {
                                let dx = (drag.current.x - drag.start.x).abs();
                                let dy = (drag.current.y - drag.start.y).abs();
                                if session.tool == Tool::Crop {
                                    // Área pequena demais foi engano: nada a
                                    // confirmar (o recorte só sai no Enter).
                                    if dx >= CROP_MIN_SIDE && dy >= CROP_MIN_SIDE {
                                        session.crop_pending =
                                            Some(normalize(drag.start, drag.current));
                                    }
                                } else if dx >= 2.0 || dy >= 2.0 {
                                    // Ignora cliques sem arrasto real.
                                    if let Some(shape) = shape_from_drag(
                                        session.tool,
                                        drag.start,
                                        drag.current,
                                        drag.shift,
                                    ) {
                                        session.doc.push(shape, session.style());
                                    }
                                }
                            }
                        }
                    }
                }
            }

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

            // As anotações são clipadas à imagem: o que vaza para fora não
            // entra no arquivo salvo (o rasterizador clipa), então também
            // não pode aparecer no editor — WYSIWYG inclusive depois de um
            // recorte ou de arrastar uma anotação para fora.
            let shape_painter = ui.painter_at(image_rect.intersect(canvas_rect));
            for layer in session.doc.layers() {
                paint_shape(&shape_painter, layer, to_screen);
            }
            if let Some(drag) = &session.drag {
                if let Some(shape) =
                    shape_from_drag(session.tool, drag.start, drag.current, drag.shift)
                {
                    // A pré-visualização ainda não é uma anotação do
                    // documento: recebe um id provisório só para reusar o
                    // mesmo desenho da forma já criada.
                    let preview = Layer { id: 0, shape, style: session.style() };
                    paint_shape(&shape_painter, &preview, to_screen);
                }
            }

            // Contorno tracejado da anotação selecionada (ferramenta Mover) —
            // no painter sem clip, para continuar visível se a anotação foi
            // arrastada para fora da imagem.
            if session.tool == Tool::Select {
                if let Some(layer) = session.selected.and_then(|i| session.doc.layers().get(i)) {
                    let color = ui.visuals().selection.stroke.color;
                    draw_selection_outline(ctx, &painter, layer, to_screen, color);
                }
            }

            // Área do recorte: véu sobre o que será descartado (issue #5).
            if session.tool == Tool::Crop {
                let region = session
                    .drag
                    .as_ref()
                    .map(|d| normalize(d.start, d.current))
                    .or(session.crop_pending);
                if let Some((min, max)) = region {
                    let selection =
                        Rect::from_min_max(to_screen.pos(min), to_screen.pos(max));
                    draw_crop_overlay(
                        &painter,
                        image_rect,
                        selection,
                        (max.x - min.x, max.y - min.y),
                        session.crop_pending.is_some(),
                    );
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

/// Converte uma anotação para primitivas do egui — geometria idêntica à da
/// exportação (`render.rs`), mudando apenas a escala.
fn paint_shape(painter: &egui::Painter, layer: &Layer, ts: ToScreen) {
    let style = &layer.style;
    let stroke = Stroke::new(ts.len(style.stroke_width), color32(style.color));
    match &layer.shape {
        Shape::Line { a, b } => {
            painter.line_segment([ts.pos(*a), ts.pos(*b)], stroke);
        }
        Shape::Arrow { a, b } => {
            let geo = arrow_geometry(*a, *b, style.stroke_width);
            painter.line_segment([ts.pos(geo.shaft_a), ts.pos(geo.shaft_b)], stroke);
            painter.add(egui::Shape::convex_polygon(
                geo.head.iter().map(|p| ts.pos(*p)).collect(),
                color32(style.color),
                Stroke::NONE,
            ));
        }
        Shape::Rect { min, max } => {
            painter.rect_stroke(
                Rect::from_min_max(ts.pos(*min), ts.pos(*max)),
                CornerRadius::ZERO,
                stroke,
                StrokeKind::Middle,
            );
        }
        Shape::Ellipse { center, rx, ry } => {
            painter.add(egui::Shape::ellipse_stroke(
                ts.pos(*center),
                Vec2::new(ts.len(*rx), ts.len(*ry)),
                stroke,
            ));
        }
        Shape::Text { anchor, content } => {
            painter.text(
                ts.pos(*anchor),
                Align2::LEFT_TOP,
                content,
                FontId::new(
                    ts.len(style.font_size),
                    egui::FontFamily::Name(crate::theme::INTER.into()),
                ),
                color32(style.color),
            );
        }
    }
}

fn color32(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

// ---------------------------------------------------------------------------
// Seleção (ferramenta Mover)
// ---------------------------------------------------------------------------

/// Layout do texto exatamente como `paint_shape` o pinta (fonte, tamanho em
/// pontos de tela) — usado para hit-test e caixa de seleção da variante Text.
fn text_galley(
    ctx: &egui::Context,
    content: &str,
    font_size: f32,
    ts: ToScreen,
) -> std::sync::Arc<egui::Galley> {
    ctx.fonts(|f| {
        f.layout_no_wrap(
            content.to_owned(),
            FontId::new(ts.len(font_size), egui::FontFamily::Name(crate::theme::INTER.into())),
            Color32::WHITE,
        )
    })
}

/// Hit-test de uma anotação no espaço da imagem; a variante Text é medida com
/// a fonte real para a caixa coincidir com o que está na tela.
fn hit_test(ctx: &egui::Context, layer: &Layer, p: Point, tol: f32, ts: ToScreen) -> bool {
    let text_size = match &layer.shape {
        Shape::Text { content, .. } => {
            let size = text_galley(ctx, content, layer.style.font_size, ts).size();
            (size.x / ts.scale, size.y / ts.scale)
        }
        _ => (0.0, 0.0),
    };
    layer.hit_test(p, tol, text_size)
}

/// Retângulo (em pontos de tela) que envolve a anotação, traço incluído.
fn shape_screen_bbox(ctx: &egui::Context, layer: &Layer, ts: ToScreen) -> Rect {
    let half_stroke = ts.len(layer.style.stroke_width) / 2.0;
    match &layer.shape {
        Shape::Line { a, b } | Shape::Arrow { a, b } => {
            Rect::from_two_pos(ts.pos(*a), ts.pos(*b)).expand(half_stroke)
        }
        Shape::Rect { min, max } => {
            Rect::from_min_max(ts.pos(*min), ts.pos(*max)).expand(half_stroke)
        }
        Shape::Ellipse { center, rx, ry } => {
            Rect::from_center_size(ts.pos(*center), Vec2::new(ts.len(*rx), ts.len(*ry)) * 2.0)
                .expand(half_stroke)
        }
        Shape::Text { anchor, content } => {
            let size = text_galley(ctx, content, layer.style.font_size, ts).size();
            Rect::from_min_size(ts.pos(*anchor), size)
        }
    }
}

/// Moldura tracejada ao redor da anotação selecionada.
fn draw_selection_outline(
    ctx: &egui::Context,
    painter: &egui::Painter,
    layer: &Layer,
    ts: ToScreen,
    color: Color32,
) {
    let rect = shape_screen_bbox(ctx, layer, ts).expand(5.0);
    let stroke = Stroke::new(1.5_f32, color);
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    for i in 0..4 {
        painter.extend(egui::Shape::dashed_line(
            &[corners[i], corners[(i + 1) % 4]],
            stroke,
            4.0,
            3.0,
        ));
    }
}

// ---------------------------------------------------------------------------
// Recorte: véu e moldura
// ---------------------------------------------------------------------------

/// Escurece o que ficará de fora, contorna a área mantida e anota as suas
/// dimensões. `confirmed` distingue a região já solta (aguardando Enter) do
/// arrasto ainda em curso.
fn draw_crop_overlay(
    painter: &egui::Painter,
    image_rect: Rect,
    selection: Rect,
    size_px: (f32, f32),
    confirmed: bool,
) {
    let selection = selection.intersect(image_rect);
    let veil = Color32::from_black_alpha(150);
    // Faixas acima, abaixo, à esquerda e à direita da área mantida.
    let bands = [
        Rect::from_min_max(
            image_rect.min,
            Pos2::new(image_rect.max.x, selection.min.y),
        ),
        Rect::from_min_max(
            Pos2::new(image_rect.min.x, selection.max.y),
            image_rect.max,
        ),
        Rect::from_min_max(
            Pos2::new(image_rect.min.x, selection.min.y),
            Pos2::new(selection.min.x, selection.max.y),
        ),
        Rect::from_min_max(
            Pos2::new(selection.max.x, selection.min.y),
            Pos2::new(image_rect.max.x, selection.max.y),
        ),
    ];
    for band in bands {
        if band.is_positive() {
            painter.rect_filled(band, CornerRadius::ZERO, veil);
        }
    }

    painter.rect_stroke(
        selection,
        CornerRadius::ZERO,
        Stroke::new(1.5_f32, Color32::WHITE),
        StrokeKind::Middle,
    );

    let (w, h) = size_px;
    let label = if confirmed {
        format!("{} × {} — Enter aplica · Esc cancela", w.round(), h.round())
    } else {
        format!("{} × {}", w.round(), h.round())
    };
    // Acima da área quando há espaço; dentro dela quando o recorte encosta
    // no topo da imagem.
    let (anchor, pos) = if selection.min.y - image_rect.min.y > 22.0 {
        (Align2::LEFT_BOTTOM, selection.left_top() + Vec2::new(0.0, -4.0))
    } else {
        (Align2::LEFT_TOP, selection.left_top() + Vec2::new(4.0, 4.0))
    };
    painter.text(
        pos,
        anchor,
        label,
        FontId::proportional(12.0),
        Color32::WHITE,
    );
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
    let shape = Shape::Text { anchor: input.anchor, content: input.buffer.clone() };
    let style = session.style();
    session.doc.push(shape, style);
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
    let base = session.doc.image().clone();
    let layers = session.doc.layers().to_vec();
    crate::jobs::spawn(move || match super::render::render(&base, &layers) {
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
    let base = session.doc.image().clone();
    let layers = session.doc.layers().to_vec();
    let target = target.clone();
    crate::jobs::spawn(move || match super::render::render(&base, &layers) {
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
    if session.drag.take().is_some() {
        // Primeiro Esc apenas cancela o arrasto de desenho em andamento.
        return;
    }
    if session.crop_pending.take().is_some() {
        // Primeiro Esc apenas descarta a área de recorte marcada.
        return;
    }
    cancel_move(session);
    if session.selected.take().is_some() {
        // Primeiro Esc apenas desfaz a seleção.
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
