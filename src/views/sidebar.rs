use crate::domain::config::Config;
use crate::viewmodels::capture_vm::CaptureVm;
use crate::viewmodels::intent::SidebarIntent;
use crate::views::app_icon::{paint_fallback_icon, paint_icon};
use crate::views::theme::{
    FS_CAPTION, RowIcon, accent, card_panel, ico, icon_only, list_row, menu_item, menu_width,
    muted, overline, resize_grip, resize_line,
};
use eframe::egui::{self, RichText, ScrollArea};
use egui_phosphor::regular as ph;

pub fn show(ui: &mut egui::Ui, vm: &mut CaptureVm) -> Vec<SidebarIntent> {
    let mut out = vec![];
    let mut open = vm.show_sidebar;
    if vm.tree_pending > 0 {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
    let cfg = vm.config_snapshot();
    let vm_ref = &*vm;
    resize_line(ui, true);
    let shown = egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(230.0)
        .show_separator_line(false)
        .frame(card_panel())
        .show_collapsible(ui, &mut open, |ui| {
            resize_line(ui, false);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(overline("CAPTURE"));
                    // Pick toggles: re-picking the active filter clears it
                    let all: usize = vm_ref
                        .tree
                        .iter()
                        .chain(vm_ref.platforms.iter().flat_map(|(_, t)| t))
                        .flat_map(|(_, h)| h.iter().map(|(_, n)| n))
                        .sum::<usize>()
                        + vm_ref.tree_pending;
                    if list_row(
                        ui,
                        Some(RowIcon::Glyph(ph::LIST)),
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
                    if vm_ref.tree.is_empty() && vm_ref.platforms.is_empty() {
                        ui.label(muted("Nothing captured yet"));
                    }
                    let folders = vm_ref.platforms.len() + usize::from(!vm_ref.tree.is_empty());
                    for (app, hosts) in &vm_ref.tree {
                        app_node(
                            ui,
                            vm_ref,
                            &cfg,
                            &mut out,
                            "",
                            app,
                            hosts,
                            folders <= 1 && vm_ref.tree.len() <= 3,
                        );
                    }
                    for (platform, apps) in &vm_ref.platforms {
                        let total: usize = apps
                            .iter()
                            .flat_map(|(_, h)| h.iter().map(|(_, n)| n))
                            .sum();
                        let id = ui.make_persistent_id(("platform", platform));
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                id,
                                true,
                            );
                        let term = filter_term("platform", platform);
                        let on = vm_ref.filter.trim() == term;
                        let head = list_row(
                            ui,
                            Some(RowIcon::Glyph(ph::DEVICE_MOBILE)),
                            platform,
                            Some(total),
                            on,
                            false,
                        );
                        if head.clicked() {
                            state.toggle(ui);
                            out.push(SidebarIntent::Pick(term));
                        }
                        state.show_body_unindented(ui, |ui| {
                            ui.indent(("apps", platform), |ui| {
                                for (app, hosts) in apps {
                                    app_node(
                                        ui, vm_ref, &cfg, &mut out, platform, app, hosts, false,
                                    );
                                }
                            });
                        });
                    }
                    if !cfg.muted.is_empty() {
                        ui.add_space(8.0);
                        ui.label(overline("MUTED"));
                        for m in &cfg.muted {
                            ui.horizontal(|ui| {
                                ui.label(muted(m));
                                if icon_only(ui, ph::X, &format!("Unmute {m}")).clicked() {
                                    out.push(SidebarIntent::Unmute(m.clone()));
                                }
                            });
                        }
                    }
                });
        });
    resize_line(ui, false);
    if let Some(r) = shown {
        let r = r.response.rect;
        resize_grip(
            ui,
            "sidebar",
            egui::Rect::from_two_pos(r.right_top(), r.right_bottom()),
        );
    }
    vm.show_sidebar = open;
    out
}

#[allow(clippy::too_many_arguments)]
fn app_node(
    ui: &mut egui::Ui,
    vm: &CaptureVm,
    cfg: &Config,
    out: &mut Vec<SidebarIntent>,
    platform: &str,
    app: &str,
    hosts: &[(String, usize)],
    open_default: bool,
) {
    let total: usize = hosts.iter().map(|(_, n)| n).sum();
    let scope = if platform.is_empty() {
        String::new()
    } else {
        format!("{} ", filter_term("platform", platform))
    };
    let app_term = format!("{scope}{}", filter_term("app", app));
    let app_on = vm.filter.trim() == app_term;
    let id = ui.make_persistent_id(("app", platform, app));
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        open_default,
    );
    let head = list_row(
        ui,
        Some(RowIcon::Paint(&|p, r| {
            if !paint_icon(p, r, &vm.icons, vm.app_ids.get(app)) {
                paint_fallback_icon(p, r, app, vm.app_ids.get(app));
            }
        })),
        app,
        Some(total),
        app_on,
        false,
    );
    if head.clicked() {
        state.toggle(ui);
        out.push(SidebarIntent::Pick(app_term.clone()));
    }
    state.show_body_unindented(ui, |ui| {
        ui.indent(("hosts", platform, app), |ui| {
            for (h, n) in hosts {
                let host_term = format!("{app_term} {}", filter_term("host", h));
                let on = vm.filter.trim() == host_term;
                let muted_h = Config::host_in(&cfg.muted, h);
                let resp = list_row(ui, None, h, Some(*n), on, muted_h);
                if resp.clicked() {
                    out.push(SidebarIntent::Pick(host_term));
                }
                resp.context_menu(|ui| {
                    let items = [
                        ico(ph::EYE_SLASH, "Mute (hide from record)"),
                        ico(ph::TARGET, "Add to capture scope"),
                        ico(ph::LOCK_OPEN, "Don't decrypt TLS"),
                        ico(ph::COPY, "Copy host"),
                    ];
                    let labels: Vec<&str> = items.iter().map(String::as_str).collect();
                    menu_width(ui, &labels);
                    for (i, label) in labels.iter().enumerate() {
                        if !menu_item(ui, label, None).clicked() {
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
        if menu_item(ui, "Show only this app", None).clicked() {
            out.push(SidebarIntent::Pick(app_term));
            ui.close();
        }
    });
}

/// Exact match, quoted so names with spaces stay one term.
fn filter_term(field: &str, value: &str) -> String {
    format!("{field}:=\"{value}\"")
}
