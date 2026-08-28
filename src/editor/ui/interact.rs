//! Interações com anotações já criadas: teclas que agem sobre a seleção,
//! conta-gotas, corridas de edição contínua e as alças de redimensionamento.

use egui::{
    CursorIcon, Key, Modifiers,
    Pos2,
};


use crate::editor::shapes::{
    Handle, Point, Shape,
};
use crate::editor::{
    EditorSession, MoveDrag, ResizeDrag, DUPLICATE_OFFSET, HANDLE_EDGE_ROOM_PTS, HANDLE_HIT_PTS,
    HIT_TOLERANCE_PTS, NUDGE_COALESCE_SECS, NUDGE_STEP, NUDGE_STEP_SHIFT,
};
use super::ToScreen;
use super::paint::hit_test;

// ---------------------------------------------------------------------------

/// `Delete`/`Backspace` apaga, `Alt+D` duplica e as setas empurram.
pub(super) fn handle_layer_keys(ctx: &egui::Context, session: &mut EditorSession) {
    if session.text_input.is_some() || session.confirm_discard || ctx.wants_keyboard_input() {
        return;
    }

    // Selecionar tudo vem antes da guarda de seleção, porque é justamente o
    // atalho de quem ainda não tem nada selecionado — com ele, apagar todas
    // as anotações é Ctrl+A e Delete.
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
        let total = session.doc.layers().len();
        if total > 0 {
            session.selection = (0..total).collect();
            // A última é a de cima: é dela que a barra mostra o estilo.
            session.selected = Some(total - 1);
        }
        return;
    }

    // Guias não dependem de seleção, então vêm antes da guarda. `Alt+H` e
    // `Alt+V` criam no cursor; `Alt+Shift+G` limpa todas.
    if let Some(guia) = ctx.input_mut(|i| {
        let h = i.consume_key(Modifiers::ALT, Key::H);
        let v = i.consume_key(Modifiers::ALT, Key::V);
        (h || v).then_some(h)
    }) {
        if let Some(p) = session.guide_hint {
            session.guides.push(crate::editor::Guide {
                horizontal: guia,
                pos: if guia { p.y } else { p.x },
            });
        }
        return;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::ALT | Modifiers::SHIFT, Key::G)) {
        session.guides.clear();
        return;
    }

    let Some(index) = session.selected else {
        return;
    };

    let (delete, duplicate, dx, dy, now) = ctx.input_mut(|i| {
        let shift = i.modifiers.shift;
        // Com `Shift` o passo é maior, então o atalho a consumir também
        // carrega o `Shift` — senão a seta não seria reconhecida.
        let (mods, step) = if shift {
            (Modifiers::SHIFT, NUDGE_STEP_SHIFT)
        } else {
            (Modifiers::NONE, NUDGE_STEP)
        };
        let mut dx = 0.0;
        let mut dy = 0.0;
        if i.consume_key(mods, Key::ArrowLeft) {
            dx -= step;
        }
        if i.consume_key(mods, Key::ArrowRight) {
            dx += step;
        }
        if i.consume_key(mods, Key::ArrowUp) {
            dy -= step;
        }
        if i.consume_key(mods, Key::ArrowDown) {
            dy += step;
        }
        (
            i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace),
            i.consume_key(Modifiers::ALT, Key::D),
            dx,
            dy,
            i.time,
        )
    });
    // Inverter a seta é uma alteração de forma, não de estilo: entra pela
    // mesma porta do movimento, para virar um passo de desfazer.
    if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::R)) {
        close_edit_run(session);
        session.doc.begin_move();
        if session.doc.reverse_arrow(index) {
            session.doc.end_move();
        } else {
            session.doc.abort_move();
        }
        return;
    }

    if delete {
        close_edit_run(session);
        let picked = std::mem::take(&mut session.selection);
        session.selected = None;
        session.doc.delete_all(&picked);
        return;
    }
    if duplicate {
        close_edit_run(session);
        let (dx, dy) = duplicate_offset(session, index);
        if session.doc.duplicate(index, dx, dy).is_some() {
            let copy = session.doc.layers().len() - 1;
            session.selected = Some(copy);
            session.selection = vec![copy];
        }
        return;
    }
    if dx != 0.0 || dy != 0.0 {
        // A primeira seta abre a corrida; as seguintes só empurram.
        if session.edit_run_until.is_none() {
            session.doc.begin_move();
        }
        let picked = session.selection.clone();
        session.doc.translate_all(&picked, dx, dy);
        session.edit_run_until = Some(now + NUDGE_COALESCE_SECS);
    }
}

/// Fecha a corrida de empurrões quando o silêncio passa da janela, gravando
/// **um** passo de desfazer para o conjunto todo. Enquanto ela estiver
/// aberta, pede repaint para que o prazo seja de fato verificado.
pub(super) fn settle_edit_run(ctx: &egui::Context, session: &mut EditorSession) {
    let Some(deadline) = session.edit_run_until else {
        return;
    };
    let now = ctx.input(|i| i.time);
    if now >= deadline {
        close_edit_run(session);
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(deadline - now));
    }
}

/// Encerra a corrida imediatamente (outra ação começou).
pub(super) fn close_edit_run(session: &mut EditorSession) {
    if session.edit_run_until.take().is_some() {
        session.doc.end_move();
    }
}

/// Como o conta-gotas deve ler a imagem.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum PickMode {
    /// O pixel exato sob o cursor.
    Point,
    /// O tom mais escuro da vizinhança — num texto, a cor da letra e não a
    /// do fundo, que é o que um clique quase sempre pegaria.
    TextColor,
    /// A média de um retângulo, para áreas com ruído ou gradiente.
    Average(Point),
}

/// Toma uma cor da imagem em `p` e volta à ferramenta anterior.
///
/// A cor amostrada é sempre opaca: o que interessa é o tom que está na tela,
/// não a transparência do buffer.
pub(super) fn pick_color(
    ctx: &egui::Context,
    session: &mut EditorSession,
    p: Point,
    mode: PickMode,
) {
    let image = session.doc.visible_image();
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 {
        return;
    }
    let coord = |p: Point| {
        (
            (p.x.max(0.0) as u32).min(w - 1),
            (p.y.max(0.0) as u32).min(h - 1),
        )
    };
    let (x, y) = coord(p);

    session.color = match mode {
        PickMode::Point => {
            let px = image.pixel(x, y);
            [px[0], px[1], px[2], 255]
        }
        PickMode::TextColor => crate::color::darkest_around(image, x, y),
        PickMode::Average(other) => {
            let (ox, oy) = coord(other);
            crate::color::average(image, x, y, ox, oy)
        }
    };
    restyle_selection(ctx, session);
    // A seleção é preservada de propósito: tirar uma cor é para aplicá-la.
    if let Some(previous) = session.tool_before_eyedropper.take() {
        session.tool = previous;
    }
}

/// A anotação selecionada aceita preenchimento e cantos arredondados?
pub(super) fn selected_shape_takes_fill(session: &EditorSession) -> bool {
    session
        .selected
        .and_then(|i| session.doc.layers().get(i))
        .is_some_and(|layer| {
            matches!(layer.shape, Shape::Rect { .. } | Shape::Ellipse { .. })
        })
}

/// A anotação selecionada deixa um traço ao longo de um caminho? É onde
/// tracejado e pontilhado querem dizer alguma coisa.
pub(super) fn selected_takes_line_style(session: &EditorSession) -> bool {
    session
        .selected
        .and_then(|i| session.doc.layers().get(i))
        .is_some_and(|layer| {
            matches!(
                layer.shape,
                Shape::Line { .. }
                    | Shape::Ruler { .. }
                    | Shape::Arrow { .. }
                    | Shape::Rect { .. }
                    | Shape::Ellipse { .. }
                    | Shape::Freehand { .. }
            )
        })
}

/// A anotação selecionada é uma redação?
pub(super) fn selected_is_redaction(session: &EditorSession) -> bool {
    session
        .selected
        .and_then(|i| session.doc.layers().get(i))
        .is_some_and(|layer| matches!(layer.shape, Shape::Redaction { .. }))
}

/// A anotação selecionada é um holofote?
pub(super) fn selected_is_spotlight(session: &EditorSession) -> bool {
    session
        .selected
        .and_then(|i| session.doc.layers().get(i))
        .is_some_and(|layer| matches!(layer.shape, Shape::Spotlight { .. }))
}

/// A anotação selecionada é um texto?
pub(super) fn selected_is_text(session: &EditorSession) -> bool {
    session
        .selected
        .and_then(|i| session.doc.layers().get(i))
        .is_some_and(|layer| matches!(layer.shape, Shape::Text { .. }))
}

/// Aplica o estilo ativo da toolbar à anotação selecionada.
///
/// Entra na mesma corrida coalescida do empurrão: arrastar o controle de
/// espessura mexe na anotação a cada quadro, mas o histórico recebe um único
/// passo quando o arrasto para.
pub(super) fn restyle_selection(ctx: &egui::Context, session: &mut EditorSession) {
    // Guias não dependem de seleção, então vêm antes da guarda. `Alt+H` e
    // `Alt+V` criam no cursor; `Alt+Shift+G` limpa todas.
    if let Some(guia) = ctx.input_mut(|i| {
        let h = i.consume_key(Modifiers::ALT, Key::H);
        let v = i.consume_key(Modifiers::ALT, Key::V);
        (h || v).then_some(h)
    }) {
        if let Some(p) = session.guide_hint {
            session.guides.push(crate::editor::Guide {
                horizontal: guia,
                pos: if guia { p.y } else { p.x },
            });
        }
        return;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::ALT | Modifiers::SHIFT, Key::G)) {
        session.guides.clear();
        return;
    }

    let Some(index) = session.selected else {
        return;
    };
    if session.edit_run_until.is_none() {
        session.doc.begin_move();
    }
    let style = session.style();
    session.doc.set_style(index, style);
    session.edit_run_until = Some(ctx.input(|i| i.time) + NUDGE_COALESCE_SECS);
}

/// Deslocamento da cópia: para baixo e para a esquerda, invertendo um eixo
/// só quando o padrão sairia da imagem e o sentido oposto couber.
pub(super) fn duplicate_offset(session: &EditorSession, index: usize) -> (f32, f32) {
    let (mut dx, mut dy) = (-DUPLICATE_OFFSET, DUPLICATE_OFFSET);
    let bounds = session
        .doc
        .layers()
        .get(index)
        .and_then(|layer| layer.bbox());
    let Some((min, max)) = bounds else {
        return (dx, dy); // texto: sem caixa conhecida aqui, vale o padrão
    };
    let (w, h) = (session.doc.visible_image().width() as f32, session.doc.visible_image().height() as f32);
    if min.x + dx < 0.0 && max.x - dx <= w {
        dx = -dx;
    }
    if max.y + dy > h && min.y - dy >= 0.0 {
        dy = -dy;
    }
    (dx, dy)
}

/// Press com a ferramenta Mover.
///
/// A alça da anotação já selecionada tem prioridade sobre o corpo das
/// anotações: ela fica *fora* da forma, e sem essa precedência clicar numa
/// alça selecionaria o que estivesse atrás dela.
pub(super) fn begin_select_drag(
    ctx: &egui::Context,
    session: &mut EditorSession,
    ts: ToScreen,
    origin: Pos2,
) {
    // Pegar o mouse encerra a corrida de empurrões pendente.
    close_edit_run(session);
    if let Some((index, handle)) = handle_at(session, ts, origin) {
        session.selected = Some(index);
        session.doc.begin_move();
        session.resize_drag = Some(ResizeDrag { index, handle });
        return;
    }

    let p = ts.inverse(origin);
    let tol = HIT_TOLERANCE_PTS / ts.scale;
    let hit = session
        .doc
        .layers()
        .iter()
        .enumerate()
        .rev() // a mais recente (pintada por cima) vence
        .find(|(_, layer)| hit_test(ctx, layer, p, tol, ts))
        .map(|(index, _)| index)
        // Só se nada foi atingido: o miolo de uma forma vazada. Vem depois,
        // e não junto, para o que está dentro dela continuar vencendo.
        .or_else(|| {
            session
                .doc
                .layers()
                .iter()
                .enumerate()
                .rev()
                .find(|(_, layer)| layer.hit_test_interior(p))
                .map(|(index, _)| index)
        });
    match hit {
        Some(index) => {
            // Com `Alt`, arrastar duplica em vez de mover: a cópia nasce no
            // lugar e é ela que segue o ponteiro, então o original fica onde
            // estava. É o `Alt+D` sem ter de reposicionar depois.
            let index = if ctx.input(|i| i.modifiers.alt) {
                match session.doc.duplicate(index, 0.0, 0.0) {
                    Some(_) => session.doc.layers().len() - 1,
                    None => index,
                }
            } else {
                index
            };
            // Clicar numa anotação já selecionada preserva o conjunto: é
            // assim que se arrasta o bloco inteiro depois de laçá-lo.
            if !session.selection.contains(&index) {
                session.selection = vec![index];
            }
            session.selected = Some(index);
            session.doc.begin_move();
            session.move_drag = Some(MoveDrag { last: p, travel: 0.0 });
        }
        None => {
            // Vazio: começa um laço. A seleção antiga só cai quando o laço
            // termina, para um clique errado não custar o trabalho de seleção.
            session.selected = None;
            session.marquee = Some((p, p));
        }
    }
}

/// Fecha o laço: seleciona tudo o que couber inteiramente dentro dele.
pub(super) fn finish_marquee(session: &mut EditorSession) {
    let Some((from, to)) = session.marquee.take() else {
        return;
    };
    let (min, max) = crate::editor::shapes::normalize(from, to);
    session.selection = session.doc.layers_within(min, max);
    session.selected = session.selection.last().copied();
}

/// Alça da anotação selecionada sob o ponteiro, se houver — a mais próxima,
/// para alças vizinhas não disputarem o mesmo clique.
pub(super) fn handle_at(session: &EditorSession, ts: ToScreen, pos: Pos2) -> Option<(usize, Handle)> {
    // Só há alças com uma anotação selecionada.
    if session.selection.len() != 1 {
        return None;
    }
    let index = session.selected?;
    let layer = session.doc.layers().get(index)?;
    let p = ts.inverse(pos);
    let tol = HANDLE_HIT_PTS / ts.scale;
    layer
        .handles(HANDLE_EDGE_ROOM_PTS / ts.scale)
        .into_iter()
        .map(|(handle, at)| {
            let (dx, dy) = (at.x - p.x, at.y - p.y);
            (handle, dx * dx + dy * dy)
        })
        .filter(|(_, dist_sq)| *dist_sq <= tol * tol)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(handle, _)| (index, handle))
}

pub(super) fn handle_cursor(handle: Handle) -> CursorIcon {
    match handle {
        Handle::TopLeft | Handle::BottomRight => CursorIcon::ResizeNwSe,
        Handle::TopRight | Handle::BottomLeft => CursorIcon::ResizeNeSw,
        Handle::Top | Handle::Bottom => CursorIcon::ResizeVertical,
        Handle::Left | Handle::Right => CursorIcon::ResizeHorizontal,
        Handle::Start | Handle::End | Handle::Bend => CursorIcon::Grab,
    }
}

