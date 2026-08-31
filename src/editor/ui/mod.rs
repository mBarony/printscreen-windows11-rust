//! Janela do editor: toolbar, canvas com zoom/pan, interações de desenho e
//! atalhos (RF-04, §8).
//!
//! Convenção de espaços: as formas vivem em px da imagem; `zoom` é px físicos
//! de tela por px de imagem; o canvas desenha em pontos do egui
//! (`pontos = px_físicos / pixels_per_point`). Em zoom 100%, 1 px da imagem
//! ocupa exatamente 1 px físico do monitor.
//!
//! Submódulos: `toolbar` (barra), `canvas` (área de desenho), `paint`
//! (desenho das anotações), `interact` (seleção, alças e teclas) e `text`
//! (caixa de texto inline). Aqui ficam o laço da janela e as ações que
//! encerram a sessão.

mod canvas;
mod interact;
mod paint;
mod text;
mod toolbar;

use egui::{
    Key, Modifiers,
    Pos2, Vec2,
};

use crate::clipboard;
use crate::notify;
use crate::storage::{self, SaveTarget};

use super::shapes::{
    Point, Shape, Tool,
};
use super::EditorSession;
use interact::{close_edit_run, handle_layer_keys, settle_edit_run};

/// Tempo de cada quadro do GIF "antes e depois", em centésimos de segundo.
/// Um segundo é o bastante para ler o quadro e curto o bastante para a
/// diferença saltar aos olhos.
const GIF_DELAY_CS: u16 = 100;

/// Texturas das imagens coladas, por `source`.
///
/// Ficam na sessão, e não no documento: o documento é a fonte da verdade e
/// não conhece GPU nenhuma. São montadas sob demanda pelo canvas.
pub(super) type ImageTextures = std::collections::HashMap<u32, egui::TextureHandle>;

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
        let (undo, redo, copy, save, pin, cancel, tool_key, confirm) = ctx.input_mut(|i| {
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
                i.consume_key(Modifiers::COMMAND, Key::P),
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
        if pin {
            session.pin_requested = true;
        }
        if save {
            save_and_close(session, target);
        }
        if cancel {
            request_close(session);
        }
    }

    handle_layer_keys(ctx, session);
    settle_edit_run(ctx, session);
    persist_session(session);

    // Botão X da janela → mesmo fluxo do Esc (com confirmação se preciso).
    if ctx.input(|i| i.viewport().close_requested()) && !session.finished {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        request_close(session);
    }

    toolbar::draw(ctx, session, target);
    canvas::draw(ctx, session);

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
pub(super) fn select_tool(session: &mut EditorSession, tool: Tool) {
    if session.tool == tool {
        return;
    }
    // Trocar de ferramenta no meio de um arrasto (possível pelos atalhos de
    // teclado) cancela o arrasto — um `drag` órfão viraria forma espúria.
    session.drag = None;
    cancel_move(session);
    // O conta-gotas é uma escapada, não uma troca: guarda de onde veio e
    // preserva a seleção, já que a cor amostrada é justamente para aplicar
    // nela. Qualquer outra ferramenta desfaz a seleção como sempre.
    if tool == Tool::Eyedropper {
        session.tool_before_eyedropper = Some(session.tool);
    } else {
        session.tool_before_eyedropper = None;
        session.clear_selection();
    }
    session.marquee = None;
    session.tool = tool;
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
pub(super) fn cancel_move(session: &mut EditorSession) {
    // Uma corrida de empurrões pendente precisa fechar antes: ela também
    // segura um `begin_move` no documento.
    close_edit_run(session);
    let dragging = session.move_drag.take().is_some() | session.resize_drag.take().is_some();
    if dragging {
        session.doc.abort_move();
    }
}

/// Ctrl+Z: com um arrasto de reposicionamento ou de alça em andamento,
/// desfaz só ele; caso contrário desfaz a última edição. A seleção é limpa
/// porque os índices das formas podem mudar.
pub(super) fn perform_undo(session: &mut EditorSession) {
    close_edit_run(session);
    let dragging = session.move_drag.take().is_some() | session.resize_drag.take().is_some();
    if dragging {
        session.doc.abort_move();
    } else {
        let before = session.doc.image_version();
        session.doc.undo();
        refit_if_image_changed(session, before);
    }
    session.clear_selection();
}

pub(super) fn perform_redo(session: &mut EditorSession) {
    cancel_move(session);
    let before = session.doc.image_version();
    session.doc.redo();
    refit_if_image_changed(session, before);
    session.clear_selection();
}

// ---------------------------------------------------------------------------
// Recorte (issue #5)
// ---------------------------------------------------------------------------

/// Aplica a região pendente: a imagem passa a ser só ela e as anotações
/// acompanham o conteúdo. Sem região pendente, é um no-op (o Enter é inócuo
/// nas demais ferramentas).
pub(super) fn apply_crop(session: &mut EditorSession) {
    let Some((min, max)) = session.crop_pending.take() else { return };
    let (img_w, img_h) = (session.doc.visible_image().width(), session.doc.visible_image().height());

    // A região já vem clampeada à imagem; floor/ceil preferem incluir o
    // pixel de borda a perdê-lo. `x`/`y` param um pixel antes do fim para
    // que a imagem resultante nunca tenha lado zero.
    let x = (min.x.floor().max(0.0) as u32).min(img_w.saturating_sub(1));
    let y = (min.y.floor().max(0.0) as u32).min(img_h.saturating_sub(1));
    let w = ((max.x.ceil().max(0.0) as u32).saturating_sub(x)).clamp(1, img_w - x);
    let h = ((max.y.ceil().max(0.0) as u32).saturating_sub(y)).clamp(1, img_h - y);

    session.doc.crop(x, y, w, h);
    reset_view(session);
    session.clear_selection();
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
pub(super) fn reset_view(session: &mut EditorSession) {
    session.texture = None;
    session.zoom = None;
    session.pan = Vec2::ZERO;
}

pub(super) fn commit_text_input(session: &mut EditorSession) {
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
pub(super) fn copy_and_close(session: &mut EditorSession) {
    commit_text_input(session);
    let base = session.doc.content_image().clone();
    let layers = session.doc.layers().to_vec();
    let backdrop = session.doc.backdrop();
    let sources = session.doc.images().clone();
    crate::jobs::spawn(move || match super::render::render(&base, &layers, backdrop, &sources) {
        Ok(final_image) => match clipboard::copy_image(&final_image) {
            Ok(()) => notify::toast(
                "Copiado para a área de transferência",
                "A imagem anotada está pronta para colar.",
            ),
            Err(err) => notify::toast_error("Falha ao copiar", &format!("{err:#}")),
        },
        Err(err) => notify::toast_error("Falha ao renderizar anotações", &format!("{err:#}")),
    });
    forget_session();
    session.finished = true;
}

/// Salva um GIF de dois quadros que alterna entre a captura sem anotações e
/// a captura anotada — o "antes e depois" do trabalho feito aqui.
///
/// **Não fecha o editor**: é um artefato a mais, não o fim da tarefa, e quem
/// o pede quase sempre ainda quer salvar a imagem parada depois.
///
/// O quadro "antes" sai do mesmo `content_image` do "depois", que já traz as
/// redações queimadas — os dois quadros têm o mesmo tamanho de graça, e o GIF
/// nunca revela o que foi ocultado.
pub(super) fn save_gif(session: &mut EditorSession, target: &SaveTarget) {
    commit_text_input(session);
    let base = session.doc.content_image().clone();
    let layers = session.doc.layers().to_vec();
    let backdrop = session.doc.backdrop();
    let sources = session.doc.images().clone();
    let target = target.clone();
    crate::jobs::spawn(move || {
        let antes = super::render::render(&base, &[], backdrop, &sources);
        let depois = super::render::render(&base, &layers, backdrop, &sources);
        let (antes, depois) = match (antes, depois) {
            (Ok(a), Ok(d)) => (a, d),
            (Err(err), _) | (_, Err(err)) => {
                notify::toast_error("Falha ao renderizar o GIF", &format!("{err:#}"));
                return;
            }
        };
        match crate::storage::write_gif(&target, &[&antes, &depois], GIF_DELAY_CS) {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                notify::toast("GIF antes e depois salvo", &name);
            }
            Err(err) => notify::toast_error("Falha ao salvar o GIF", &format!("{err:#}")),
        }
    });
}

/// Arrasta a captura anotada para outro programa, como um arquivo.
///
/// **Bloqueia** enquanto o arrasto dura: `DoDragDrop` roda um laço modal
/// próprio, e ele exige a thread que é dona da janela. O editor fica parado
/// nesse intervalo, que é o mesmo que acontece em qualquer aplicativo nativo
/// durante um arrasto.
///
/// O PNG vai para a pasta temporária com o nome do template configurado: é o
/// nome que aparece no destino, e `captura.png` diria menos que a data.
pub(super) fn drag_out(session: &mut EditorSession, target: &SaveTarget) {
    commit_text_input(session);
    let base = session.doc.content_image().clone();
    let layers = session.doc.layers().to_vec();
    let backdrop = session.doc.backdrop();
    let sources = session.doc.images().clone();

    let imagem = match super::render::render(&base, &layers, backdrop, &sources) {
        Ok(imagem) => imagem,
        Err(err) => {
            notify::toast_error("Falha ao renderizar", &format!("{err:#}"));
            return;
        }
    };
    let destino = match crate::storage::write_png_into(
        &std::env::temp_dir(),
        &target.filename_template,
        &imagem,
    ) {
        Ok(caminho) => caminho,
        Err(err) => {
            notify::toast_error("Falha ao preparar o arquivo", &format!("{err:#}"));
            return;
        }
    };
    // O arquivo temporário não é apagado depois: um destino que só o lê
    // quando o usuário confirma encontraria o vazio. Quem limpa é o Windows.
    let soltou = crate::platform::dragdrop::drag_file(&destino);
    log::info!("arrasto de {} terminou (solto: {soltou})", destino.display());
}

/// Ctrl+S: renderiza, salva na pasta configurada e fecha o editor (RF-04).
pub(super) fn save_and_close(session: &mut EditorSession, target: &SaveTarget) {
    commit_text_input(session);
    let base = session.doc.content_image().clone();
    let layers = session.doc.layers().to_vec();
    let backdrop = session.doc.backdrop();
    let sources = session.doc.images().clone();
    let opacity = session.opacity;
    let mut target = target.clone();
    // Uma imagem translúcida gravada em JPG sairia opaca: a escolha do
    // usuário vence a preferência de formato.
    if opacity < 1.0 {
        target.image_format = crate::imgout::Format::Png;
    }
    crate::jobs::spawn(move || match super::render::render(&base, &layers, backdrop, &sources) {
        Ok(final_image) => {
            let final_image = if opacity < 1.0 {
                final_image.with_opacity(opacity)
            } else {
                final_image
            };
            match storage::write_image(&target, &final_image) {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                log::info!("captura anotada salva em {}", path.display());
                notify::toast("Captura salva", &name);
            }
            Err(err) => notify::toast_error("Falha ao salvar captura", &format!("{err:#}")),
            }
        }
        Err(err) => notify::toast_error("Falha ao renderizar anotações", &format!("{err:#}")),
    });
    forget_session();
    session.finished = true;
}

/// Esc / X / Cancelar: fecha, confirmando se houver anotações (RF-04).
pub(super) fn request_close(session: &mut EditorSession) {
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
    if session.marquee.take().is_some() {
        // Primeiro Esc abandona o laço em andamento.
        return;
    }
    cancel_move(session);
    if session.selected.is_some() {
        // Primeiro Esc apenas desfaz a seleção.
        session.clear_selection();
        return;
    }
    if session.dirty() {
        session.confirm_discard = true;
    } else {
        forget_session();
    session.finished = true;
    }
}

pub(super) fn confirm_discard_modal(ctx: &egui::Context, session: &mut EditorSession) {
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
                    forget_session();
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


/// Grava a sessão em disco quando ela muda, para um fechamento inesperado
/// não levar o trabalho junto.
///
/// A imagem de origem vai uma vez só — reescrever dezenas de MB a cada
/// anotação seria absurdo —, e de novo quando o teto do histórico a muda. O
/// log, que é pequeno, vai a cada edição.
///
/// A assinatura de sujeira é a revisão do documento, e não o número de
/// operações aplicadas: com o log no teto esse número fica parado em
/// `MAX_OPS`, e tudo a partir da 101ª edição deixava de ser gravado.
fn persist_session(session: &mut EditorSession) {
    let revision = session.doc.revision();
    if session.saved_revision == Some(revision) {
        return;
    }
    let dir = crate::config::state_dir();
    let baseline = session.doc.baseline_version();
    if session.saved_baseline != Some(baseline) {
        if let Err(err) = super::session_file::save_source(&session.doc, &dir) {
            // Falhar aqui não pode atrapalhar a edição: perde-se a rede de
            // segurança, não o trabalho em andamento.
            log::warn!("sessão não pôde ser gravada: {err:#}");
            session.saved_revision = Some(revision);
            return;
        }
        session.saved_baseline = Some(baseline);
    }
    // As imagens coladas vão antes do log: recuperar uma sessão cujo log
    // aponta para pixels que não estão no disco deixaria buracos na tela.
    if let Err(err) = super::session_file::save_pasted(&session.doc, &dir) {
        log::warn!("imagens coladas não puderam ser gravadas: {err:#}");
    }
    if let Err(err) = super::session_file::save_log(&session.doc, &dir) {
        log::warn!("log da sessão não pôde ser gravado: {err:#}");
    }
    session.saved_revision = Some(revision);
}

/// Apaga a sessão gravada: o editor terminou por vontade do usuário.
pub(super) fn forget_session() {
    super::session_file::clear(&crate::config::state_dir());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EditorConfig;
    use crate::imgbuf::RgbaImage;

    fn sessao_com_duas_linhas() -> EditorSession {
        let mut session = EditorSession::new(
            1,
            RgbaImage::filled(32, 24, [0, 0, 0, 255]),
            &EditorConfig::default(),
        );
        let style = session.style();
        for i in 0..2 {
            let y = 4.0 + i as f32 * 8.0;
            session
                .doc
                .push(Shape::Line { a: Point::new(2.0, y), b: Point::new(20.0, y) }, style);
        }
        session
    }

    /// Refazer traz anotações de volta e mexe nos índices, então a seleção
    /// tem de cair inteira. Enquanto só o `selected` caía, o contorno das
    /// anotações em `selection` continuava desenhado — sem alças e surdo às
    /// teclas.
    #[test]
    fn refazer_limpa_a_selecao_inteira() {
        let mut session = sessao_com_duas_linhas();
        perform_undo(&mut session);

        // Com a segunda linha desfeita, o usuário seleciona a que restou e
        // só então manda refazer.
        session.selected = Some(0);
        session.selection = vec![0];
        perform_redo(&mut session);

        assert_eq!(session.selected, None);
        assert!(session.selection.is_empty(), "nada continua marcado como selecionado");
    }
}
