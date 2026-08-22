//! Caixa de texto inline da ferramenta Texto.

use egui::{
    FontId, Key,
};


use crate::editor::EditorSession;
use super::ToScreen;
use super::paint::color32;
use super::commit_text_input;

// ---------------------------------------------------------------------------

pub(super) fn text_input_overlay(ctx: &egui::Context, session: &mut EditorSession, ts: ToScreen) {
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
            let edit = egui::TextEdit::multiline(&mut input.buffer)
                .font(FontId::new(font_pts, egui::FontFamily::Name(crate::theme::INTER.into())))
                .text_color(color)
                .hint_text("Texto…")
                .desired_rows(1)
                .desired_width(280.0_f32.max(font_pts * 6.0));
            let response = ui.add(edit);
            if !input.focus_requested {
                response.request_focus();
                input.focus_requested = true;
            }
            // Enter agora insere linha; a confirmação é por Ctrl+Enter ou
            // clicando fora da caixa.
            let confirm_key = ui.input(|i| {
                i.key_pressed(Key::Enter) && (i.modifiers.command || i.modifiers.ctrl)
            });
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            if esc {
                cancel = true;
            } else if confirm_key || (response.lost_focus() && !response.has_focus()) {
                commit = true;
            }
        });

    if cancel {
        session.text_input = None;
    } else if commit {
        commit_text_input(session);
    }
}

