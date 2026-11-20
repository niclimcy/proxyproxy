use crate::domain::config::Config;
use crate::viewmodels::capture_vm::CaptureVm;
use crate::viewmodels::intent::SidebarIntent;
use crate::views::theme::*;
use eframe::egui::{self, Frame, Margin, RichText, ScrollArea, Stroke};
use egui_phosphor::regular as ph;

pub fn show(ui: &mut egui::Ui, vm: &mut CaptureVm) -> Vec<SidebarIntent> {
    let mut out = vec![];
    let mut open = vm.show_sidebar;
    let cfg = vm.config_snapshot();
    let vm_ref = &*vm;
    egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(230.0)
        .frame(Frame::new().fill(PANEL).inner_margin(Margin::same(PAD)).stroke(Stroke::new(1.0, BORDER)))
        .show_collapsible(ui, &mut open, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("APPS & DOMAINS").size(11.0).strong().color(MUTED));
                });
                if cfg.scope_only {
                    ui.label(RichText::new(format!("{} Scope: {}", ph::TARGET, if cfg.scope.is_empty() { "empty — capturing all".into() } else { cfg.scope.join(", ") })).size(11.5).color(ACCENT));
                }
                if vm_ref.tree.is_empty() {
                    ui.label(muted("Nothing captured yet"));
                }
                for (app, hosts) in &vm_ref.tree {
                    let total: usize = hosts.iter().map(|(_, n)| n).sum();
                    let r = egui::CollapsingHeader::new(RichText::new(format!("{app}  ·  {total}")).strong()).id_salt(("app", app)).default_open(vm_ref.tree.len() <= 3).show(ui, |ui| {
                        for (h, n) in hosts {
                            let on = vm_ref.filter.trim() == format!("host:{h}");
                            let muted_h = Config::host_in(&cfg.muted, h);
                            let text = RichText::new(format!("{h}  ({n})")).size(13.0).color(if muted_h { MUTED } else { TEXT });
                            let resp = ui.selectable_label(on, text);
                            if resp.clicked() {
                                out.push(SidebarIntent::Pick(format!("host:{h}")));
                            }
                            resp.context_menu(|ui| {
                                if ui.button(ico(ph::EYE_SLASH, "Mute (hide from record)")).clicked() {
                                    out.push(SidebarIntent::Mute(h.clone()));
                                    ui.close();
                                }
                                if ui.button(ico(ph::TARGET, "Add to capture scope")).clicked() {
                                    out.push(SidebarIntent::Scope(h.clone()));
                                    ui.close();
                                }
                                if ui.button(ico(ph::LOCK_OPEN, "Don't decrypt TLS")).clicked() {
                                    out.push(SidebarIntent::NoTls(h.clone()));
                                    ui.close();
                                }
                                if ui.button(ico(ph::COPY, "Copy host")).clicked() {
                                    ui.ctx().copy_text(h.clone());
                                    ui.close();
                                }
                            });
                        }
                    });
                    r.header_response.context_menu(|ui| {
                        if ui.button("Show only this app").clicked() {
                            out.push(SidebarIntent::Pick(format!("app:{app}")));
                            ui.close();
                        }
                    });
                }
                if !cfg.muted.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("MUTED").size(11.0).strong().color(MUTED));
                    for m in &cfg.muted {
                        ui.horizontal(|ui| {
                            ui.label(muted(m));
                            if ui.small_button(ph::X).on_hover_text("Unmute").clicked() {
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
