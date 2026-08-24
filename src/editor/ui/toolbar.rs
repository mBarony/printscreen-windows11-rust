//! Barra de ferramentas do editor: seleção de ferramenta, cor, traço,
//! preenchimento, texto e as ações de histórico e saída.

use egui::{
    Color32, CornerRadius, Key,
    Pos2, Sense, Stroke, StrokeKind, Vec2,
};

use crate::storage::SaveTarget;

use crate::editor::icons::{self, Icon};
use crate::editor::backdrop::BackdropStyle;
use crate::editor::shapes::{
    RedactionStyle, Tool, CORNER_RADIUS_MAX, MAGNIFICATION_MAX, MAGNIFICATION_MIN,
};
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
    restyle_selection, selected_is_redaction, selected_is_spotlight, selected_is_text,
    selected_shape_takes_fill,
};

/// Lado do botão de ícone da toolbar, em pontos.
pub(super) const ICON_BUTTON: f32 = 26.0;
/// Lado da amostra de cor da paleta, em pontos.
const SWATCH: f32 = 20.0;

/// Dica de hover de uma ferramenta da toolbar: a tecla configurada (issue
/// #1/#4) e, para as ferramentas cujo nome não basta, o que ela faz.
pub(super) fn tool_hint(tool: Tool, key: Option<Key>) -> String {
    let key_name = match key {
        Some(key) => key.name().to_string(),
        None => "sem atalho".to_string(),
    };
    match tool {
        Tool::Select => format!("{key_name} — selecione uma anotação para mover ou redimensionar"),
        Tool::Crop => format!("{key_name} — arraste a área a manter e confirme com Enter"),
        // "Ocultar" soa reversível e não é: a região é queimada na imagem.
        // Como o nome não carrega isso, a dica carrega.
        Tool::Redact => format!("{key_name} — apaga a região de vez; não há como revelar depois"),
        // O nome diz o quê, não como: o número sai sozinho, em sequência.
        Tool::Marker => format!("{key_name} — cada clique carimba o próximo número"),
        // Vizinha de Recortar e quase homônima, mas faz o oposto.
        Tool::Cut => format!("{key_name} — joga fora a faixa arrastada e junta o que sobrou"),
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
/// Ferramentas na ordem em que aparecem, agrupadas pelo que fazem com a
/// imagem: apontar, desenhar por cima, esconder, mudar a moldura — e o
/// conta-gotas por último, encostado na cor, porque é de lá que ela sai e é
/// para lá que ele a leva.
const TOOL_GROUPS: [&[Tool]; 5] = [
    &[Tool::Select],
    &[
        Tool::Line,
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Freehand,
        Tool::Highlighter,
        Tool::Marker,
        Tool::Text,
    ],
    &[Tool::Redact, Tool::Spotlight],
    &[Tool::Crop, Tool::Cut],
    &[Tool::Eyedropper],
];

pub(super) fn draw(ctx: &egui::Context, session: &mut EditorSession, target: &SaveTarget) {
    egui::TopBottomPanel::top("editor_toolbar")
        .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(8, 5)))
        .show(ctx, |ui| {
            // Histórico e saída são as ações que sempre têm de estar à mão, e
            // enquanto vinham no fim da mesma fila eram justamente elas que
            // sobravam quando a barra não cabia: cortadas na borda ou jogadas
            // para uma segunda linha. `shrink_left` acrescenta a direita antes
            // de tudo e limita a esquerda ao que sobrar — quem cede espaço é o
            // resto, nunca elas.
            // As duas metades não podem tocar `session` ao mesmo tempo, então
            // a direita só relata o que foi clicado e a ação é aplicada aqui
            // fora, com o empréstimo já livre.
            let can_undo = session.doc.can_undo();
            let can_redo = session.doc.can_redo();
            let mut pedido = None;

            egui::Sides::new()
                .height(ICON_BUTTON)
                .shrink_left()
                .show(
                ui,
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                        left_side(ctx, ui, session);
                    });
                },
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                    pedido = right_side(ui, can_undo, can_redo);
                },
            );

            match pedido {
                Some(RightAction::Undo) => perform_undo(session),
                Some(RightAction::Redo) => perform_redo(session),
                Some(RightAction::Copy) => copy_and_close(session),
                Some(RightAction::Save) => save_and_close(session, target),
                Some(RightAction::Close) => request_close(session),
                None => {}
            }
        });
}

/// O que a ponta direita pediu neste quadro.
enum RightAction {
    Undo,
    Redo,
    Copy,
    Save,
    Close,
}

/// Histórico e saída, encostados na ponta direita.
///
/// São acrescentados da direita para a esquerda: o primeiro daqui é o que
/// fica mais na ponta. Lidos na tela, saem como `| desfazer refazer | copiar
/// salvar fechar`.
fn right_side(ui: &mut egui::Ui, can_undo: bool, can_redo: bool) -> Option<RightAction> {
    let mut pedido = None;

    if icon_button(ui, Icon::Close, false, true)
        .on_hover_text("Cancelar (Esc)")
        .clicked()
    {
        pedido = Some(RightAction::Close);
    }
    if icon_button(ui, Icon::Save, false, true)
        .on_hover_text("Salvar e fechar (Ctrl+S)")
        .clicked()
    {
        pedido = Some(RightAction::Save);
    }
    if icon_button(ui, Icon::Copy, false, true)
        .on_hover_text("Copiar e fechar (Ctrl+C)")
        .clicked()
    {
        pedido = Some(RightAction::Copy);
    }

    separator(ui);

    if icon_button(ui, Icon::Redo, false, can_redo)
        .on_hover_text("Refazer (Ctrl+Y)")
        .clicked()
    {
        pedido = Some(RightAction::Redo);
    }
    if icon_button(ui, Icon::Undo, false, can_undo)
        .on_hover_text("Desfazer (Ctrl+Z)")
        .clicked()
    {
        pedido = Some(RightAction::Undo);
    }

    separator(ui);

    pedido
}

/// Ferramentas, cor, traço e os controles da ferramenta ativa.
fn left_side(ctx: &egui::Context, ui: &mut egui::Ui, session: &mut EditorSession) {
    // --- Ferramentas ---
    for (index, group) in TOOL_GROUPS.iter().enumerate() {
        if index > 0 {
            separator(ui);
        }
        for &tool in *group {
            let key = session
                .tool_keys
                .iter()
                .find(|(candidate, _)| *candidate == tool)
                .and_then(|(_, key)| *key);
            let selected = session.tool == tool;
            if icon_button(ui, Icon::of(tool), selected, true)
                .on_hover_text(format!("{} — {}", tool.label(), tool_hint(tool, key)))
                .clicked()
            {
                select_tool(session, tool);
            }
        }
        // Confirmação do recorte, junto da ferramenta que a pede.
        if group.contains(&Tool::Crop) && session.tool == Tool::Crop {
            let ready = session.crop_pending.is_some();
            if icon_button(ui, Icon::Check, false, ready)
                .on_hover_text("Aplicar recorte (Enter)")
                .clicked()
            {
                apply_crop(session);
            }
        }
    }

    separator(ui);

    // --- Cor: na barra fica só a atual; a paleta sai num popup ---
    //
    // As oito amostras lado a lado custavam uns 180 pontos numa barra que já
    // não cabia a 150% de escala. Trocar de cor passa a custar um clique a
    // mais, e é o que menos dói: numa anotação escolhe-se a cor uma vez e
    // desenha-se várias.
    color_popup(ctx, ui, session);

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

    // Daqui até a moldura, cada bloco só existe quando serve para a ferramenta
    // ativa (ou para a anotação selecionada). Antes ficavam todos sempre na
    // barra, cinzentos: uns 450 pontos de controles mortos.

    // --- Preenchimento e cantos: retângulo e elipse ---
    if matches!(session.tool, Tool::Rect | Tool::Ellipse) || selected_shape_takes_fill(session) {
        separator(ui);
        if icon_button(ui, Icon::Fill, session.filled, true)
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
                    .fixed_decimals(0),
            )
            .on_hover_text("Raio dos cantos do retângulo")
            .changed()
        {
            session.corner_radius = radius.round().clamp(0.0, CORNER_RADIUS_MAX);
            restyle_selection(ctx, session);
        }
    }

    // --- Modo da redação: mosaico ou cor chapada ---
    if session.tool == Tool::Redact || selected_is_redaction(session) {
        separator(ui);
        let solid = session.redaction == RedactionStyle::Solid;
        if icon_button(ui, Icon::Redact, solid, true)
            .on_hover_text(format!(
                "Ocultar: {} (clique para alternar)",
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
    }

    // --- Holofote: recorte e ampliação ---
    if session.tool == Tool::Spotlight || selected_is_spotlight(session) {
        separator(ui);
        if icon_button(ui, Icon::Spotlight, false, true)
            .on_hover_text(format!(
                "Holofote: {} (clique para alternar)",
                session.spotlight.label()
            ))
            .clicked()
        {
            session.spotlight = session.spotlight.next();
            restyle_selection(ctx, session);
        }
        let mut zoom = session.magnification;
        if ui
            .add(
                egui::DragValue::new(&mut zoom)
                    .range(MAGNIFICATION_MIN..=MAGNIFICATION_MAX)
                    .speed(0.05)
                    .fixed_decimals(1)
                    .suffix("×"),
            )
            .on_hover_text("Quanto o holofote amplia")
            .changed()
        {
            session.magnification = zoom.clamp(MAGNIFICATION_MIN, MAGNIFICATION_MAX);
            restyle_selection(ctx, session);
        }
    }

    // --- Tamanho da fonte: com a ferramenta Texto ou com um texto
    // selecionado, que é como se redimensiona um texto ---
    if session.tool == Tool::Text || selected_is_text(session) {
        separator(ui);
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
        if icon_button(ui, Icon::TextPill, session.text_pill, true)
            .on_hover_text("Fundo claro atrás do texto")
            .clicked()
        {
            session.text_pill = !session.text_pill;
            restyle_selection(ctx, session);
        }
    }

    separator(ui);

    // --- Moldura decorativa: vale para a imagem inteira, então não depende
    // de ferramenta e fica sempre à mão ---
    let backdrop = session.doc.backdrop();
    if icon_button(ui, Icon::Backdrop, backdrop != BackdropStyle::None, true)
        .on_hover_text(format!("Fundo: {} (clique para trocar)", backdrop.label()))
        .clicked()
    {
        session.doc.set_backdrop(backdrop.next());
    }

    // --- Reconhecer o texto da imagem ---
    //
    // Vizinha da moldura porque também age sobre a imagem inteira, e não
    // sobre uma anotação. O editor só levanta a bandeira: quem reconhece é o
    // `app`, que é quem tem onde pendurar o aviso do resultado.
    if icon_button(ui, Icon::Ocr, false, true)
        .on_hover_text("Reconhecer o texto da imagem e copiá-lo")
        .clicked()
    {
        log::info!("barra do editor: reconhecimento de texto pedido");
        session.ocr_requested = true;
        // Quem recolhe a bandeira é a janela-raiz, e ela dorme: sem este
        // toque o pedido ficaria armado até algo mais a acordar. Mesmo
        // tropeço que a janela de configurações já teve.
        ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
    }
}

/// Amostra da cor atual que abre a paleta ao ser clicada.
fn color_popup(ctx: &egui::Context, ui: &mut egui::Ui, session: &mut EditorSession) {
    let button = color_swatch(ui, session.color, true).on_hover_text("Cor — clique para escolher");

    egui::Popup::menu(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            ui.horizontal(|ui| {
                for color in PALETTE {
                    if color_swatch(ui, color, session.color == color).clicked() {
                        session.color = color;
                        restyle_selection(ctx, session);
                    }
                }
            });
            let mut rgb = [session.color[0], session.color[1], session.color[2]];
            if ui
                .color_edit_button_srgb(&mut rgb)
                .on_hover_text("Cor personalizada")
                .changed()
            {
                session.color = [rgb[0], rgb[1], rgb[2], 255];
                restyle_selection(ctx, session);
            }
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
