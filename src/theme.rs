//! Tema Fluent (Windows 11) para o egui: paleta clara/escura seguindo o
//! sistema, cor de destaque do Windows, cantos arredondados e a fonte
//! **Segoe UI Variable** do sistema para a UI.
//!
//! A fonte embutida Inter continua registrada como família nomeada `"Inter"`
//! — o canvas do editor a usa explicitamente para manter o WYSIWYG com a
//! exportação (`ab_glyph` rasteriza com a mesma TTF embutida).

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, Theme};

/// Família nomeada da fonte embutida (preview do editor = exportação).
pub const INTER: &str = "Inter";
/// Família nomeada da fonte de UI do Windows 11 (quando disponível).
const SEGOE: &str = "Segoe UI Variable";

// ---------------------------------------------------------------------------
// Fontes
// ---------------------------------------------------------------------------

/// Instala Inter (embutida) e, quando disponível, a Segoe UI Variable do
/// sistema como fonte primária da UI. Inter fica também como família nomeada
/// para o canvas do editor.
pub fn install_fonts(ctx: &egui::Context) {
    // Sem a feature "default_fonts" do egui, o `default()` vem vazio: toda
    // família precisa ser preenchida aqui — o epaint entra em pânico ao
    // formatar texto com uma família sem fonte alguma.
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        INTER.into(),
        Arc::new(egui::FontData::from_static(crate::editor::FONT_BYTES)),
    );
    fonts
        .families
        .insert(FontFamily::Name(INTER.into()), vec![INTER.into()]);

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, INTER.into());
    // A UI não usa monoespaçada, mas o `TextStyle::Monospace` do egui existe e
    // qualquer widget pode alcançá-lo: fica na Inter em vez de vazio.
    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace.insert(0, INTER.into());

    if let Some(bytes) = system_ui_font() {
        fonts
            .font_data
            .insert(SEGOE.into(), Arc::new(egui::FontData::from_owned(bytes)));
        let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
        proportional.insert(0, SEGOE.into());
    }

    ctx.set_fonts(fonts);
}

/// Lê a Segoe UI Variable (ou Segoe UI) da pasta de fontes do Windows.
fn system_ui_font() -> Option<Vec<u8>> {
    let windir = std::env::var_os("WINDIR")?;
    let fonts = std::path::Path::new(&windir).join("Fonts");
    for name in ["SegUIVar.ttf", "segoeui.ttf"] {
        if let Ok(bytes) = std::fs::read(fonts.join(name)) {
            return Some(bytes);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Paleta
// ---------------------------------------------------------------------------

/// Aplica o tema Fluent aos dois modos e segue o claro/escuro do sistema.
pub fn apply(ctx: &egui::Context) {
    let accent = accent_color();
    ctx.set_theme(egui::ThemePreference::System);
    ctx.style_mut_of(Theme::Light, |style| style_win11(style, false, accent));
    ctx.style_mut_of(Theme::Dark, |style| style_win11(style, true, accent));
}

/// Cor de destaque do Windows (`HKCU\...\DWM\AccentColor`, ABGR); fallback
/// no azul padrão do Windows 11.
fn accent_color() -> Color32 {
    #[cfg(windows)]
    if let Some(abgr) = read_hkcu_dword("Software\\Microsoft\\Windows\\DWM", "AccentColor") {
        let [r, g, b, _a] = abgr.to_le_bytes();
        return Color32::from_rgb(r, g, b);
    }
    Color32::from_rgb(0x00, 0x67, 0xC0)
}

#[cfg(windows)]
fn read_hkcu_dword(subkey: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
    };

    let sub: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let val: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: buffers válidos pelo escopo; RegGetValueW só escreve `size`
    // bytes em `data`.
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut u32 as *mut core::ffi::c_void,
            &mut size,
        )
    };
    (ok == 0).then_some(data)
}

fn style_win11(style: &mut egui::Style, dark: bool, accent: Color32) {
    let (window, card, card_hover, card_active, border, border_strong, text, text_weak, input_bg) =
        if dark {
            (
                Color32::from_rgb(0x20, 0x20, 0x20), // fundo Mica escuro
                Color32::from_rgb(0x2B, 0x2B, 0x2B), // cartão / botão
                Color32::from_rgb(0x32, 0x32, 0x32),
                Color32::from_rgb(0x27, 0x27, 0x27),
                Color32::from_rgb(0x3A, 0x3A, 0x3A),
                Color32::from_rgb(0x45, 0x45, 0x45),
                Color32::from_rgb(0xFF, 0xFF, 0xFF),
                Color32::from_rgb(0xC8, 0xC8, 0xC8),
                Color32::from_rgb(0x1F, 0x1F, 0x1F),
            )
        } else {
            (
                Color32::from_rgb(0xF3, 0xF3, 0xF3), // fundo Mica claro
                Color32::from_rgb(0xFD, 0xFD, 0xFD),
                Color32::from_rgb(0xF4, 0xF4, 0xF4),
                Color32::from_rgb(0xEA, 0xEA, 0xEA),
                Color32::from_rgb(0xE0, 0xE0, 0xE0),
                Color32::from_rgb(0xC6, 0xC6, 0xC6),
                Color32::from_rgb(0x1B, 0x1B, 0x1B),
                Color32::from_rgb(0x5D, 0x5D, 0x5D),
                Color32::from_rgb(0xFF, 0xFF, 0xFF),
            )
        };
    let on_accent = readable_on(accent);

    let visuals = &mut style.visuals;
    visuals.panel_fill = window;
    visuals.window_fill = window;
    visuals.window_stroke = Stroke::new(1.0_f32, border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.faint_bg_color = card_hover;
    visuals.extreme_bg_color = input_bg;
    visuals.code_bg_color = card_active;
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = Stroke::new(1.0_f32, on_accent);
    visuals.slider_trailing_fill = true;
    visuals.override_text_color = Some(text);

    let radius = CornerRadius::same(5);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = window;
    widgets.noninteractive.weak_bg_fill = window;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text_weak);
    widgets.noninteractive.corner_radius = radius;

    widgets.inactive.bg_fill = card;
    widgets.inactive.weak_bg_fill = card;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text);
    widgets.inactive.corner_radius = radius;

    widgets.hovered.bg_fill = card_hover;
    widgets.hovered.weak_bg_fill = card_hover;
    widgets.hovered.bg_stroke = Stroke::new(1.0_f32, border_strong);
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, text);
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;

    widgets.active.bg_fill = card_active;
    widgets.active.weak_bg_fill = card_active;
    widgets.active.bg_stroke = Stroke::new(1.0_f32, accent);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, text);
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;

    widgets.open.bg_fill = card_active;
    widgets.open.weak_bg_fill = card_active;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, accent);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, text);
    widgets.open.corner_radius = radius;

    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::Vec2::new(8.0, 8.0);
    spacing.button_padding = egui::Vec2::new(12.0, 6.0);
    spacing.interact_size.y = 30.0;
    spacing.slider_width = 90.0;
    spacing.menu_margin = egui::Margin::same(6);
    spacing.window_margin = egui::Margin::same(14);

    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(17.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
}

/// Preto ou branco, o que contrastar melhor sobre `bg`.
fn readable_on(bg: Color32) -> Color32 {
    let luma = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if luma > 150.0 { Color32::from_rgb(0x1B, 0x1B, 0x1B) } else { Color32::WHITE }
}

// ---------------------------------------------------------------------------
// Widgets Fluent
// ---------------------------------------------------------------------------

/// "Card" Fluent: fundo de cartão, borda de 1 px e cantos de 8 px.
pub fn card<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let dark = ui.visuals().dark_mode;
    let fill = if dark {
        Color32::from_rgb(0x2B, 0x2B, 0x2B)
    } else {
        Color32::from_rgb(0xFB, 0xFB, 0xFB)
    };
    let border = if dark {
        Color32::from_rgb(0x3A, 0x3A, 0x3A)
    } else {
        Color32::from_rgb(0xE5, 0xE5, 0xE5)
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
}

/// Interruptor no estilo do Windows 11 (substitui checkbox). O rótulo fica a
/// cargo do chamador (padrão Fluent: texto à esquerda, interruptor à direita).
pub fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let track = egui::Vec2::new(40.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(track, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let how_on = ui.ctx().animate_bool(response.id, *on);
    let visuals = ui.visuals();
    let accent = visuals.selection.bg_fill;
    let (track_fill, track_stroke, knob_fill) = if *on {
        (accent, Stroke::NONE, readable_on(accent))
    } else {
        (
            visuals.extreme_bg_color,
            Stroke::new(1.0_f32, visuals.widgets.inactive.bg_stroke.color),
            visuals.widgets.inactive.fg_stroke.color,
        )
    };

    let radius = track.y / 2.0;
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(radius as u8),
        track_fill,
        track_stroke,
        egui::StrokeKind::Inside,
    );
    let knob_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    painter.circle_filled(egui::Pos2::new(knob_x, rect.center().y), radius - 4.0, knob_fill);

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sem a feature `default_fonts` do egui nada vem preenchido de graça, e uma
    /// família sem fonte só se manifesta em runtime: o epaint entra em pânico ao
    /// formatar o primeiro texto. O teste percorre as três famílias que a UI e o
    /// editor alcançam.
    #[test]
    fn every_family_has_a_font() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            for family in [
                FontFamily::Proportional,
                FontFamily::Monospace,
                FontFamily::Name(INTER.into()),
            ] {
                ctx.fonts(|fonts| {
                    let galley = fonts.layout_no_wrap(
                        "RustShot".to_owned(),
                        FontId::new(14.0, family.clone()),
                        Color32::WHITE,
                    );
                    assert!(
                        galley.rect.width() > 0.0,
                        "família {family:?} não rasterizou glifo algum"
                    );
                });
            }
        });
    }
}
