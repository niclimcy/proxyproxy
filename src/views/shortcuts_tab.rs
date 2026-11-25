use crate::domain::keymap::{BindError, Binding, RowAction, default_binding};
use crate::viewmodels::intent::KeymapIntent;
use crate::viewmodels::keymap_vm::KeymapVm;
use crate::views::shortcuts::combo;
use crate::views::theme::*;
use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

fn label(a: RowAction) -> String {
    match a {
        RowAction::Replay => "Replay / edit".into(),
        RowAction::Pin => "Pin / unpin".into(),
        RowAction::Comment => "Comment".into(),
        RowAction::DiffBase => "Use as diff base".into(),
        RowAction::Mute => "Mute host".into(),
        RowAction::Color(Some(i)) => format!("Color: {}", tag_colors()[i as usize % 5].0),
        RowAction::Color(None) => "Clear color".into(),
        RowAction::Delete => "Delete".into(),
    }
}

fn message(e: BindError) -> String {
    match e {
        BindError::Unsupported => "Use a letter, digit or punctuation key".into(),
        BindError::Reserved => "That shortcut belongs to proxyproxy or the system".into(),
        BindError::InUse(other) => format!("Already used by {}", label(other)),
    }
}

/// The next key pressed in this window, taken so nothing else acts on it.
fn take_key(ui: &egui::Ui) -> Option<(egui::Key, egui::Modifiers)> {
    ui.input_mut(|i| {
        let mut hit = None;
        i.events.retain(|e| match e {
            egui::Event::Key { key, pressed: true, modifiers, .. } if hit.is_none() => {
                hit = Some((*key, *modifiers));
                false
            }
            _ => true,
        });
        hit
    })
}

pub fn show(ui: &mut egui::Ui, vm: &KeymapVm) -> Vec<KeymapIntent> {
    let mut out = vec![];
    if let Some(a) = vm.recording
        && let Some((key, m)) = take_key(ui)
    {
        if key == egui::Key::Escape {
            out.push(KeymapIntent::Cancel);
        } else if m.command && !m.alt {
            out.push(KeymapIntent::Set(a, Binding { key: key.name().into(), shift: m.shift }));
        }
    }
    ui.label(muted(format!(
        "Shortcuts act on the selected request. Each is {} plus a key, with Shift optional. Delete, and Shift+F10 or the Menu key for the full menu, stay fixed.",
        cmd().trim_end_matches('+')
    )));
    ui.add_space(10.0);
    let rows = vm.rows();
    egui::Grid::new("shortcut-grid").num_columns(3).spacing([16.0, 6.0]).show(ui, |ui| {
        for row in &rows {
            let a = row.action;
            let name = label(a);
            ui.label(&name);
            if vm.recording == Some(a) {
                pill(ui, &format!("Press {}+key…  Esc cancels", cmd().trim_end_matches('+')), accent());
            } else {
                match &row.binding {
                    Some(b) => ui.label(RichText::new(combo(b)).monospace()),
                    None => ui.label(muted("None")),
                };
            }
            ui.horizontal(|ui| {
                if vm.recording == Some(a) {
                    if icon_button(ui, ph::X, "Cancel", true).clicked() {
                        out.push(KeymapIntent::Cancel);
                    }
                } else {
                    let change = icon_button(ui, ph::KEYBOARD, "Change", true);
                    // every row has a Change button, so the spoken name says which
                    change.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Change the {name} shortcut")));
                    if change.clicked() {
                        out.push(KeymapIntent::Record(a));
                    }
                }
                if row.binding.is_some() && icon_only(ui, ph::PROHIBIT, &format!("Turn off the {name} shortcut")).clicked() {
                    out.push(KeymapIntent::Clear(a));
                }
                if row.custom {
                    let default = default_binding(a).map_or_else(|| "none".into(), |b| combo(&b));
                    if icon_only(ui, ph::ARROW_COUNTER_CLOCKWISE, &format!("Reset {name} to {default}")).clicked() {
                        out.push(KeymapIntent::Reset(a));
                    }
                }
            });
            ui.end_row();
            if let Some((ea, e)) = vm.error
                && ea == a
            {
                ui.label("");
                ui.label(RichText::new(message(e)).color(danger()));
                ui.end_row();
            }
        }
    });
    ui.add_space(10.0);
    if icon_button(ui, ph::ARROW_COUNTER_CLOCKWISE, "Reset all to defaults", rows.iter().any(|r| r.custom)).clicked() {
        out.push(KeymapIntent::ResetAll);
    }
    out
}
