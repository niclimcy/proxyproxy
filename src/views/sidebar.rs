use crate::domain::config::Config;
use crate::viewmodels::capture_vm::CaptureVm;
use crate::viewmodels::intent::SidebarIntent;
use crate::views::theme::*;
use eframe::egui::{self, Margin, RichText, ScrollArea};
use egui_phosphor::regular as ph;

pub fn show(ui: &mut egui::Ui, vm: &mut CaptureVm) -> Vec<SidebarIntent> {
    let mut out = vec![];
    let mut open = vm.show_sidebar;
    let cfg = vm.config_snapshot();
    let vm_ref = &*vm;
    egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(230.0)
        .frame(panel_frame().inner_margin(Margin::same(PAD)))
        .show_collapsible(ui, &mut open, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(overline("CAPTURE"));
                    // Pick toggles: re-picking the active filter clears it
                    let all: usize = vm_ref
                        .tree
                        .iter()
                        .flat_map(|(_, h)| h.iter().map(|(_, n)| n))
                        .sum();
                    if list_row(
                        ui,
                        Some(ph::LIST),
                        "All requests",
                        Some(all),
                        vm_ref.filter.trim().is_empty(),
                        false,
                    )
                    .clicked()
                        && !vm_ref.filter.trim().is_empty()
                    {
                        out.push(SidebarIntent::Pick(vm_ref.filter.trim().to_string()));
                    }
                    if cfg.scope_only {
                        ui.label(
                            RichText::new(format!(
                                "{} Scope: {}",
                                ph::TARGET,
                                if cfg.scope.is_empty() {
                                    "empty — capturing all".into()
                                } else {
                                    cfg.scope.join(", ")
                                }
                            ))
                            .size(FS_CAPTION)
                            .color(accent()),
                        );
                    }
                    ui.add_space(8.0);
                    ui.label(overline("APPS"));
                    if vm_ref.tree.is_empty() {
                        ui.label(muted("Nothing captured yet"));
                    }
                    for (app, hosts) in &vm_ref.tree {
                        let total: usize = hosts.iter().map(|(_, n)| n).sum();
                        let app_on = vm_ref.filter.trim() == format!("app:{app}");
                        let open_default = vm_ref.tree.len() <= 3;
                        let id = ui.make_persistent_id(("app", app));
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                id,
                                open_default,
                            );
                        let head = list_row(
                            ui,
                            Some(if state.is_open() {
                                ph::FOLDER_OPEN
                            } else {
                                ph::FOLDER
                            }),
                            app,
                            Some(total),
                            app_on,
                            false,
                        );
                        if head.clicked() {
                            state.toggle(ui);
                            out.push(SidebarIntent::Pick(format!("app:{app}")));
                        }
                        state.show_body_unindented(ui, |ui| {
                            ui.indent(("hosts", app), |ui| {
                                for (h, n) in hosts {
                                    let on = vm_ref.filter.trim() == format!("host:{h}");
                                    let muted_h = Config::host_in(&cfg.muted, h);
                                    let resp = list_row(ui, None, h, Some(*n), on, muted_h);
                                    if resp.clicked() {
                                        out.push(SidebarIntent::Pick(format!("host:{h}")));
                                    }
                                    resp.context_menu(|ui| {
                                        let items = [
                                            ico(ph::EYE_SLASH, "Mute (hide from record)"),
                                            ico(ph::TARGET, "Add to capture scope"),
                                            ico(ph::LOCK_OPEN, "Don't decrypt TLS"),
                                            ico(ph::COPY, "Copy host"),
                                        ];
                                        let labels: Vec<&str> =
                                            items.iter().map(String::as_str).collect();
                                        menu_width(ui, &labels);
                                        for (i, label) in labels.iter().enumerate() {
                                            if !menu_item(ui, label).clicked() {
                                                continue;
                                            }
                                            match i {
                                                0 => out.push(SidebarIntent::Mute(h.clone())),
                                                1 => out.push(SidebarIntent::Scope(h.clone())),
                                                2 => out.push(SidebarIntent::NoTls(h.clone())),
                                                _ => ui.ctx().copy_text(h.clone()),
                                            }
                                            ui.close();
                                        }
                                    });
                                }
                            });
                        });
                        head.context_menu(|ui| {
                            menu_width(ui, &["Show only this app"]);
                            if menu_item(ui, "Show only this app").clicked() {
                                out.push(SidebarIntent::Pick(format!("app:{app}")));
                                ui.close();
                            }
                        });
                    }
                    if !cfg.muted.is_empty() {
                        ui.add_space(8.0);
                        ui.label(overline("MUTED"));
                        for m in &cfg.muted {
                            ui.horizontal(|ui| {
                                ui.label(muted(m));
                                if icon_button(ui, ph::X, "", true)
                                    .on_hover_text("Unmute")
                                    .clicked()
                                {
                                    out.push(SidebarIntent::Unmute(m.clone()));
                                }
                            });
                        }
                    }
                });
        });
    vm.show_sidebar = open;
    out
}
