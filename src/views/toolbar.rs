use crate::domain::filter;
use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::intent::{DeviceAction, SessionIntent, ToolbarIntent};
use crate::views::theme::*;
use eframe::egui::{self, Margin, RichText, TextEdit};
use egui_phosphor::regular as ph;

pub struct Props<'a> {
    pub port: u16,
    pub cert: &'a str,
    pub ios_simulators: &'a [crate::services::devices::IosSimulator],
    pub system_proxy_enabled: bool,
    pub show_rules: &'a mut bool,
    pub active_rules: usize,
    pub status: &'a str,
    pub api_port: u16,
    pub paused_breakpoints: usize,
    pub show_help: bool,
}

pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &mut CaptureVm,
    p: Props,
) -> Vec<ToolbarIntent> {
    let mut out = vec![];
    let Props {
        port,
        cert,
        ios_simulators,
        system_proxy_enabled,
        show_rules,
        active_rules,
        status,
        api_port,
        paused_breakpoints,
        show_help,
    } = p;
    egui::Panel::top("top").frame(panel_frame().inner_margin(Margin::symmetric(14, 8))).show(ui, |ui| {
        let compact = ui.available_width() < 900.0;
        if compact {
            compact_primary_rows(ui, ctx, vm, port, cert, ios_simulators, system_proxy_enabled, show_rules, active_rules, status, api_port, paused_breakpoints, show_help, &mut out);
        } else {
            ui.horizontal(|ui| {
            if sidebar_toggle(ui, vm.show_sidebar).clicked() {
                vm.show_sidebar = !vm.show_sidebar;
            }
            let paused = vm.capture_paused();
            let (icon, label, state, tip) = if paused {
                (ph::PAUSE_CIRCLE, "Paused".to_string(), warning(), format!("Capture paused. Click to resume (proxy on 127.0.0.1:{port})"))
            } else {
                (ph::RECORD, format!("Capturing · :{port}"), success(), format!("Capturing on 127.0.0.1:{port}. Click to pause"))
            };
            let pill_resp = flat(ui, |ui| ui.add(egui::Button::new(RichText::new(ico(icon, &label)).size(FS_SMALL).color(state)).fill(tint(state, Tint::Normal)).stroke(egui::Stroke::NONE).min_size(egui::vec2(0.0, HIT)).corner_radius(RADIUS_PILL)));
            if pill_resp.on_hover_text(tip).clicked() {
                out.push(ToolbarIntent::ToggleCapture);
            }
            ui.add_space(8.0);
            let search = ui.add(
                TextEdit::singleline(&mut vm.filter)
                    .id(egui::Id::unique("search"))
                    .hint_text(ico(ph::MAGNIFYING_GLASS, &format!("Search or filter  ({}F)", cmd())))
                .desired_width(420.0)
                    .margin(Margin::symmetric(12, 7))
                    .background_color(canvas()),
            );
            if vm.focus_search {
                search.request_focus();
                vm.focus_search = false;
            }
            search.on_hover_text(format!("Example: host:api.* status:>=400 method:POST body:\"userId\"\nFields: {}\nOperators: -negate  =exact  *wildcard  /regex/  >=num\nis: pinned ws error modified pending json", filter::FIELDS.join(" ")));
            saved_menu(ui, vm, &mut out);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if active_rules > 0 { format!("Rules ({active_rules})") } else { "Rules".into() };
                let tip = format!("{} rules panel", if *show_rules { "Hide" } else { "Show" });
                if right_panel_toggle(ui, *show_rules, &label, tip).clicked() {
                    *show_rules = !*show_rules;
                }
                capsule(ui, |ui| {
                    // right_to_left parent: add in reverse visual order
                    if show_help {
                        toolbar_menu(ui, ph::QUESTION, "Help", |ui| {
                            menu_width(ui, &["Setup guide", "About proxyproxy"]);
                            if menu_item(ui, "Setup guide").clicked() {
                                out.push(ToolbarIntent::Onboarding);
                                ui.close();
                            }
                            if menu_item(ui, "About proxyproxy").clicked() {
                                out.push(ToolbarIntent::About);
                                ui.close();
                            }
                        });
                    }
                    devices_menu(ui, ctx, port, api_port, cert, ios_simulators, system_proxy_enabled, &mut out, false);
                    session_menu(ui, &mut out, false);
                    if icon_button(ui, ph::PLUS, "Compose", true).clicked() {
                        out.push(ToolbarIntent::Compose);
                    }
                    if icon_button(ui, ph::TRASH, "Clear", true).on_hover_text("Remove all requests (including pinned)").clicked() {
                        out.push(ToolbarIntent::ClearAll);
                    }
                });
                // remaining space between the search box and the actions
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if paused_breakpoints > 0 {
                        pill(ui, &format!("{paused_breakpoints} paused"), warning());
                    }
                    if !status.is_empty() {
                        ui.add(egui::Label::new(RichText::new(status).size(FS_SMALL).color(fg_muted())).truncate()).on_hover_text(status);
                    }
                });
            });
        });
        }
        if compact {
            compact_filter_rows(ui, vm, &mut out);
        } else {
            ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (label, tok, c) in [("Errors", "is:error", danger()), ("Pinned", "is:pinned", warning()), ("JSON", "is:json", success()), ("WebSocket", "is:ws", violet()), ("Modified", "is:modified", info()), ("Pending", "is:pending", fg_muted())] {
                if chip(ui, label, vm.token_on(tok), c).clicked() {
                    out.push(ToolbarIntent::ToggleToken(tok));
                }
            }
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(e) = &vm.query.error {
                ui.label(RichText::new(format!("{} {e}", ph::WARNING)).color(danger()).size(FS_CAPTION));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                checkbox(ui, &mut vm.follow, "Follow");
                let hidden = vm.hidden_cols();
                toolbar_menu(ui, ph::COLUMNS, "Columns", |ui| {
                    for c in ALL_COLS.iter().skip(1) {
                        let mut on = !hidden.iter().any(|h| h == c);
                        if checkbox(ui, &mut on, c).changed() {
                            out.push(ToolbarIntent::SetColumn(c, on));
                        }
                    }
                });
                ui.label(muted(format!("{} of {}", vm.visible.len(), vm.flow_count())));
            });
            });
        }
    });
    out
}

fn compact_filter_rows(ui: &mut egui::Ui, vm: &mut CaptureVm, out: &mut Vec<ToolbarIntent>) {
    ui.horizontal(|ui| {
        let controls_width = 250.0;
        let filters_width = (ui.available_width() - controls_width).max(180.0);
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .max_width(filters_width)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (label, tok, c) in [
                    ("Errors", "is:error", danger()),
                    ("Pinned", "is:pinned", warning()),
                    ("JSON", "is:json", success()),
                    ("WebSocket", "is:ws", violet()),
                    ("Modified", "is:modified", info()),
                    ("Pending", "is:pending", fg_muted()),
                ] {
                    if chip(ui, label, vm.token_on(tok), c).clicked() {
                        out.push(ToolbarIntent::ToggleToken(tok));
                    }
                }
                if let Some(e) = &vm.query.error {
                    ui.label(
                        RichText::new(format!("{} {e}", ph::WARNING))
                            .color(danger())
                            .size(FS_CAPTION),
                    );
                }
            });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            checkbox(ui, &mut vm.follow, "Follow");
            toolbar_menu(ui, ph::COLUMNS, "Columns", |ui| {
                for c in ALL_COLS.iter().skip(1) {
                    let mut on = !vm.hidden_cols().iter().any(|h| h == c);
                    if checkbox(ui, &mut on, c).changed() {
                        out.push(ToolbarIntent::SetColumn(c, on));
                    }
                }
            });
            ui.label(muted(format!(
                "{} of {}",
                vm.visible.len(),
                vm.flow_count()
            )));
        });
    });
}

fn compact_primary_rows(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &mut CaptureVm,
    port: u16,
    cert: &str,
    ios_simulators: &[crate::services::devices::IosSimulator],
    system_proxy_enabled: bool,
    show_rules: &mut bool,
    active_rules: usize,
    status: &str,
    api_port: u16,
    paused_breakpoints: usize,
    show_help: bool,
    out: &mut Vec<ToolbarIntent>,
) {
    ui.horizontal(|ui| {
        if sidebar_toggle(ui, vm.show_sidebar).clicked() {
            vm.show_sidebar = !vm.show_sidebar;
        }
        let paused = vm.capture_paused();
        let (icon, label, state, tip) = if paused {
            (ph::PAUSE_CIRCLE, "Paused".to_string(), warning(), format!("Capture paused. Click to resume (proxy on 127.0.0.1:{port})"))
        } else {
            (ph::RECORD, format!("Capturing · :{port}"), success(), format!("Capturing on 127.0.0.1:{port}. Click to pause"))
        };
        let pill_resp = flat(ui, |ui| ui.add(egui::Button::new(RichText::new(ico(icon, &label)).size(FS_SMALL).color(state)).fill(tint(state, Tint::Normal)).stroke(egui::Stroke::NONE).min_size(egui::vec2(0.0, HIT)).corner_radius(RADIUS_PILL)));
        if pill_resp.on_hover_text(tip).clicked() {
            out.push(ToolbarIntent::ToggleCapture);
        }
        let search_width = (ui.available_width() - 8.0).max(180.0);
        let search = ui.add(
            TextEdit::singleline(&mut vm.filter)
                .id(egui::Id::unique("search-compact"))
                .hint_text(ico(ph::MAGNIFYING_GLASS, &format!("Search or filter  ({}F)", cmd())))
                .desired_width(search_width)
                .margin(Margin::symmetric(12, 7))
                .background_color(canvas()),
        );
        if vm.focus_search {
            search.request_focus();
            vm.focus_search = false;
        }
        search.on_hover_text(format!("Example: host:api.* status:>=400 method:POST body:\"userId\"\nFields: {}\nOperators: -negate  =exact  *wildcard  /regex/  >=num\nis: pinned ws error modified pending json", filter::FIELDS.join(" ")));
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        saved_menu(ui, vm, out);
        if icon_button(ui, ph::TRASH, "Clear", true)
            .on_hover_text("Remove all requests (including pinned)")
            .clicked()
        {
            out.push(ToolbarIntent::ClearAll);
        }
        if icon_button(ui, ph::PLUS, "Compose", true).clicked() {
            out.push(ToolbarIntent::Compose);
        }
        session_menu(ui, out, true);
        devices_menu(
            ui,
            ctx,
            port,
            api_port,
            cert,
            ios_simulators,
            system_proxy_enabled,
            out,
            true,
        );
        if show_help {
            toolbar_menu(ui, ph::QUESTION, "", |ui| {
                menu_width(ui, &["Setup guide", "About proxyproxy"]);
                if menu_item(ui, "Setup guide").clicked() {
                    out.push(ToolbarIntent::Onboarding);
                    ui.close();
                }
                if menu_item(ui, "About proxyproxy").clicked() {
                    out.push(ToolbarIntent::About);
                    ui.close();
                }
            });
        }
        if paused_breakpoints > 0 {
            pill(ui, &format!("{paused_breakpoints} paused"), warning());
        }
        if !status.is_empty() {
            ui.add(
                egui::Label::new(RichText::new(status).size(FS_SMALL).color(fg_muted())).truncate(),
            )
            .on_hover_text(status);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if active_rules > 0 {
                format!("Rules ({active_rules})")
            } else {
                "Rules".into()
            };
            if right_panel_toggle(
                ui,
                *show_rules,
                &label,
                format!("{} rules panel", if *show_rules { "Hide" } else { "Show" }),
            )
            .clicked()
            {
                *show_rules = !*show_rules;
            }
        });
    });
}

fn saved_menu(ui: &mut egui::Ui, vm: &CaptureVm, out: &mut Vec<ToolbarIntent>) {
    let menu = flat(ui, |ui| {
        ui.menu_button(RichText::new(ph::STAR).size(FS_BODY), |ui| {
            if !vm.filter.trim().is_empty()
                && ui.button(ico(ph::PLUS, "Save current query")).clicked()
            {
                out.push(ToolbarIntent::SaveQuery);
                ui.close();
                return;
            }
            let saved = vm.saved_queries();
            if saved.is_empty() {
                ui.label(muted("No saved queries"));
            }
            for (i, (name, q)) in saved.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button(name).on_hover_text(q).clicked() {
                        out.push(ToolbarIntent::LoadQuery(q.clone()));
                        ui.close();
                    }
                    if ui.small_button(ph::X).clicked() {
                        out.push(ToolbarIntent::DeleteQuery(i));
                    }
                });
            }
        })
    });
    menu.response.on_hover_text("Saved queries");
}

fn session_menu(ui: &mut egui::Ui, out: &mut Vec<ToolbarIntent>, compact: bool) {
    toolbar_menu(
        ui,
        ph::FOLDER_OPEN,
        if compact { "" } else { "Session" },
        |ui| {
            menu_width(
                ui,
                &[
                    "New session",
                    "Open session…",
                    "Save session as…",
                    "Import HAR / Charles JSON…",
                    "Export HAR (visible)",
                    "Export CSV (visible)",
                    "Export rules…",
                    "Import rules…",
                    "Clear unpinned requests",
                ],
            );
            let mut act = None::<u8>;
            for (i, l) in [
                "New session",
                "Open session…",
                "Save session as…",
                "-",
                "Import HAR / Charles JSON…",
                "-",
                "Export HAR (visible)",
                "Export CSV (visible)",
                "Export rules…",
                "Import rules…",
                "-",
                "Clear unpinned requests",
            ]
            .iter()
            .enumerate()
            {
                if *l == "-" {
                    ui.separator();
                } else if menu_item(ui, l).clicked() {
                    act = Some(i as u8);
                    ui.close();
                }
            }
            let Some(a) = act else { return };
            let dlg = rfd::FileDialog::new;
            let intent = match a {
                0 => Some(SessionIntent::New),
                1 => dlg()
                    .add_filter("proxyproxy session", &["pxp"])
                    .pick_file()
                    .map(SessionIntent::Open),
                2 => dlg()
                    .add_filter("proxyproxy session", &["pxp"])
                    .set_file_name("session.pxp")
                    .save_file()
                    .map(SessionIntent::SaveAs),
                4 => dlg()
                    .add_filter("HAR / Charles JSON", &["har", "json", "chlsj"])
                    .pick_file()
                    .map(SessionIntent::ImportFlows),
                6 => dlg()
                    .set_file_name("export.har")
                    .save_file()
                    .map(SessionIntent::ExportHar),
                7 => dlg()
                    .set_file_name("export.csv")
                    .save_file()
                    .map(SessionIntent::ExportCsv),
                8 => dlg()
                    .set_file_name("proxyproxy-rules.json")
                    .save_file()
                    .map(SessionIntent::ExportRules),
                9 => dlg()
                    .add_filter("rules", &["json"])
                    .pick_file()
                    .map(SessionIntent::ImportRules),
                11 => {
                    out.push(ToolbarIntent::ClearUnpinned);
                    None
                }
                _ => None,
            };
            if let Some(i) = intent {
                out.push(ToolbarIntent::Session(i));
            }
        },
    );
}

fn devices_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    port: u16,
    api_port: u16,
    cert: &str,
    ios_simulators: &[crate::services::devices::IosSimulator],
    system_proxy_enabled: bool,
    out: &mut Vec<ToolbarIntent>,
    compact: bool,
) {
    toolbar_menu(
        ui,
        ph::DEVICE_MOBILE,
        if compact { "" } else { "Capture" },
        |ui| {
            menu_width(
                ui,
                &[
                    "This computer",
                    "Trust CA on this computer",
                    "Set system proxy",
                    "Clear system proxy",
                    "Launch Chrome with proxy (clean profile)",
                    "Terminal & scripts",
                    "Copy `proxyproxy run -- <cmd>` hint",
                    "Copy shell env snippet",
                    "Devices",
                    "iOS Simulator: install CA",
                    "Android: set proxy (adb)",
                    "Android: clear proxy",
                    "Android: push CA",
                    "Copy CA path",
                    "Copy REST API URL (127.0.0.1:00000)",
                ],
            );
            let item = |ui: &mut egui::Ui, label: &str| {
                let c = menu_item(ui, label).clicked();
                if c {
                    ui.close();
                }
                c
            };
            let mut device = |ui: &mut egui::Ui, label: &str, a: DeviceAction| {
                if item(ui, label) {
                    out.push(ToolbarIntent::Device(a));
                }
            };
            ui.label(muted("This computer"));
            device(ui, "Trust CA on this computer", DeviceAction::TrustCa);
            if system_proxy_enabled {
                ui.label(muted("System capture: enabled"));
                device(ui, "Disable system proxy", DeviceAction::ClearSystemProxy);
            } else {
                ui.label(muted("System capture: disabled"));
                device(ui, "Enable system proxy", DeviceAction::SetSystemProxy);
            }
            device(
                ui,
                "Launch Chrome with proxy (clean profile)",
                DeviceAction::LaunchChrome,
            );
            ui.separator();
            ui.label(muted("Terminal & scripts"));
            if item(ui, "Copy `proxyproxy run -- <cmd>` hint") {
                ctx.copy_text(format!("proxyproxy run --port {port} -- "));
            }
            if item(ui, "Copy shell env snippet") {
                ctx.copy_text(crate::services::devices::env_snippet(port, cert));
            }
            ui.separator();
            ui.label(muted("Devices"));
            ui.label(muted("iOS Simulators"));
            let running: Vec<_> = ios_simulators
                .iter()
                .filter(|sim| sim.state == crate::services::devices::IosSimulatorState::Booted)
                .collect();
            if running.is_empty() {
                ui.label(muted("No running iOS simulator"));
            } else {
                for sim in running {
                    let ca = match sim.ca_status {
                        crate::services::devices::IosCaStatus::Installed => "CA installed",
                        crate::services::devices::IosCaStatus::Missing => "CA missing",
                        crate::services::devices::IosCaStatus::Unknown => "CA status unknown",
                    };
                    ui.label(muted(format!("{} · {}", sim.name, ca)));
                    if sim.ca_status != crate::services::devices::IosCaStatus::Installed {
                        device(
                            ui,
                            &format!("Install CA on {}", sim.name),
                            DeviceAction::IosSimInstallCa(sim.udid.clone()),
                        );
                    }
                    if sim.ca_status == crate::services::devices::IosCaStatus::Installed {
                        device(
                            ui,
                            &format!("Reboot {}", sim.name),
                            DeviceAction::IosSimReboot(sim.udid.clone()),
                        );
                    }
                }
            }
            device(
                ui,
                "Android: set proxy (adb)",
                DeviceAction::AndroidSetProxy,
            );
            device(ui, "Android: clear proxy", DeviceAction::AndroidClearProxy);
            device(ui, "Android: push CA", DeviceAction::AndroidInstallCa);
            ui.separator();
            if item(ui, "Copy CA path") {
                ctx.copy_text(cert.to_string());
            }
            if item(ui, &format!("Copy REST API URL (127.0.0.1:{api_port})")) {
                ctx.copy_text(format!("http://127.0.0.1:{api_port}"));
            }
        },
    );
}
