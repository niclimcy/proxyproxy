use crate::viewmodels::intent::{RowAction, ShortcutIntent};
use eframe::egui::{self, Key};

/// Cmd+key bindings for the selected row: key, whether Shift is part of it, action.
/// Cmd+M is taken by macOS (Minimize), so mute adds Shift.
pub const ROW_KEYS: [(Key, bool, RowAction); 11] = [
    (Key::R, false, RowAction::Replay),
    (Key::P, false, RowAction::Pin),
    (Key::Slash, false, RowAction::Comment),
    (Key::D, false, RowAction::DiffBase),
    (Key::M, true, RowAction::Mute),
    (Key::Num1, false, RowAction::Color(Some(0))),
    (Key::Num2, false, RowAction::Color(Some(1))),
    (Key::Num3, false, RowAction::Color(Some(2))),
    (Key::Num4, false, RowAction::Color(Some(3))),
    (Key::Num5, false, RowAction::Color(Some(4))),
    (Key::Num0, false, RowAction::Color(None)),
];

pub fn read(ctx: &egui::Context, has_selection: bool) -> Vec<ShortcutIntent> {
    let mut out = vec![];
    let typing = ctx.text_edit_focused();
    // an open menu owns the arrows, Enter and Escape
    let menu_open = egui::Popup::is_any_open(ctx);
    let (toggle, find, down, up, del, esc, settings, record, row_menu) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        (
            cmd && i.key_pressed(Key::B),
            cmd && i.key_pressed(Key::F),
            !cmd && i.key_pressed(Key::ArrowDown),
            !cmd && i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
            i.key_pressed(Key::Escape),
            cmd && i.key_pressed(Key::Comma),
            cmd && i.modifiers.shift && i.key_pressed(Key::P),
            (i.modifiers.shift_only() && i.key_pressed(Key::F10)) || i.key_pressed(Key::Menu),
        )
    });
    if toggle {
        out.push(ShortcutIntent::ToggleSidebar);
    }
    if find {
        out.push(ShortcutIntent::FocusSearch);
    }
    if settings {
        out.push(ShortcutIntent::OpenSettings);
    }
    if record {
        out.push(ShortcutIntent::ToggleCapture);
    }
    if typing {
        if esc {
            out.push(ShortcutIntent::ClearFilter);
        }
        return out;
    }
    if menu_open {
        return out;
    }
    if down || up {
        out.push(ShortcutIntent::Step(down));
    }
    if del {
        out.push(ShortcutIntent::Selected(RowAction::Delete));
    }
    if esc {
        out.push(ShortcutIntent::Deselect);
    }
    if row_menu {
        out.push(ShortcutIntent::OpenRowMenu);
    }
    // only with a selection, so Cmd+0 still resets egui's zoom otherwise
    if has_selection {
        ctx.input_mut(|i| {
            let m = i.modifiers;
            for (key, shift, action) in ROW_KEYS {
                if m.command && m.shift == shift && !m.alt && i.consume_key(m, key) {
                    out.push(ShortcutIntent::Selected(action));
                }
            }
        });
    }
    out
}
