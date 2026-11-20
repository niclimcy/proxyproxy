use crate::viewmodels::intent::ShortcutIntent;
use eframe::egui;

pub fn read(ctx: &egui::Context) -> Vec<ShortcutIntent> {
    let mut out = vec![];
    let typing = ctx.egui_wants_keyboard_input();
    let (toggle, find, down, up, del, replay, esc) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        (
            cmd && i.key_pressed(egui::Key::B),
            cmd && i.key_pressed(egui::Key::F),
            !cmd && i.key_pressed(egui::Key::ArrowDown),
            !cmd && i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            cmd && i.key_pressed(egui::Key::R),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if toggle {
        out.push(ShortcutIntent::ToggleSidebar);
    }
    if find {
        out.push(ShortcutIntent::FocusSearch);
    }
    if typing {
        if esc {
            out.push(ShortcutIntent::ClearFilter);
        }
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
    out
}
