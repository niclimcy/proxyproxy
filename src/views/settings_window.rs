use crate::viewmodels::intent::{NetworkIntent, SchemasIntent};
use crate::viewmodels::network_vm::NetworkVm;
use crate::viewmodels::schemas_vm::SchemasVm;
use crate::viewmodels::settings_vm::{SettingsTab, SettingsVm};
use crate::views::theme::*;
use crate::views::{network_tab, schemas_tab};
use eframe::egui::{self, Frame, Margin, ScrollArea};

pub fn show(
    ctx: &egui::Context,
    vm: &mut SettingsVm,
    network: &NetworkVm,
    schemas: &SchemasVm,
) -> (Vec<NetworkIntent>, Vec<SchemasIntent>) {
    let (mut net, mut sch) = (vec![], vec![]);
    if !vm.show {
        return (net, sch);
    }
    let builder = egui::ViewportBuilder::default()
        .with_title("Settings")
        .with_inner_size([620.0, 560.0])
        .with_min_inner_size([480.0, 360.0]);
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("settings"),
        builder,
        |ui, _| {
            let close = ui.input(|i| {
                (i.modifiers.command && i.key_pressed(egui::Key::W))
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
                        });
                });
        },
    );
    (net, sch)
}
