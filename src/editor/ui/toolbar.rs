//! Barra de ferramentas do editor: seleção de ferramenta, cor, traço,
//! preenchimento, texto e as ações de histórico e saída.

use egui::{
    Color32, CornerRadius, Key,
    Pos2, Sense, Stroke, StrokeKind, Vec2,
};

use crate::storage::SaveTarget;

use crate::editor::icons::{self, Icon};
use crate::editor::shapes::{RedactionStyle, Tool, CORNER_RADIUS_MAX};
use crate::editor::{
    EditorSession,
    FONT_MAX, FONT_MIN, PALETTE, STROKE_MAX,
    STROKE_MIN,
};
use super::{
    apply_crop, copy_and_close, perform_redo, perform_undo, request_close, save_and_close,
    select_tool,
};
use super::interact::{
    restyle_selection, selected_is_redaction, selected_is_text, selected_shape_takes_fill,
};

/// Lado do botão de ícone da toolbar, em pontos.
pub(super) const ICON_BUTTON: f32 = 26.0;
/// Lado da amostra de cor da paleta, em pontos.
const SWATCH: f32 = 20.0;

/// Dica de hover de uma ferramenta da toolbar: a tecla configurada (issue
/// #1/#4) e, para Mover/Recortar, o que a ferramenta faz.
pub(super) fn tool_hint(tool: Tool, key: Option<Key>) -> String {
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

// ---------------------------------------------------------------------------

/// Botão quadrado de ícone: fundo só quando ativo ou sob o cursor, para a
/// faixa ficar leve — a identificação vem do ícone e do tooltip.
pub(super) fn icon_button(ui: &mut egui::Ui, icon: Icon, selected: bool, enabled: bool) -> egui::Response {
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
pub(super) fn color_swatch(ui: &mut egui::Ui, rgba: [u8; 4], selected: bool) -> egui::Response {
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

pub(super) fn draw(ctx: &egui::Context, session: &mut EditorSession, target: &SaveTarget) {
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

                // --- Cor (também repinta a anotação selecionada) ---
                for color in PALETTE {
                    if color_swatch(ui, color, session.color == color).clicked() {
                        session.color = color;
                        restyle_selection(ctx, session);
                    }
                }
                let mut rgb = [session.color[0], session.color[1], session.color[2]];
                if ui
                    .color_edit_button_srgb(&mut rgb)
                    .on_hover_text("Cor personalizada")
                    .changed()
                {
                    session.color = [rgb[0], rgb[1], rgb[2], 255];
                    restyle_selection(ctx, session);
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
                    restyle_selection(ctx, session);
                }

                // --- Preenchimento e cantos: só valem para retângulo e
                // elipse, seja a ferramenta ativa ou a anotação selecionada ---
                let shape_tool = matches!(session.tool, Tool::Rect | Tool::Ellipse)
                    || selected_shape_takes_fill(session);
                ui.add_enabled_ui(shape_tool, |ui| {
                    if icon_button(ui, Icon::Fill, session.filled, shape_tool)
                        .on_hover_text("Preencher a forma")
                        .clicked()
                    {
                        session.filled = !session.filled;
                        restyle_selection(ctx, session);
                    }
                    let mut radius = session.corner_radius;
                    if ui
                        .add(
                            egui::DragValue::new(&mut radius)
                                .range(0.0..=CORNER_RADIUS_MAX)
                                .speed(0.2)
                                .fixed_decimals(0)
                                .prefix("⌜"),
                        )
                        .on_hover_text("Raio dos cantos do retângulo")
                        .changed()
                    {
                        session.corner_radius = radius.round().clamp(0.0, CORNER_RADIUS_MAX);
                        restyle_selection(ctx, session);
                    }
                });

                // --- Modo da redação: mosaico ou cor chapada ---
                let redacting = session.tool == Tool::Redact || selected_is_redaction(session);
                ui.add_enabled_ui(redacting, |ui| {
                    let solid = session.redaction == RedactionStyle::Solid;
                    if icon_button(ui, Icon::Redact, solid, redacting)
                        .on_hover_text(format!(
                            "Redação: {} (clique para alternar)",
                            session.redaction.label()
                        ))
                        .clicked()
                    {
                        session.redaction = if solid {
                            RedactionStyle::Pixelate
                        } else {
                            RedactionStyle::Solid
                        };
                        restyle_selection(ctx, session);
                    }
                });

                // --- Tamanho da fonte: com a ferramenta Texto ou com um
                // texto selecionado, que é como se redimensiona um texto ---
                let text_tool = session.tool == Tool::Text || selected_is_text(session);
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
                        restyle_selection(ctx, session);
                    }
                    if icon_button(ui, Icon::TextPill, session.text_pill, text_tool)
                        .on_hover_text("Fundo claro atrás do texto")
                        .clicked()
                    {
                        session.text_pill = !session.text_pill;
                        restyle_selection(ctx, session);
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
pub(super) fn separator(ui: &mut egui::Ui) {
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
pub(super) fn stroke_preview(ui: &mut egui::Ui, width: f32) {
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
