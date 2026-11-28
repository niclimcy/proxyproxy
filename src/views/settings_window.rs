use crate::viewmodels::intent::{KeymapIntent, McpIntent, NetworkIntent, SchemasIntent};
use crate::viewmodels::keymap_vm::KeymapVm;
use crate::viewmodels::mcp_vm::McpVm;
use crate::viewmodels::network_vm::NetworkVm;
use crate::viewmodels::schemas_vm::SchemasVm;
use crate::viewmodels::settings_vm::{SettingsTab, SettingsVm};
use crate::views::theme::{bar_frame, pane, segmented, window_frame};
use crate::views::{mcp_tab, network_tab, schemas_tab, shortcuts_tab};
use eframe::egui::{self, ScrollArea};

pub fn show(
    ctx: &egui::Context,
    vm: &mut SettingsVm,
    network: &NetworkVm,
    schemas: &SchemasVm,
    keymap: &KeymapVm,
    mcp_vm: &McpVm,
) -> (
    Vec<NetworkIntent>,
    Vec<SchemasIntent>,
    Vec<KeymapIntent>,
    Vec<McpIntent>,
) {
    let (mut net, mut sch, mut keys, mut mcp) = (vec![], vec![], vec![], vec![]);
    if !vm.show {
        return (net, sch, keys, mcp);
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
            egui::CentralPanel::default()
                .frame(window_frame())
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    bar_frame().show(ui, |ui| {
                        segmented(
                            ui,
                            &mut vm.tab,
                            &[
                                (SettingsTab::Network, "Network"),
                                (SettingsTab::Schemas, "Schemas"),
                                (SettingsTab::Shortcuts, "Shortcuts"),
                                (SettingsTab::Mcp, "MCP"),
                            ],
                        );
                    });
                    pane(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| match vm.tab {
                                SettingsTab::Network => net = network_tab::show(ui, network),
                                SettingsTab::Mcp => mcp = mcp_tab::show(ui, mcp_vm),
                                SettingsTab::Schemas => sch = schemas_tab::show(ui, schemas),
                                SettingsTab::Shortcuts => keys = shortcuts_tab::show(ui, keymap),
                            });
                    });
                });
        },
    );
    (net, sch, keys, mcp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::{Config, UpstreamMode};
    use crate::views::layout_test::*;
    use std::sync::{Arc, Mutex};

    /// Room the tab content has in the narrowest settings window: 480 px less its frames.
    const MIN_CONTENT: f32 = 480.0 - 44.0;

    fn config(mode: UpstreamMode) -> Config {
        let mut c = Config {
            dns_overrides: vec![(HOST.into(), "255.255.255.255-not-an-ip".into())],
            proto_descs: vec![FILE.into(), format!("{FILE}.missing")],
            keylog_path: FILE.into(),
            ..Default::default()
        };
        c.upstream.mode = mode;
        c.upstream.url = url();
        c.upstream.pac_path = FILE.into();
        c
    }

    #[test]
    fn every_tab_fits_the_narrowest_window() {
        let ctx = ctx();
        let mut leaks = vec![];
        for mode in [
            UpstreamMode::Off,
            UpstreamMode::Manual,
            UpstreamMode::Socks,
            UpstreamMode::Pac,
        ] {
            let cfg = Arc::new(Mutex::new(config(mode)));
            let network = NetworkVm::new(cfg.clone());
            let used = used_width(&ctx, MIN_CONTENT, |ui| {
                network_tab::show(ui, &network);
            });
            if used > MIN_CONTENT + 1.0 {
                leaks.push(format!("Network ({mode:?}) needs {used} px"));
            }
        }
        let cfg = Arc::new(Mutex::new(config(UpstreamMode::Off)));
        let schemas = SchemasVm::new(cfg.clone());
        schemas.sync();
        let keymap = KeymapVm::new(cfg.clone());
        let mcp = McpVm::new(cfg, 8080, 8081, Some(FILE));
        let tabs: [(&str, f32); 3] = [
            (
                "Schemas",
                used_width(&ctx, MIN_CONTENT, |ui| {
                    schemas_tab::show(ui, &schemas);
                }),
            ),
            (
                "Shortcuts",
                used_width(&ctx, MIN_CONTENT, |ui| {
                    shortcuts_tab::show(ui, &keymap);
                }),
            ),
            (
                "MCP",
                used_width(&ctx, MIN_CONTENT, |ui| {
                    mcp_tab::show(ui, &mcp);
                }),
            ),
        ];
        for (name, used) in tabs {
            if used > MIN_CONTENT + 1.0 {
                leaks.push(format!("{name} needs {used} px"));
            }
        }
        assert!(
            leaks.is_empty(),
            "of {MIN_CONTENT} px: {}",
            leaks.join(", ")
        );
    }
}
