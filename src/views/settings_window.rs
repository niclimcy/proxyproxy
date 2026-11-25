use crate::viewmodels::intent::{KeymapIntent, NetworkIntent, SchemasIntent};
use crate::viewmodels::keymap_vm::KeymapVm;
use crate::viewmodels::network_vm::NetworkVm;
use crate::viewmodels::schemas_vm::SchemasVm;
use crate::viewmodels::settings_vm::{SettingsTab, SettingsVm};
use crate::views::theme::*;
use crate::views::{network_tab, schemas_tab, shortcuts_tab};
use eframe::egui::{self, Frame, Margin, ScrollArea};

pub fn show(
    ctx: &egui::Context,
    vm: &mut SettingsVm,
    network: &NetworkVm,
    schemas: &SchemasVm,
    keymap: &KeymapVm,
) -> (Vec<NetworkIntent>, Vec<SchemasIntent>, Vec<KeymapIntent>) {
    let (mut net, mut sch, mut keys) = (vec![], vec![], vec![]);
    if !vm.show {
        return (net, sch, keys);
    }
    let builder = egui::ViewportBuilder::default()
        .with_title("Settings")
        .with_inner_size([620.0, 560.0])
        .with_min_inner_size([480.0, 360.0]);
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("settings"),
        builder,
        |ui, _| {
            // while a shortcut is being recorded, Cmd+W is a combo to record (and get rejected), not close
            let recording = keymap.recording.is_some();
            let close = ui.input(|i| {
                (!recording && i.modifiers.command && i.key_pressed(egui::Key::W))
                    || i.viewport().close_requested()
            });
            if close {
                vm.show = false;
            }
            egui::Panel::top(egui::Id::unique("settings-tabs"))
                .frame(panel_frame().inner_margin(Margin::symmetric(14, 10)))
                .show(ui, |ui| {
                    segmented(
                        ui,
                        &mut vm.tab,
                        &[
                            (SettingsTab::Network, "Network"),
                            (SettingsTab::Schemas, "Schemas"),
                            (SettingsTab::Shortcuts, "Shortcuts"),
                        ],
                    );
                });
            egui::CentralPanel::default()
                .frame(Frame::new().fill(canvas()).inner_margin(Margin::same(PAD)))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match vm.tab {
                            SettingsTab::Network => net = network_tab::show(ui, network),
                            SettingsTab::Schemas => sch = schemas_tab::show(ui, schemas),
                            SettingsTab::Shortcuts => keys = shortcuts_tab::show(ui, keymap),
                        });
                });
        },
    );
    (net, sch, keys)
}
