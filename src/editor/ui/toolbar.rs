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
    LineStyle, RedactionStyle, Tool, CORNER_RADIUS_MAX, MAGNIFICATION_MAX, MAGNIFICATION_MIN,
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
    selected_shape_takes_fill, selected_takes_line_style,
};

/// Lado do botão de ícone da toolbar, em pontos.
pub(super) const ICON_BUTTON: f32 = 30.0;
/// Lado da amostra de cor da paleta, em pontos.
const SWATCH: f32 = 20.0;
/// Arredondamento do realce do botão e do fundo dos grupos.
const ROUND: u8 = 8;
/// Folga entre os botões de um mesmo grupo.
const TIGHT: f32 = 2.0;
/// Folga entre grupos — é ela que os separa, no lugar de um traço.
const LOOSE: f32 = 8.0;

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
        // O número sai da imagem, não da tela: é o que a régua mede.
        Tool::Ruler => format!("{key_name} — mede a distância em px da imagem"),
        _ => key_name,
    }
}

// ---------------------------------------------------------------------------

/// Botão quadrado de ícone: fundo só quando ativo ou sob o cursor, para a
/// faixa ficar leve — a identificação vem do ícone e do tooltip.
///
/// O realce do cursor entra e sai por animação curta. Numa fila de dezoito
/// botões, o fundo aparecendo de estalo a cada pixel percorrido pisca; a
/// transição resolve isso sem custar nada além de um `f32` por botão.
pub(super) fn icon_button(ui: &mut egui::Ui, icon: Icon, selected: bool, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(ICON_BUTTON),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let hovered = enabled && response.hovered();
    let warmth = ui.ctx().animate_bool(response.id, hovered);
    let visuals = ui.visuals();

    // A ferramenta ativa fica marcada por um fundo discreto, não pela cor de
    // destaque cheia: numa fila de quinze ícones um retângulo saturado puxa
    // o olho para si e some com o desenho que está por baixo dele.
    let background = if selected {
        visuals.selection.bg_fill.gamma_multiply(0.30)
    } else {
        // Do transparente até o realce de hover, conforme a animação.
        visuals.widgets.hovered.bg_fill.gamma_multiply(warmth * 0.8)
    };
    let color = if !enabled {
        visuals.weak_text_color()
    } else if selected || hovered {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    if background.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(ROUND), background);
    }
    icons::paint(ui.painter(), rect.shrink(ICON_BUTTON * 0.28), icon, color);
    response
}

/// Agrupa controles afins numa unidade que não quebra no meio quando a linha
/// envolve.
///
/// Sem fundo, de propósito: quem separa os grupos é o espaço em volta e um
/// traço fino entre eles. Caixas atrás de cada punhado de ícones competem
/// com os próprios ícones — a barra fica desenhada em vez de organizada.
fn group<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(TIGHT, TIGHT);
        ui.horizontal(|ui| add(ui)).inner
    })
    .inner
}

/// Fronteira entre dois grupos: folga, traço fino, folga.
fn group_divider(ui: &mut egui::Ui) {
    ui.add_space(LOOSE);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, ICON_BUTTON * 0.55), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(1),
        ui.visuals()
            .widgets
            .noninteractive
            .bg_stroke
            .color
            .gamma_multiply(0.8),
    );
    ui.add_space(LOOSE);
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
        Tool::Ruler,
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
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, TIGHT);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, TIGHT);
                        left_side(ctx, ui, session);
                    });
                },
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, TIGHT);
                    pedido = right_side(ui, can_undo, can_redo);
                },
            );

            match pedido {
                Some(RightAction::Undo) => perform_undo(session),
                Some(RightAction::Redo) => perform_redo(session),
                Some(RightAction::Copy) => copy_and_close(session),
                Some(RightAction::Save) => save_and_close(session, target),
                // O editor não cria janelas: levanta a bandeira e o `app`,
                // dono dos viewports, fixa e fecha esta.
                Some(RightAction::Pin) => session.pin_requested = true,
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
    Pin,
    Close,
}

/// Histórico e saída, encostados na ponta direita.
///
/// São acrescentados da direita para a esquerda: o primeiro daqui é o que
/// fica mais na ponta. Lidos na tela, saem como `| desfazer refazer | copiar
/// salvar fechar`.
fn right_side(ui: &mut egui::Ui, can_undo: bool, can_redo: bool) -> Option<RightAction> {
    let mut pedido = None;

    // Sair do editor e desfazer são coisas diferentes, e ficam em blocos
    // diferentes: um clique errado entre elas custa caro dos dois lados.
    group(ui, |ui| {
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
        if icon_button(ui, Icon::Pin, false, true)
            .on_hover_text("Fixar na tela (Ctrl+P) — fica sempre no topo até fechar")
            .clicked()
        {
            pedido = Some(RightAction::Pin);
        }
    });

    group_divider(ui);

    group(ui, |ui| {
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
    });

    ui.add_space(LOOSE);

    pedido
}

/// Ferramentas, cor, traço e os controles da ferramenta ativa.
fn left_side(ctx: &egui::Context, ui: &mut egui::Ui, session: &mut EditorSession) {
    // --- Bloco 1: as ferramentas, com divisórias internas por família ---
    group(ui, |ui| {
        for (index, tools) in TOOL_GROUPS.iter().enumerate() {
            if index > 0 {
                separator(ui);
            }
            for &tool in *tools {
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
            if tools.contains(&Tool::Crop) && session.tool == Tool::Crop {
                let ready = session.crop_pending.is_some();
                if icon_button(ui, Icon::Check, false, ready)
                    .on_hover_text("Aplicar recorte (Enter)")
                    .clicked()
                {
                    apply_crop(session);
                }
            }
        }
    });

    group_divider(ui);

    // --- Bloco 2: como a anotação fica — cor, traço e o que a ferramenta
    // ativa (ou a seleção) acrescenta ---
    group(ui, |ui| tool_options(ctx, ui, session));

    group_divider(ui);

    // --- Bloco 3: o que vale para a imagem inteira ---
    group(ui, |ui| image_options(ui, session));
}

/// Cor, espessura e os controles que só existem para a ferramenta ativa.
fn tool_options(ctx: &egui::Context, ui: &mut egui::Ui, session: &mut EditorSession) {
    // --- Cor: na barra fica só a atual; a paleta sai num popup ---
    //
    // As oito amostras lado a lado custavam uns 180 pontos numa barra que já
    // não cabia a 150% de escala. Trocar de cor passa a custar um clique a
    // mais, e é o que menos dói: numa anotação escolhe-se a cor uma vez e
    // desenha-se várias.
    color_popup(ctx, ui, session);

    separator(ui);

    // --- Espessura do traço: amostra + valor arrastável ---
    stroke_preview(ui, session.stroke_width, session.line);
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

    // --- Padrão do traço: só onde há um traço ao longo de um caminho ---
    //
    // Sem separador antes: padrão e espessura descrevem a mesma coisa — como
    // o traço sai — e ficam de propósito no mesmo grupo visual.
    if session.tool.takes_line_style() || selected_takes_line_style(session) {
        let icone = match session.line {
            LineStyle::Solid => Icon::LineSolid,
            LineStyle::Dashed => Icon::LineDashed,
            LineStyle::Dotted => Icon::LineDotted,
        };
        if icon_button(ui, icone, session.line != LineStyle::Solid, true)
            .on_hover_text(format!(
                "Traço: {} (clique para alternar)",
                session.line.label()
            ))
            .clicked()
        {
            session.line = session.line.next();
            restyle_selection(ctx, session);
        }
        if icon_button(ui, Icon::Sketch, session.sketch, true)
            .on_hover_text("Traço desenhado à mão")
            .clicked()
        {
            session.sketch = !session.sketch;
            restyle_selection(ctx, session);
        }
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
        // A dica carrega a ressalva porque a funcionalidade não pode
        // prometer sigilo que não entrega: onde o OCR não reconhece, nada é
        // ocultado, e o nome do botão sozinho sugeriria o contrário.
        if icon_button(ui, Icon::RedactText, session.redact_words, true)
            .on_hover_text(
                "Ocultar só as palavras da região, preservando gráficos e layout. \
                 É melhor-esforço: o que o reconhecimento não achar continua visível.",
            )
            .clicked()
        {
            session.redact_words = !session.redact_words;
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
}

/// O que age sobre a imagem inteira, e não sobre uma anotação.
fn image_options(ui: &mut egui::Ui, session: &mut EditorSession) {
    // --- Tamanho da imagem inteira ---
    //
    // Em porcentagem, arrastável: escalar é uma decisão de "quanto", não de
    // "mais um passo", e um campo diz melhor onde se está do que dois botões.
    let mut percent = 100.0_f32;
    let response = ui.add(
        egui::DragValue::new(&mut percent)
            .range(10.0..=400.0)
            .speed(1.0)
            .suffix("%")
            .fixed_decimals(0),
    );
    if response.changed() {
        // O campo volta sempre a 100%: o fator é relativo ao tamanho atual,
        // e mostrar um acumulado exigiria guardar o original só para isso.
        session.doc.scale(percent / 100.0);
    }
    response.on_hover_text("Redimensionar a imagem inteira, com as anotações junto");

    // --- Opacidade da exportação ---
    let mut opacidade = session.opacity * 100.0;
    if ui
        .add(
            egui::DragValue::new(&mut opacidade)
                .range(10.0..=100.0)
                .speed(1.0)
                .suffix("% α")
                .fixed_decimals(0),
        )
        .on_hover_text(
            "Opacidade do arquivo salvo. Abaixo de 100% a saída vai em PNG, \
             porque o JPG não tem canal alfa.",
        )
        .changed()
    {
        session.opacity = (opacidade / 100.0).clamp(0.1, 1.0);
    }

    // --- Voltar ao enquadramento original ---
    if icon_button(ui, Icon::Crop, false, true)
        .on_hover_text("Desfazer os recortes e voltar ao enquadramento original")
        .clicked()
    {
        session.doc.reset_crop();
    }

    // --- Moldura decorativa: não depende de ferramenta e fica sempre à mão ---
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
    // A dica traz a cor nos três formatos: o HEX para colar em qualquer
    // lugar, o OKLCH para quem trabalha com design, e o contraste APCA
    // contra branco e preto — que responde "dá para ler texto nesta cor?".
    let rgb = [session.color[0], session.color[1], session.color[2]];
    let dica = format!(
        "Cor — clique para escolher\n{}\n{}\ncontraste APCA: {:.0} sobre branco, {:.0} sobre preto",
        crate::color::format_hex(rgb),
        crate::color::format_oklch(rgb),
        crate::color::apca_contrast(rgb, [255, 255, 255]).abs(),
        crate::color::apca_contrast(rgb, [0, 0, 0]).abs(),
    );
    let button = color_swatch(ui, session.color, true).on_hover_text(dica);

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
/// Divisória fina entre famílias de botões **dentro** de um bloco.
///
/// Entre blocos quem separa é o espaço, não um traço: com o fundo do grupo
/// já marcando a fronteira, um traço ali seria ruído em cima de ruído.
pub(super) fn separator(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, ICON_BUTTON * 0.5), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(1),
        ui.visuals()
            .widgets
            .noninteractive
            .bg_stroke
            .color
            .gamma_multiply(0.7),
    );
    ui.add_space(3.0);
}

/// Amostra da espessura e do padrão atuais — o valor numérico ao lado dá a
/// precisão. A amostra passa pelo mesmo `dash::split` do canvas: uma linha
/// sempre cheia aqui mentiria sobre o que a próxima anotação vai parecer.
pub(super) fn stroke_preview(ui: &mut egui::Ui, width: f32, line: LineStyle) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, ICON_BUTTON), Sense::hover());
    let thickness = (width * 0.7).clamp(1.0, 7.0);
    let cor = ui.visuals().text_color();
    let painter = ui.painter();
    let em_pontos = |p: &crate::editor::shapes::Point| Pos2::new(p.x, p.y);
    let caminho = [
        crate::editor::shapes::Point::new(rect.left() + 1.0, rect.center().y),
        crate::editor::shapes::Point::new(rect.right() - 1.0, rect.center().y),
    ];
    for parte in crate::editor::dash::split(&caminho, line, thickness) {
        if parte.len() == 1 {
            painter.circle_filled(em_pontos(&parte[0]), thickness / 2.0, cor);
        } else {
            painter.line_segment(
                [em_pontos(&parte[0]), em_pontos(&parte[parte.len() - 1])],
                Stroke::new(thickness, cor),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::icons::{geometry, Primitive};

    const PAD: f32 = 16.0;

    /// Prévia da barra montada, sem GPU e sem Windows — o mesmo truque que
    /// `icons::tests::svg_preview` usa para os ícones soltos, aqui aplicado
    /// ao layout: blocos, folgas e estado ativo saem das constantes reais.
    ///
    /// É uma reprodução do layout, não uma captura dele: serve para julgar
    /// proporção e agrupamento, não para provar que o egui desenhou assim.
    ///
    /// `cargo test --bin rustshot ui::toolbar::tests::svg_barra -- --ignored --nocapture`
    #[test]
    #[ignore = "gera a prévia sob demanda"]
    fn svg_barra() {
        // (ícones do bloco, qual deles aparece ativo)
        let tools = ferramentas();
        let blocos: [(&[Icon], Option<usize>); 5] = [
            (&tools, Some(2)),
            (&[Icon::Fill, Icon::TextPill], None),
            (&[Icon::Backdrop, Icon::Ocr], None),
            (&[Icon::Close, Icon::Save, Icon::Copy, Icon::Pin], None),
            (&[Icon::Redo, Icon::Undo], None),
        ];

        let altura = ICON_BUTTON + PAD * 2.0;
        // Cada fronteira entre blocos custa folga + traço + folga.
        let fronteira = LOOSE * 2.0 + 1.0;
        let largura: f32 = blocos
            .iter()
            .map(|(icones, _)| bloco_largura(icones.len()))
            .sum::<f32>()
            + fronteira * (blocos.len() - 1) as f32
            + PAD * 2.0;

        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{largura:.0}\" \
             height=\"{altura:.0}\" viewBox=\"0 0 {largura:.2} {altura:.2}\">\
             <rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>"
        );

        let mut x = PAD;
        for (i, (icones, ativo)) in blocos.into_iter().enumerate() {
            if i > 0 {
                // Fronteira entre grupos: folga, traço fino, folga.
                let cx = x + LOOSE;
                svg.push_str(&format!(
                    "<rect x=\"{cx:.2}\" y=\"{:.2}\" width=\"1\" height=\"{:.2}\" \
                     fill=\"#d6d6da\"/>",
                    PAD + ICON_BUTTON * 0.22,
                    ICON_BUTTON * 0.55
                ));
                x += LOOSE * 2.0 + 1.0;
            }
            svg.push_str(&bloco_svg(icones, ativo, x));
            x += bloco_largura(icones.len());
        }
        svg.push_str("</svg>");
        println!("{svg}");
    }

    /// Os ícones das ferramentas na ordem real da barra — derivados de
    /// `TOOL_GROUPS`, para a prévia não descolar dela.
    fn ferramentas() -> Vec<Icon> {
        TOOL_GROUPS
            .iter()
            .flat_map(|grupo| grupo.iter().map(|&tool| Icon::of(tool)))
            .collect()
    }

    fn bloco_largura(n: usize) -> f32 {
        n as f32 * ICON_BUTTON + n.saturating_sub(1) as f32 * TIGHT
    }

    fn bloco_svg(icones: &[Icon], ativo: Option<usize>, x: f32) -> String {
        let mut svg = String::new();
        for (i, icone) in icones.iter().enumerate() {
            let bx = x + i as f32 * (ICON_BUTTON + TIGHT);
            let by = PAD;
            if ativo == Some(i) {
                svg.push_str(&format!(
                    "<rect x=\"{bx:.2}\" y=\"{by:.2}\" width=\"{ICON_BUTTON:.2}\" \
                     height=\"{ICON_BUTTON:.2}\" rx=\"{ROUND}\" fill=\"#e6e7ea\"/>"
                ));
            }
            svg.push_str(&icone_svg(*icone, bx, by));
        }
        svg
    }

    /// Um ícone dentro do quadrado do botão, com o mesmo recuo da toolbar.
    fn icone_svg(icone: Icon, bx: f32, by: f32) -> String {
        let inset = ICON_BUTTON * 0.28;
        let lado = ICON_BUTTON - inset * 2.0;
        let mut svg = String::new();
        for primitivo in geometry(icone) {
            let (pontos, preenche) = match primitivo {
                Primitive::Stroke(p) => (p, false),
                Primitive::Fill(p) => (p, true),
            };
            let coords: Vec<String> = pontos
                .iter()
                .map(|(px, py)| {
                    format!("{:.2},{:.2}", bx + inset + px * lado, by + inset + py * lado)
                })
                .collect();
            svg.push_str(&if preenche {
                format!("<polygon points=\"{}\" fill=\"#2b2c30\"/>", coords.join(" "))
            } else {
                format!(
                    "<polyline points=\"{}\" fill=\"none\" stroke=\"#2b2c30\" \
                     stroke-width=\"1.6\" stroke-linecap=\"round\" \
                     stroke-linejoin=\"round\"/>",
                    coords.join(" ")
                )
            });
        }
        svg
    }
}
