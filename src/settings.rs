//! Janela de configurações (RF-05), acessível pelo menu da bandeja.
//!
//! Edita um rascunho (`draft`) de `Config`; "Salvar" publica o rascunho em
//! `pending_apply`, e o `App` aplica com efeito imediato (re-registro de
//! atalhos, autostart, persistência em `config.json`).
//!
//! O widget de atalho combina "clique e pressione a combinação" (para teclas
//! que o egui enxerga) com seletor explícito de tecla + modificadores — a
//! tecla `PrintScreen` não chega como evento de teclado do egui, então o
//! seletor é o caminho garantido para ela.

use egui::{Color32, Key, RichText};

use crate::config::{Config, CtrlWheel, HotkeyDef, ToolKeysConfig};
use crate::editor::shapes::Tool;
use crate::hotkeys::HotkeyAction;

/// Estado da janela de configurações.
pub struct SettingsState {
    pub draft: Config,
    /// Ação cujo atalho está em modo "pressione a combinação".
    capturing: Option<HotkeyAction>,
    /// Falhas do último apply (ex.: atalho tomado por outro app).
    pub last_failures: Vec<String>,
    /// Pedido de fechamento da janela.
    pub close_requested: bool,
    /// Rascunho pronto para o App aplicar.
    pub pending_apply: Option<Config>,
}

impl SettingsState {
    pub fn new(config: Config) -> Self {
        Self {
            draft: config,
            capturing: None,
            last_failures: Vec::new(),
            close_requested: false,
            pending_apply: None,
        }
    }
}

/// Teclas oferecidas no seletor (nomes = `keyboard_types::Code`).
const KEY_CHOICES: &[&str] = &[
    "PrintScreen", "ScrollLock", "Pause", "Insert", "Home", "End", "PageUp", "PageDown",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK",
    "KeyL", "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV",
    "KeyW", "KeyX", "KeyY", "KeyZ",
    "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8",
    "Digit9", "Space", "Enter", "Backquote", "Minus", "Equal",
];

pub fn show(ctx: &egui::Context, state: &mut SettingsState) {
    if ctx.input(|i| i.viewport().close_requested()) {
        state.close_requested = true;
        return;
    }

    // Modo captura: lê a próxima combinação de teclas que o egui reportar.
    if let Some(action) = state.capturing {
        if let Some(update) = capture_combo(ctx) {
            match update {
                CaptureResult::Cancel => state.capturing = None,
                CaptureResult::Combo(def) => {
                    *hotkey_mut(&mut state.draft, action) = def;
                    state.capturing = None;
                }
            }
        }
    }

    let conflicts = conflict_pairs(&state.draft);
    let tool_conflicts = tool_key_conflicts(&state.draft.editor.tool_keys);

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            crate::theme::card(ui, |ui| {
                ui.heading("Atalhos de teclado");
                ui.add_space(6.0);
                egui::Grid::new("hotkeys_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for action in [
                            HotkeyAction::Fullscreen,
                            HotkeyAction::Region,
                            HotkeyAction::Edit,
                        ] {
                            ui.label(action.label());
                            hotkey_editor(ui, state, action);
                            ui.end_row();
                        }
                    });

                if !conflicts.is_empty() {
                    ui.add_space(4.0);
                    for (a, b) in &conflicts {
                        ui.label(
                            RichText::new(format!(
                                "⚠ Conflito: \"{}\" e \"{}\" usam o mesmo atalho.",
                                a.label(),
                                b.label()
                            ))
                            .color(Color32::from_rgb(230, 80, 70)),
                        );
                    }
                }
                for failure in &state.last_failures {
                    ui.label(RichText::new(failure).color(Color32::from_rgb(235, 150, 60)));
                }
            });

            ui.add_space(10.0);
            crate::theme::card(ui, |ui| {
                ui.heading("Capturas");
                ui.add_space(6.0);
                ui.label("Pasta de destino:");
                ui.horizontal(|ui| {
                    let mut shown = if state.draft.output_dir.trim().is_empty() {
                        crate::config::default_output_dir().display().to_string()
                    } else {
                        state.draft.output_dir.clone()
                    };
                    if ui
                        .add(egui::TextEdit::singleline(&mut shown).desired_width(330.0))
                        .changed()
                    {
                        state.draft.output_dir = shown;
                    }
                    if ui.button("Procurar…").clicked() {
                        if let Some(dir) =
                            crate::platform::dialog::pick_folder("Escolha a pasta de capturas")
                        {
                            state.draft.output_dir = dir.display().to_string();
                        }
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Escopo da tela cheia:");
                    egui::ComboBox::from_id_salt("fullscreen_scope")
                        .selected_text(state.draft.fullscreen_scope.label())
                        .show_ui(ui, |ui| {
                            for scope in [
                                crate::config::FullscreenScope::AllMonitors,
                                crate::config::FullscreenScope::Primary,
                                crate::config::FullscreenScope::MonitorUnderCursor,
                            ] {
                                ui.selectable_value(
                                    &mut state.draft.fullscreen_scope,
                                    scope,
                                    scope.label(),
                                );
                            }
                        });
                });
            });

            ui.add_space(10.0);
            crate::theme::card(ui, |ui| {
                ui.heading("Editor");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Cor padrão:");
                    let rgba = crate::config::parse_color(&state.draft.editor.default_color);
                    let mut rgb = [rgba[0], rgba[1], rgba[2]];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        state.draft.editor.default_color =
                            crate::config::format_color([rgb[0], rgb[1], rgb[2], 255]);
                    }
                    ui.add_space(12.0);
                    ui.label("Traço:");
                    ui.add(
                        egui::DragValue::new(&mut state.draft.editor.default_stroke_width)
                            .range(crate::editor::STROKE_MIN..=crate::editor::STROKE_MAX)
                            .speed(0.2),
                    );
                    ui.add_space(12.0);
                    ui.label("Fonte:");
                    ui.add(
                        egui::DragValue::new(&mut state.draft.editor.default_font_size)
                            .range(crate::editor::FONT_MIN..=crate::editor::FONT_MAX)
                            .speed(0.5),
                    );
                });

                ui.add_space(10.0);
                ui.label("Atalhos das ferramentas (uma letra, sem modificador):");
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    let tk = &mut state.draft.editor.tool_keys;
                    for (tool, key) in [
                        (Tool::Select, &mut tk.select),
                        (Tool::Line, &mut tk.line),
                        (Tool::Arrow, &mut tk.arrow),
                        (Tool::Rect, &mut tk.rect),
                        (Tool::Ellipse, &mut tk.ellipse),
                        (Tool::Text, &mut tk.text),
                        (Tool::Crop, &mut tk.crop),
                    ] {
                        ui.label(tool.label());
                        egui::ComboBox::from_id_salt(("tool_key", tool.label()))
                            .selected_text(key.clone())
                            .width(48.0)
                            .show_ui(ui, |ui| {
                                for c in 'A'..='Z' {
                                    ui.selectable_value(key, c.to_string(), c.to_string());
                                }
                            });
                        ui.add_space(8.0);
                    }
                });
                for (a, b) in &tool_conflicts {
                    ui.label(
                        RichText::new(format!(
                            "⚠ Conflito: \"{}\" e \"{}\" usam a mesma tecla.",
                            a.label(),
                            b.label()
                        ))
                        .color(Color32::from_rgb(230, 80, 70)),
                    );
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Ctrl+roda ajusta:");
                    egui::ComboBox::from_id_salt("ctrl_wheel")
                        .selected_text(state.draft.editor.ctrl_wheel.label())
                        .show_ui(ui, |ui| {
                            for mode in [CtrlWheel::StrokeFont, CtrlWheel::Zoom] {
                                ui.selectable_value(
                                    &mut state.draft.editor.ctrl_wheel,
                                    mode,
                                    mode.label(),
                                );
                            }
                        });
                });
            });

            ui.add_space(10.0);
            crate::theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Iniciar com o Windows").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        crate::theme::toggle_switch(ui, &mut state.draft.start_with_windows);
                    });
                });
            });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let can_save = conflicts.is_empty() && tool_conflicts.is_empty();
                let accent = ui.visuals().selection.bg_fill;
                let save = egui::Button::new(
                    RichText::new("Salvar").color(ui.visuals().selection.stroke.color),
                )
                .fill(accent);
                if ui
                    .add_enabled(can_save, save)
                    .on_hover_text("Aplica imediatamente e grava no config.json")
                    .clicked()
                {
                    state.last_failures.clear();
                    state.pending_apply = Some(state.draft.clone());
                }
                if ui.button("Fechar").clicked() {
                    state.close_requested = true;
                }
                if ui.button("Restaurar padrões").clicked() {
                    state.draft = Config::default();
                }
            });
        });
    });
}

/// Editor de um atalho: seletor de modificadores/tecla + modo captura.
fn hotkey_editor(ui: &mut egui::Ui, state: &mut SettingsState, action: HotkeyAction) {
    let capturing = state.capturing == Some(action);
    let def = hotkey_mut(&mut state.draft, action);

    ui.horizontal(|ui| {
        let mut ctrl = has_mod(def, "CTRL");
        let mut shift = has_mod(def, "SHIFT");
        let mut alt = has_mod(def, "ALT");
        let mut win = has_mod(def, "WIN");
        let mut changed = false;
        changed |= ui.toggle_value(&mut ctrl, "Ctrl").changed();
        changed |= ui.toggle_value(&mut shift, "Shift").changed();
        changed |= ui.toggle_value(&mut alt, "Alt").changed();
        changed |= ui.toggle_value(&mut win, "Win").changed();
        if changed {
            def.modifiers.clear();
            if ctrl {
                def.modifiers.push("CTRL".into());
            }
            if shift {
                def.modifiers.push("SHIFT".into());
            }
            if alt {
                def.modifiers.push("ALT".into());
            }
            if win {
                def.modifiers.push("WIN".into());
            }
        }

        egui::ComboBox::from_id_salt(("hotkey_code", action.label()))
            .selected_text(def.code.clone())
            .width(130.0)
            .show_ui(ui, |ui| {
                for choice in KEY_CHOICES {
                    ui.selectable_value(&mut def.code, (*choice).to_string(), *choice);
                }
            });

        let label = if capturing {
            "Pressione a combinação… (Esc cancela)"
        } else {
            "Detectar…"
        };
        if ui
            .selectable_label(capturing, label)
            .on_hover_text(
                "Pressione a combinação desejada. Obs.: a tecla PrintScreen não é \
                 detectável por aqui — selecione-a na lista ao lado.",
            )
            .clicked()
        {
            state.capturing = if capturing { None } else { Some(action) };
        }
    });
}

fn has_mod(def: &HotkeyDef, name: &str) -> bool {
    def.modifiers.iter().any(|m| {
        let up = m.to_ascii_uppercase();
        match name {
            "CTRL" => up == "CTRL" || up == "CONTROL",
            "WIN" => up == "WIN" || up == "SUPER" || up == "META" || up == "CMD",
            other => up == other,
        }
    })
}

fn hotkey_mut(config: &mut Config, action: HotkeyAction) -> &mut HotkeyDef {
    match action {
        HotkeyAction::Fullscreen => &mut config.hotkeys.fullscreen,
        HotkeyAction::Region => &mut config.hotkeys.region,
        HotkeyAction::Edit => &mut config.hotkeys.edit,
    }
}

/// Pares de ações com o mesmo atalho (aviso de conflito, RF-05).
fn conflict_pairs(config: &Config) -> Vec<(HotkeyAction, HotkeyAction)> {
    let entries = [
        (HotkeyAction::Fullscreen, &config.hotkeys.fullscreen),
        (HotkeyAction::Region, &config.hotkeys.region),
        (HotkeyAction::Edit, &config.hotkeys.edit),
    ];
    let normalize = |def: &HotkeyDef| {
        let mut mods: Vec<String> = def
            .modifiers
            .iter()
            .map(|m| {
                let up = m.to_ascii_uppercase();
                match up.as_str() {
                    "CONTROL" => "CTRL".to_string(),
                    "SUPER" | "META" | "CMD" => "WIN".to_string(),
                    _ => up,
                }
            })
            .collect();
        mods.sort();
        (mods, def.code.trim().to_string())
    };
    let mut out = Vec::new();
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            if normalize(entries[i].1) == normalize(entries[j].1) {
                out.push((entries[i].0, entries[j].0));
            }
        }
    }
    out
}

/// Pares de ferramentas do editor com a mesma tecla **efetiva** (issue #4):
/// compara o que o editor realmente usará — valor inválido cai no padrão da
/// ferramenta (`editor::parse_tool_key`), então um config editado à mão pode
/// colidir via fallback mesmo com strings diferentes.
fn tool_key_conflicts(keys: &ToolKeysConfig) -> Vec<(Tool, Tool)> {
    let defaults = ToolKeysConfig::default();
    let entries = [
        (Tool::Select, &keys.select, &defaults.select),
        (Tool::Line, &keys.line, &defaults.line),
        (Tool::Arrow, &keys.arrow, &defaults.arrow),
        (Tool::Rect, &keys.rect, &defaults.rect),
        (Tool::Ellipse, &keys.ellipse, &defaults.ellipse),
        (Tool::Text, &keys.text, &defaults.text),
        (Tool::Crop, &keys.crop, &defaults.crop),
    ];
    let effective = |configured: &str, fallback: &str| {
        crate::editor::parse_tool_key(configured)
            .or_else(|| crate::editor::parse_tool_key(fallback))
    };
    let mut out = Vec::new();
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let a = effective(entries[i].1, entries[i].2);
            let b = effective(entries[j].1, entries[j].2);
            if a.is_some() && a == b {
                out.push((entries[i].0, entries[j].0));
            }
        }
    }
    out
}

enum CaptureResult {
    Cancel,
    Combo(HotkeyDef),
}

/// Lê a próxima tecla não-modificadora pressionada (com seus modificadores).
fn capture_combo(ctx: &egui::Context) -> Option<CaptureResult> {
    ctx.input(|i| {
        for event in &i.events {
            // Ctrl+C/X/V chegam como eventos de clipboard, não como teclas.
            let ctrl_combo = match event {
                egui::Event::Copy => Some("KeyC"),
                egui::Event::Cut => Some("KeyX"),
                egui::Event::Paste(_) => Some("KeyV"),
                _ => None,
            };
            if let Some(code) = ctrl_combo {
                return Some(CaptureResult::Combo(HotkeyDef {
                    modifiers: vec!["CTRL".to_string()],
                    code: code.to_string(),
                }));
            }

            if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
                if *key == Key::Escape {
                    return Some(CaptureResult::Cancel);
                }
                if let Some(code) = egui_key_to_code(*key) {
                    let mut mods = Vec::new();
                    if modifiers.ctrl || modifiers.command {
                        mods.push("CTRL".to_string());
                    }
                    if modifiers.shift {
                        mods.push("SHIFT".to_string());
                    }
                    if modifiers.alt {
                        mods.push("ALT".to_string());
                    }
                    return Some(CaptureResult::Combo(HotkeyDef {
                        modifiers: mods,
                        code: code.to_string(),
                    }));
                }
            }
        }
        None
    })
}

/// Mapeia `egui::Key` → nome de `keyboard_types::Code`.
fn egui_key_to_code(key: Key) -> Option<&'static str> {
    use Key::*;
    Some(match key {
        A => "KeyA", B => "KeyB", C => "KeyC", D => "KeyD", E => "KeyE", F => "KeyF",
        G => "KeyG", H => "KeyH", I => "KeyI", J => "KeyJ", K => "KeyK", L => "KeyL",
        M => "KeyM", N => "KeyN", O => "KeyO", P => "KeyP", Q => "KeyQ", R => "KeyR",
        S => "KeyS", T => "KeyT", U => "KeyU", V => "KeyV", W => "KeyW", X => "KeyX",
        Y => "KeyY", Z => "KeyZ",
        Num0 => "Digit0", Num1 => "Digit1", Num2 => "Digit2", Num3 => "Digit3",
        Num4 => "Digit4", Num5 => "Digit5", Num6 => "Digit6", Num7 => "Digit7",
        Num8 => "Digit8", Num9 => "Digit9",
        F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6",
        F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
        Space => "Space", Enter => "Enter", Tab => "Tab", Insert => "Insert",
        Delete => "Delete", Home => "Home", End => "End", PageUp => "PageUp",
        PageDown => "PageDown", ArrowUp => "ArrowUp", ArrowDown => "ArrowDown",
        ArrowLeft => "ArrowLeft", ArrowRight => "ArrowRight",
        Minus => "Minus", Equals => "Equal", Backtick => "Backquote",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_conflicts_ignoring_order_and_case() {
        let mut cfg = Config::default();
        cfg.hotkeys.fullscreen = HotkeyDef::new(&["SHIFT", "CTRL"], "PrintScreen");
        cfg.hotkeys.edit = HotkeyDef::new(&["ctrl", "shift"], "PrintScreen");
        let pairs = conflict_pairs(&cfg);
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn defaults_have_no_conflicts() {
        assert!(conflict_pairs(&Config::default()).is_empty());
    }

    #[test]
    fn tool_key_conflicts_ignore_case() {
        let mut keys = ToolKeysConfig::default();
        assert!(tool_key_conflicts(&keys).is_empty());
        keys.rect = "t".into(); // colide com Texto ("T")
        let pairs = tool_key_conflicts(&keys);
        assert_eq!(pairs, vec![(Tool::Rect, Tool::Text)]);
    }

    #[test]
    fn tool_key_conflicts_detect_fallback_collision() {
        // "XY" é inválida → o editor usa o padrão do Mover ("M"), que colide
        // com a Linha configurada em "m" — mesmo com strings diferentes.
        let keys = ToolKeysConfig {
            select: "XY".into(),
            line: "m".into(),
            ..ToolKeysConfig::default()
        };
        assert_eq!(tool_key_conflicts(&keys), vec![(Tool::Select, Tool::Line)]);
    }
}
