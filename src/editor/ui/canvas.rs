//! Canvas do editor: zoom, pan, criação de formas e composição do quadro.

use egui::{
    Align2, Color32, ColorImage, CornerRadius, CursorIcon, FontId, PointerButton,
    Pos2, Rect, Sense, Stroke, StrokeKind, TextureOptions, Vec2,
};


use crate::editor::redact;
use crate::editor::shapes::{
    normalize, push_sample, shape_from_drag, stroke_from_samples, Layer, Point, Shape, Tool,
    REDACTION_MIN_SIDE,
};
use crate::editor::{
    DragPreview, EditorSession, TextInput, CROP_MIN_SIDE,
    FONT_MAX, FONT_MIN, STROKE_MAX,
    STROKE_MIN, ZOOM_MAX, ZOOM_MIN,
};
use super::{cancel_move, ToScreen};
use super::interact::{
    begin_select_drag, handle_at, handle_cursor, pick_color,
    restyle_selection, selected_is_text,
};
use super::paint::{
    draw_crop_overlay, draw_handles, draw_selection_outline, paint_shape,
};
use super::text::text_input_overlay;

// ---------------------------------------------------------------------------

pub(super) fn draw(ctx: &egui::Context, session: &mut EditorSession) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(Color32::from_gray(28)))
        .show(ctx, |ui| {
            let ppp = ui.ctx().pixels_per_point();
            let canvas_rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(canvas_rect, Sense::click_and_drag());

            // A textura carrega a imagem já redigida, então precisa ser
            // refeita sempre que os pixels visíveis mudam — não só quando o
            // enquadramento muda.
            if session.texture_version != session.doc.pixels_version() {
                session.texture = None;
                session.texture_version = session.doc.pixels_version();
            }
            let texture = {
                let doc = &session.doc;
                session.texture.get_or_insert_with(|| {
                    let img = doc.visible_image();
                    let color = ColorImage::from_rgba_unmultiplied(
                        [img.width() as usize, img.height() as usize],
                        img.as_raw(),
                    );
                    ui.ctx()
                        .load_texture("editor_capture", color, TextureOptions::LINEAR)
                })
            };
            let tex_id = texture.id();

            let img_w = session.doc.visible_image().width() as f32;
            let img_h = session.doc.visible_image().height() as f32;

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
                        // Com uma anotação de texto selecionada, a roda mexe
                        // no tamanho dela — é o único jeito de redimensionar
                        // um texto, que não tem alça.
                        let sizing_text = session.tool == Tool::Text || selected_is_text(session);
                        if sizing_text {
                            session.font_size =
                                (session.font_size + steps).round().clamp(FONT_MIN, FONT_MAX);
                        } else {
                            session.stroke_width = (session.stroke_width + steps)
                                .round()
                                .clamp(STROKE_MIN, STROKE_MAX);
                        }
                        restyle_selection(ctx, session);
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
                    // Amostra a cor sob o cursor e devolve a ferramenta.
                    Tool::Eyedropper => {
                        if response.hovered() {
                            ctx.output_mut(|o| o.cursor_icon = CursorIcon::Crosshair);
                        }
                        if response.clicked_by(PointerButton::Primary) {
                            if let Some(p) = response
                                .interact_pointer_pos()
                                .or_else(|| response.hover_pos())
                                .map(|p| clamp_img(to_screen.inverse(p)))
                            {
                                pick_color(ctx, session, p);
                            }
                        }
                    }
                    // O contador é carimbado num clique, sem arrasto.
                    Tool::Marker => {
                        if response.hovered() {
                            ctx.output_mut(|o| o.cursor_icon = CursorIcon::Crosshair);
                        }
                        if response.clicked_by(PointerButton::Primary) {
                            if let Some(p) = response
                                .interact_pointer_pos()
                                .or_else(|| response.hover_pos())
                                .map(|p| clamp_img(to_screen.inverse(p)))
                            {
                                let shape = Shape::Marker {
                                    center: p,
                                    number: session.doc.next_marker(),
                                };
                                session.doc.push(shape, session.style());
                            }
                        }
                    }
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
                        let hover_handle = (session.move_drag.is_none()
                            && session.resize_drag.is_none())
                        .then(|| latest_pos.and_then(|pos| handle_at(session, to_screen, pos)))
                        .flatten();
                        if response.hovered() {
                            let icon = match (&session.resize_drag, &session.move_drag) {
                                (Some(rz), _) => handle_cursor(rz.handle),
                                (None, Some(_)) => CursorIcon::Grabbing,
                                _ => hover_handle
                                    .map(|(_, h)| handle_cursor(h))
                                    .unwrap_or(CursorIcon::Default),
                            };
                            ctx.output_mut(|o| o.cursor_icon = icon);
                        }
                        if session.move_drag.is_none()
                            && session.resize_drag.is_none()
                            && primary_pressed
                            && response.hovered()
                        {
                            // `press_origin` já foi limpo se o release chegou
                            // no mesmo frame (clique coalescido) — o clique
                            // ainda deve selecionar.
                            if let Some(origin) =
                                press_origin.or_else(|| response.interact_pointer_pos())
                            {
                                begin_select_drag(ctx, session, to_screen, origin);
                            }
                        }
                        if let Some(rz) = &session.resize_drag {
                            if let Some(pos) = latest_pos {
                                let constrain = ui.input(|i| i.modifiers.shift);
                                let p = to_screen.inverse(pos);
                                session.doc.resize(rz.index, rz.handle, p, constrain);
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
                            // Um arrasto de alça que não mudou nada não vira
                            // histórico: `end_move` só registra o que mudou.
                            if session.resize_drag.take().is_some() {
                                session.doc.end_move();
                            }
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
                        // Relidos a cada quadro: dá para ligar e desligar a
                        // restrição no meio do arrasto, sem recomeçar.
                        let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
                        if session.drag.is_none() && primary_pressed && response.hovered() {
                            if let Some(origin) =
                                press_origin.or_else(|| response.interact_pointer_pos())
                            {
                                let p = clamp_img(to_screen.inverse(origin));
                                session.drag = Some(DragPreview {
                                    start: p,
                                    current: p,
                                    shift,
                                    alt,
                                    samples: vec![p],
                                });
                                // Começar uma área nova descarta a anterior.
                                session.crop_pending = None;
                            }
                        }
                        if let Some(drag) = &mut session.drag {
                            if let Some(pos) = latest_pos {
                                drag.current = clamp_img(to_screen.inverse(pos));
                                if session.tool.is_stroke() {
                                    push_sample(&mut drag.samples, drag.current);
                                }
                            }
                            drag.shift = shift;
                            drag.alt = alt;
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
                                } else if session.tool.is_stroke() {
                                    // Um rabisco é medido pelo caminho
                                    // percorrido, não pela distância entre as
                                    // pontas: um círculo começa e termina no
                                    // mesmo lugar e nem por isso é um clique.
                                    if let Some(shape) =
                                        stroke_from_samples(session.tool, &drag.samples)
                                    {
                                        session.doc.push(shape, session.style());
                                    }
                                } else if dx >= 2.0 || dy >= 2.0 {
                                    // Ignora cliques sem arrasto real.
                                    if let Some(mut shape) = shape_from_drag(
                                        session.tool,
                                        drag.start,
                                        drag.current,
                                        drag.shift,
                                        drag.alt,
                                    ) {
                                        if let Shape::Redaction { min, max, seed } = &mut shape {
                                            // Uma redação minúscula não
                                            // esconderia nada; e cada uma leva
                                            // a própria semente.
                                            max.x = max.x.max(min.x + REDACTION_MIN_SIDE);
                                            max.y = max.y.max(min.y + REDACTION_MIN_SIDE);
                                            *seed = redact::fresh_seed();
                                        }
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
                let in_progress = if session.tool.is_stroke() {
                    stroke_from_samples(session.tool, &drag.samples)
                } else {
                    shape_from_drag(session.tool, drag.start, drag.current, drag.shift, drag.alt)
                };
                if let Some(shape) = in_progress {
                    let burnt_area = match &shape {
                        Shape::Redaction { min, max, .. } => Some((*min, *max)),
                        Shape::Spotlight { center, rx, ry } => Some((
                            Point::new(center.x - rx, center.y - ry),
                            Point::new(center.x + rx, center.y + ry),
                        )),
                        _ => None,
                    };
                    if let Some((min, max)) = burnt_area {
                        // Redação e holofote só existem depois de queimados
                        // na imagem; durante o arrasto, o que se mostra é a
                        // área que eles vão ocupar.
                        let area = Rect::from_min_max(to_screen.pos(min), to_screen.pos(max));
                        shape_painter.rect_filled(
                            area,
                            CornerRadius::ZERO,
                            Color32::from_black_alpha(170),
                        );
                        shape_painter.rect_stroke(
                            area,
                            CornerRadius::ZERO,
                            Stroke::new(1.5_f32, Color32::WHITE),
                            StrokeKind::Middle,
                        );
                    } else {
                        // A pré-visualização ainda não é uma anotação do
                        // documento: recebe um id provisório só para reusar o
                        // mesmo desenho da forma já criada.
                        let preview = Layer { id: 0, shape, style: session.style() };
                        paint_shape(&shape_painter, &preview, to_screen);
                    }
                }
            }

            // Contorno tracejado da anotação selecionada (ferramenta Mover) —
            // no painter sem clip, para continuar visível se a anotação foi
            // arrastada para fora da imagem.
            if session.tool == Tool::Select {
                if let Some(layer) = session.selected.and_then(|i| session.doc.layers().get(i)) {
                    let color = ui.visuals().selection.stroke.color;
                    draw_selection_outline(ctx, &painter, layer, to_screen, color);
                    draw_handles(&painter, layer, to_screen, ui.visuals().selection.bg_fill);
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
