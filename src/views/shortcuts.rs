use crate::domain::keymap::{Binding, RowAction};
use crate::viewmodels::intent::ShortcutIntent;
use crate::views::theme::cmd;
use eframe::egui::{self, Key};

/// How a binding reads on screen, e.g. ⌘⇧M or Ctrl+⇧M.
pub fn combo(b: &Binding) -> String {
    let key = Key::from_name(&b.key).map_or_else(|| b.key.clone(), |k| k.symbol_or_name().to_string());
    format!("{}{}{key}", cmd(), if b.shift { "⇧" } else { "" })
}

/// Shortcut label for a row action, as the row menu shows it.
pub fn hint(action: RowAction, keymap: &[(RowAction, Binding)]) -> Option<String> {
    if action == RowAction::Delete {
        return Some(Key::Delete.name().to_string());
    }
    keymap.iter().find(|(a, _)| *a == action).map(|(_, b)| combo(b))
}

pub fn read(ctx: &egui::Context, keymap: &[(RowAction, Binding)], has_selection: bool) -> Vec<ShortcutIntent> {
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
            for (action, b) in keymap {
                let Some(key) = Key::from_name(&b.key) else { continue };
                if m.command && m.shift == b.shift && !m.alt && i.consume_key(m, key) {
                    out.push(ShortcutIntent::Selected(*action));
                }
            }
        });
    }
    out
}
