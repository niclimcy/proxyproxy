use crate::viewmodels::intent::ShortcutIntent;
use eframe::egui;

pub fn read(ctx: &egui::Context) -> Vec<ShortcutIntent> {
    let mut out = vec![];
    let typing = ctx.text_edit_focused();
    // an open menu owns the arrows, Enter and Escape
    let menu_open = egui::Popup::is_any_open(ctx);
    let (toggle, find, down, up, del, replay, esc, settings, record, row_menu) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        (
            cmd && i.key_pressed(egui::Key::B),
            cmd && i.key_pressed(egui::Key::F),
            !cmd && i.key_pressed(egui::Key::ArrowDown),
            !cmd && i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            cmd && i.key_pressed(egui::Key::R),
            i.key_pressed(egui::Key::Escape),
            cmd && i.key_pressed(egui::Key::Comma),
            cmd && i.modifiers.shift && i.key_pressed(egui::Key::P),
            (i.modifiers.shift_only() && i.key_pressed(egui::Key::F10)) || i.key_pressed(egui::Key::Menu),
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
        out.push(ShortcutIntent::DeleteSelected);
    }
    if replay {
        out.push(ShortcutIntent::ReplaySelected);
    }
    if esc {
        out.push(ShortcutIntent::Deselect);
    }
    if row_menu {
        out.push(ShortcutIntent::OpenRowMenu);
    }
    out
}
