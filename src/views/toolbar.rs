use crate::domain::filter;
use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::devices_vm::{DevicesVm, IosCaStatus};
use crate::viewmodels::intent::{DeviceAction, SessionIntent, ToolbarIntent};
use crate::views::theme::*;
use eframe::egui::{self, Margin, RichText, TextEdit};
use egui_phosphor::regular as ph;

pub struct Props<'a> {
    pub port: u16,
    pub cert: &'a str,
    pub devices: &'a DevicesVm,
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
        devices,
        show_rules,
        active_rules,
        status,
        api_port,
        paused_breakpoints,
        show_help,
    } = p;
    let setup = Setup {
        port,
        api_port,
        cert,
        devices,
    };
    egui::Panel::top("top").frame(panel_frame().inner_margin(Margin::symmetric(14, 8))).show(ui, |ui| {
        let compact = ui.available_width() < 900.0;
        if compact {
            let props = Props {
                port,
                cert,
                devices,
                show_rules: &mut *show_rules,
                active_rules,
                status,
                api_port,
                paused_breakpoints,
                show_help,
            };
            compact_primary_rows(ui, ctx, vm, props, &mut out);
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
                        help_menu(ui, "Help", &mut out);
                    }
                    devices_menu(ui, ctx, setup, &mut out, false);
                    session_menu(ui, &mut out, false);
                    if icon_button(ui, ph::TRASH, "Clear", true).on_hover_text("Remove all requests except pinned").clicked() {
                        out.push(ToolbarIntent::ClearUnpinned);
                    }
                    if icon_button(ui, ph::PLUS, "Compose", true).clicked() {
                        out.push(ToolbarIntent::Compose);
                    }
                });
                // remaining space between the search box and the actions
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if paused_breakpoints > 0 && paused_pill(ui, paused_breakpoints).clicked() {
                        out.push(ToolbarIntent::ShowBreakpoint);
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
                if chip(ui, &ico(ph::ARROW_LINE_DOWN, "Follow"), vm.follow, accent())
                    .on_hover_text("Keep the newest request in view")
                    .clicked()
                {
                    vm.follow = !vm.follow;
                }
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
            if chip(ui, &ico(ph::ARROW_LINE_DOWN, "Follow"), vm.follow, accent())
                .on_hover_text("Keep the newest request in view")
                .clicked()
            {
                vm.follow = !vm.follow;
            }
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
    p: Props,
    out: &mut Vec<ToolbarIntent>,
) {
    let Props {
        port,
        cert,
        devices,
        show_rules,
        active_rules,
        status,
        api_port,
        paused_breakpoints,
        show_help,
    } = p;
    let setup = Setup {
        port,
        api_port,
        cert,
        devices,
    };
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
        if icon_button(ui, ph::PLUS, "Compose", true).clicked() {
            out.push(ToolbarIntent::Compose);
        }
        if icon_button(ui, ph::TRASH, "Clear", true)
            .on_hover_text("Remove all requests except pinned")
            .clicked()
        {
            out.push(ToolbarIntent::ClearUnpinned);
        }
        session_menu(ui, out, true);
        devices_menu(ui, ctx, setup, out, true);
        if show_help {
            help_menu(ui, "", out);
        }
        if paused_breakpoints > 0 && paused_pill(ui, paused_breakpoints).clicked() {
            out.push(ToolbarIntent::ShowBreakpoint);
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

fn paused_pill(ui: &mut egui::Ui, n: usize) -> egui::Response {
    flat(ui, |ui| {
        ui.add(
            egui::Button::new(
                RichText::new(ico(ph::PAUSE_CIRCLE, &format!("{n} paused")))
                    .size(FS_SMALL)
                    .color(warning()),
            )
            .fill(tint(warning(), Tint::Normal))
            .min_size(egui::vec2(0.0, HIT))
            .corner_radius(RADIUS_PILL),
        )
    })
    .on_hover_text("Show the breakpoint editor")
}

fn saved_menu(ui: &mut egui::Ui, vm: &CaptureVm, out: &mut Vec<ToolbarIntent>) {
    let menu = flat(ui, |ui| {
        ui.menu_button(RichText::new(ph::STAR).size(FS_BODY), |ui| {
            let saved = vm.saved_queries();
            let save = ico(ph::PLUS, "Save current query");
            let mut labels: Vec<&str> = saved.iter().map(|(n, _)| n.as_str()).collect();
            labels.push(&save);
            menu_width(ui, &labels);
            if !vm.filter.trim().is_empty() && menu_item(ui, &save).clicked() {
                out.push(ToolbarIntent::SaveQuery);
                ui.close();
                return;
            }
            if saved.is_empty() {
                ui.label(muted("No saved queries"));
            }
            for (i, (name, q)) in saved.iter().enumerate() {
                ui.horizontal(|ui| {
                    if icon_button(ui, ph::X, "", true)
                        .on_hover_text("Delete saved query")
                        .clicked()
                    {
                        out.push(ToolbarIntent::DeleteQuery(i));
                    }
                    if menu_item(ui, name).on_hover_text(q).clicked() {
                        out.push(ToolbarIntent::LoadQuery(q.clone()));
                        ui.close();
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
            let items = [
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
                "Clear all, including pinned",
            ];
            menu_width(ui, &items);
            let mut act = None::<u8>;
            for (i, l) in items.iter().enumerate() {
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
                    out.push(ToolbarIntent::ClearAll);
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

#[derive(Clone, Copy)]
struct Setup<'a> {
    port: u16,
    api_port: u16,
    cert: &'a str,
    devices: &'a DevicesVm,
}

fn help_menu(ui: &mut egui::Ui, label: &str, out: &mut Vec<ToolbarIntent>) {
    toolbar_menu(ui, ph::QUESTION, label, |ui| {
        let settings = format!("Settings…  ({},)", cmd());
        menu_width(ui, &[&settings, "Setup guide", "About proxyproxy"]);
        if menu_item(ui, &settings).clicked() {
            out.push(ToolbarIntent::Settings);
            ui.close();
        }
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

enum Row {
    Note(String),
    Separator,
    Device(String, DeviceAction),
    Copy(String, String),
}

fn device_rows(s: Setup) -> Vec<Row> {
    let (port, api_port) = (s.port, s.api_port);
    let device = |label: &str, action| Row::Device(label.to_string(), action);
    let note = |text: &str| Row::Note(text.to_string());
    let mut rows = vec![
        note("This computer"),
        device("Trust CA on this computer", DeviceAction::TrustCa),
    ];
    if s.devices.system_proxy_enabled {
        rows.push(note("System capture: enabled"));
        rows.push(device(
            "Disable system proxy",
            DeviceAction::ToggleSystemProxy,
        ));
    } else {
        rows.push(note("System capture: disabled"));
        rows.push(device(
            "Enable system proxy",
            DeviceAction::ToggleSystemProxy,
        ));
    }
    rows.extend([
        device(
            "Launch Chrome with proxy (clean profile)",
            DeviceAction::LaunchChrome,
        ),
        Row::Separator,
        note("Terminal & scripts"),
        Row::Copy(
            "Copy `proxyproxy run -- <cmd>` hint".into(),
            format!("proxyproxy run --port {port} -- "),
        ),
        Row::Copy(
            "Copy shell env snippet".into(),
            crate::services::devices::env_snippet(port, s.cert),
        ),
        Row::Separator,
        note("iOS Simulators"),
    ]);
    if s.devices.running().next().is_none() {
        rows.push(note("No running iOS simulator"));
    }
    for sim in s.devices.running() {
        let ca = match sim.ca_status {
            IosCaStatus::Installed => "CA installed",
            IosCaStatus::Missing => "CA missing",
            IosCaStatus::Unknown => "CA status unknown",
        };
        rows.push(note(&format!("{} · {ca}", sim.name)));
        rows.push(if sim.ca_status == IosCaStatus::Installed {
            Row::Device(
                format!("Reboot {}", sim.name),
                DeviceAction::IosSimReboot(sim.udid.clone()),
            )
        } else {
            Row::Device(
                format!("Install CA on {}", sim.name),
                DeviceAction::IosSimInstallCa(sim.udid.clone()),
            )
        });
    }
    rows.extend([
        note("Android"),
        device("Android: set proxy (adb)", DeviceAction::AndroidSetProxy),
        device("Android: clear proxy", DeviceAction::AndroidClearProxy),
        device("Android: push CA", DeviceAction::AndroidInstallCa),
        Row::Separator,
        Row::Copy("Copy CA path".into(), s.cert.to_string()),
        Row::Copy(
            format!("Copy REST API URL (127.0.0.1:{api_port})"),
            format!("http://127.0.0.1:{api_port}"),
        ),
    ]);
    rows
}

fn devices_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    setup: Setup,
    out: &mut Vec<ToolbarIntent>,
    compact: bool,
) {
    toolbar_menu(
        ui,
        ph::DEVICE_MOBILE,
        if compact { "" } else { "Connect" },
        |ui| {
            let rows = device_rows(setup);
            let labels: Vec<&str> = rows
                .iter()
                .filter_map(|row| match row {
                    Row::Note(l) | Row::Device(l, _) | Row::Copy(l, _) => Some(l.as_str()),
                    Row::Separator => None,
                })
                .collect();
            menu_width(ui, &labels);
            for row in rows {
                match row {
                    Row::Note(text) => {
                        ui.label(muted(text));
                    }
                    Row::Separator => {
                        ui.separator();
                    }
                    Row::Device(label, action) => {
                        if menu_item(ui, &label).clicked() {
                            out.push(ToolbarIntent::Device(action));
                            ui.close();
                        }
                    }
                    Row::Copy(label, text) => {
                        if menu_item(ui, &label).clicked() {
                            ctx.copy_text(text);
                            ui.close();
                        }
                    }
                }
            }
        },
    );
}
