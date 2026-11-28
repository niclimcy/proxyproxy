use crate::domain::filter;
use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::devices_vm::{DevicesVm, IosCaStatus};
use crate::viewmodels::intent::{DeviceAction, SessionIntent, ToolbarIntent};
use crate::views::theme::{
    FS_BODY, FS_CAPTION, FS_SMALL, HIT, RADIUS_PILL, Tint, accent, bar_frame, canvas, capsule,
    checkbox, chip, cmd, danger, fg_muted, flat, ico, icon_button, icon_only, info, menu_item,
    menu_width, muted, right_panel_toggle, row_within, sidebar_toggle, submenu_item, success, tint,
    toolbar_menu, violet, warning,
};
use eframe::egui::{self, Margin, RichText, TextEdit};
use egui_phosphor::regular as ph;

// the status text can truncate, but not to nothing
const STATUS_MIN: f32 = 120.0;
// star menu, capsule padding and the Rules toggle that share the compact action row
const COMPACT_ROW_RESERVED: f32 = 330.0;

#[derive(Clone, Copy)]
pub struct Props<'a> {
    pub port: u16,
    pub cert: &'a str,
    pub devices: &'a DevicesVm,
    pub show_rules: bool,
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
    egui::Panel::top("top")
        .frame(bar_frame())
        .show_separator_line(false)
        .show(ui, |ui| bar(ui, ctx, vm, p, &mut out));
    out
}

fn bar(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &mut CaptureVm,
    p: Props,
    out: &mut Vec<ToolbarIntent>,
) {
    let scope_active = vm.scope_active();
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
    ui.spacing_mut().item_spacing.y = 8.0;
    // measured from the last wide layout so the actions never overlap the search box
    let wide_min_id = egui::Id::unique("toolbar-wide-min");
    let wide_min = ui
        .data(|d| d.get_temp::<f32>(wide_min_id))
        .unwrap_or(1240.0);
    let compact = ui.available_width() < wide_min;
    if compact {
        let props = Props {
            port,
            cert,
            devices,
            show_rules,
            active_rules,
            status,
            api_port,
            paused_breakpoints,
            show_help,
        };
        compact_primary_rows(ui, ctx, vm, props, out);
    } else {
        ui.horizontal(|ui| {
            if sidebar_toggle(ui, vm.show_sidebar).clicked() {
                out.push(ToolbarIntent::ToggleSidebar);
            }
            record_pill(ui, vm.capture_paused(), port, out);
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
            saved_menu(ui, vm, out);
            let left_used = ui.min_rect().width();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if active_rules > 0 { format!("Rules ({active_rules})") } else { "Rules".into() };
                let tip = format!("{} rules panel", if show_rules { "Hide" } else { "Show" });
                if right_panel_toggle(ui, show_rules, &label, tip).clicked() {
                    out.push(ToolbarIntent::ToggleRules);
                }
                action_buttons(ui, ctx, setup, show_help, true, true, out);
                let needed = left_used + ui.min_rect().width() + STATUS_MIN;
                ui.data_mut(|d| d.insert_temp(wide_min_id, needed));
                // remaining space between the search box and the actions
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if paused_breakpoints > 0 && paused_pill(ui, paused_breakpoints).clicked() {
                        out.push(ToolbarIntent::ShowBreakpoint);
                    }
                    if scope_active && scope_pill(ui).clicked() {
                        out.push(ToolbarIntent::ShowScope);
                    }
                    if let Some(n) = devices.hosts_only() && hosts_pill(ui, n).clicked() {
                        out.push(ToolbarIntent::ShowScope);
                    }
                    if !status.is_empty() {
                        ui.add(egui::Label::new(RichText::new(status).size(FS_SMALL).color(fg_muted())).truncate()).on_hover_text(status);
                    }
                });
            });
        });
    }
    if compact {
        compact_filter_rows(ui, vm, out);
    } else {
        let w = ui.available_width();
        // the controls sit first at the right edge, so a long filter error can only take what is left
        row_within(ui, w, true, |ui| {
            if chip(ui, &ico(ph::ARROW_LINE_DOWN, "Follow"), vm.follow, accent())
                .on_hover_text("Keep the newest request in view")
                .clicked()
            {
                out.push(ToolbarIntent::ToggleFollow);
            }
            let hidden = vm.hidden_cols();
            toolbar_menu(ui, ph::COLUMNS, "Columns", false, |ui| {
                for c in ALL_COLS.iter().skip(1) {
                    let mut on = !hidden.iter().any(|h| h == c);
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
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
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
                ui.spacing_mut().item_spacing.x = 8.0;
                if let Some(e) = &vm.query.error {
                    let text = format!("{} {e}", ph::WARNING);
                    ui.add(
                        egui::Label::new(RichText::new(&text).color(danger()).size(FS_CAPTION))
                            .truncate(),
                    )
                    .on_hover_text(text);
                }
            });
        });
    }
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
                out.push(ToolbarIntent::ToggleFollow);
            }
            toolbar_menu(ui, ph::COLUMNS, "Columns", false, |ui| {
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
    let scope_active = vm.scope_active();
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
            out.push(ToolbarIntent::ToggleSidebar);
        }
        record_pill(ui, vm.capture_paused(), port, out);
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
    let w = ui.available_width();
    // titles collapse together once the labelled group no longer fits beside Rules
    let labelled_w = ui
        .data(|d| d.get_temp::<f32>(egui::Id::unique("toolbar-actions-width")))
        .unwrap_or(520.0);
    let labels = w >= labelled_w + COMPACT_ROW_RESERVED;
    // Rules sits first at the right edge so a long status can only take what is left
    row_within(ui, w, true, |ui| {
        let label = if active_rules > 0 {
            format!("Rules ({active_rules})")
        } else {
            "Rules".into()
        };
        if right_panel_toggle(
            ui,
            show_rules,
            &label,
            format!("{} rules panel", if show_rules { "Hide" } else { "Show" }),
        )
        .clicked()
        {
            out.push(ToolbarIntent::ToggleRules);
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            saved_menu(ui, vm, out);
            action_buttons(ui, ctx, setup, show_help, labels, false, out);
            if paused_breakpoints > 0 && paused_pill(ui, paused_breakpoints).clicked() {
                out.push(ToolbarIntent::ShowBreakpoint);
            }
            if scope_active && scope_pill(ui).clicked() {
                out.push(ToolbarIntent::ShowScope);
            }
            if let Some(n) = devices.hosts_only()
                && hosts_pill(ui, n).clicked()
            {
                out.push(ToolbarIntent::ShowScope);
            }
            if !status.is_empty() {
                ui.add(
                    egui::Label::new(RichText::new(status).size(FS_SMALL).color(fg_muted()))
                        .truncate(),
                )
                .on_hover_text(status);
            }
        });
    });
}

#[derive(Clone, Copy)]
enum Action {
    Compose,
    Clear,
    Session,
    Sources,
    Help,
}

/// One connected group of actions. `reverse` is for right-to-left parents, which flip the order.
fn action_buttons(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    setup: Setup,
    show_help: bool,
    labels: bool,
    reverse: bool,
    out: &mut Vec<ToolbarIntent>,
) {
    let mut actions = vec![
        Action::Compose,
        Action::Clear,
        Action::Session,
        Action::Sources,
    ];
    if show_help {
        actions.push(Action::Help);
    }
    if reverse {
        actions.reverse();
    }
    let width_id = egui::Id::unique("toolbar-actions-width");
    capsule(ui, |ui| {
        for action in actions {
            match action {
                Action::Compose => {
                    let r = if labels {
                        icon_button(ui, ph::PLUS, "Compose", true)
                    } else {
                        icon_only(ui, ph::PLUS, "Compose")
                    };
                    if r.clicked() {
                        out.push(ToolbarIntent::Compose);
                    }
                }
                Action::Clear => {
                    let r = if labels {
                        icon_button(ui, ph::TRASH, "Clear", true)
                    } else {
                        icon_only(ui, ph::TRASH, "Clear")
                    };
                    if r.on_hover_text("Remove all requests except pinned")
                        .clicked()
                    {
                        out.push(ToolbarIntent::ClearUnpinned);
                    }
                }
                Action::Session => session_menu(ui, out, !labels),
                Action::Sources => devices_menu(ui, ctx, setup, out, !labels),
                Action::Help => help_menu(ui, if labels { "Help" } else { "" }, out),
            }
        }
        if labels {
            ui.data_mut(|d| d.insert_temp(width_id, ui.min_rect().width()));
        }
    });
}

/// Only controls recording: the proxy keeps forwarding while paused. Routing lives in the Sources menu.
fn record_pill(ui: &mut egui::Ui, paused: bool, port: u16, out: &mut Vec<ToolbarIntent>) {
    let (icon, label, state, tip) = if paused {
        (
            ph::PAUSE_CIRCLE,
            "Paused",
            warning(),
            format!(
                "Recording paused. Traffic still flows through :{port} but is not saved. Click to resume ({}⇧P)",
                cmd()
            ),
        )
    } else {
        (
            ph::RECORD,
            "Recording",
            success(),
            format!(
                "Recording traffic arriving on 127.0.0.1:{port}. Click to pause ({}⇧P)",
                cmd()
            ),
        )
    };
    let resp = flat(ui, |ui| {
        ui.add(
            egui::Button::new(RichText::new(ico(icon, label)).size(FS_SMALL).color(state))
                .fill(tint(state, Tint::Normal))
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(0.0, HIT))
                .corner_radius(RADIUS_PILL),
        )
    });
    if resp.on_hover_text(tip).clicked() {
        out.push(ToolbarIntent::ToggleCapture);
    }
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

fn scope_pill(ui: &mut egui::Ui) -> egui::Response {
    flat(ui, |ui| {
        ui.add(
            egui::Button::new(
                RichText::new(ico(ph::TARGET, "Scoped"))
                    .size(FS_SMALL)
                    .color(accent()),
            )
            .fill(tint(accent(), Tint::Normal))
            .min_size(egui::vec2(0.0, HIT))
            .corner_radius(RADIUS_PILL),
        )
    })
    .on_hover_text("Only part of the traffic is recorded. Open scope settings")
}

fn hosts_pill(ui: &mut egui::Ui, n: usize) -> egui::Response {
    flat(ui, |ui| {
        ui.add(
            egui::Button::new(
                RichText::new(ico(ph::ARROWS_SPLIT, &format!("{n} hosts only")))
                    .size(FS_SMALL)
                    .color(accent()),
            )
            .fill(tint(accent(), Tint::Normal))
            .min_size(egui::vec2(0.0, HIT))
            .corner_radius(RADIUS_PILL),
        )
    })
    .on_hover_text(
        "Only these hosts go through the proxy; the rest connect directly. Open the list",
    )
}

fn saved_menu(ui: &mut egui::Ui, vm: &CaptureVm, out: &mut Vec<ToolbarIntent>) {
    let menu = flat(ui, |ui| {
        ui.menu_button(RichText::new(ph::STAR).size(FS_BODY), |ui| {
            let saved = vm.saved_queries();
            let save = ico(ph::PLUS, "Save current query");
            let mut labels: Vec<&str> = saved.iter().map(|(n, _)| n.as_str()).collect();
            labels.push(&save);
            menu_width(ui, &labels);
            if !vm.filter.trim().is_empty() && menu_item(ui, &save, None).clicked() {
                out.push(ToolbarIntent::SaveQuery);
                ui.close();
                return;
            }
            if saved.is_empty() {
                ui.label(muted("No saved queries"));
            }
            for (i, (name, q)) in saved.iter().enumerate() {
                ui.horizontal(|ui| {
                    if icon_only(ui, ph::X, "Delete saved query").clicked() {
                        out.push(ToolbarIntent::DeleteQuery(i));
                    }
                    if menu_item(ui, name, None).on_hover_text(q).clicked() {
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
        false,
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
                } else if menu_item(ui, l, None).clicked() {
                    act = u8::try_from(i).ok();
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
    toolbar_menu(ui, ph::QUESTION, label, false, |ui| {
        let settings = format!("{},", cmd());
        menu_width(
            ui,
            &[
                &format!("Settings…    {settings}"),
                "Setup guide",
                "About proxyproxy",
            ],
        );
        if menu_item(ui, "Settings…", Some(&settings)).clicked() {
            out.push(ToolbarIntent::Settings);
            ui.close();
        }
        if menu_item(ui, "Setup guide", None).clicked() {
            out.push(ToolbarIntent::Onboarding);
            ui.close();
        }
        if menu_item(ui, "About proxyproxy", None).clicked() {
            out.push(ToolbarIntent::About);
            ui.close();
        }
    });
}

enum Row {
    Note(String),
    Separator,
    Device(String, DeviceAction),
    // checked state is shown in the row, so the label never flips between Enable and Disable
    Toggle(String, bool, DeviceAction),
    Copy(String, String),
}

fn device_rows(s: Setup) -> Vec<Row> {
    let (port, api_port) = (s.port, s.api_port);
    let device = |label: &str, action| Row::Device(label.to_string(), action);
    let note = |text: &str| Row::Note(text.to_string());
    let mut rows = vec![
        Row::Note(format!("Listening on 127.0.0.1:{port}")),
        Row::Separator,
        note("This computer"),
        device("Trust CA on this computer", DeviceAction::TrustCa),
    ];
    let hosts_only = s.devices.hosts_only();
    rows.push(Row::Toggle(
        match hosts_only {
            Some(n) => format!("System proxy · {n} host(s) only"),
            None => "System proxy".into(),
        },
        s.devices.system_proxy_enabled,
        DeviceAction::ToggleSystemProxy,
    ));
    rows.push(note(if s.devices.system_proxy_enabled {
        "Routes this Mac and iOS Simulators"
    } else {
        "Off: this Mac and iOS Simulators connect directly"
    }));
    if cfg!(target_os = "macos") {
        let (on, n) = s.devices.proxy_only_setting();
        rows.push(Row::Toggle(
            format!("Only route listed hosts ({n})"),
            on,
            DeviceAction::ToggleProxyOnly,
        ));
    } else {
        rows.push(note("Host-only routing needs macOS"));
    }
    rows.extend([
        device(
            "Launch Chrome with proxy (clean profile)",
            DeviceAction::LaunchChrome,
        ),
        Row::Separator,
        note("iOS Simulators (follow System proxy)"),
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
        let route = if s.devices.system_proxy_enabled {
            "routed"
        } else {
            "not routed"
        };
        rows.push(note(&format!("{} · {route} · {ca}", sim.name)));
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
    rows.push(Row::Separator);
    rows.push(note(
        "Android (set per device, independent of System proxy)",
    ));
    if s.devices.android.is_empty() {
        rows.push(note("No Android device or emulator found by adb"));
    }
    for d in &s.devices.android {
        rows.push(Row::Toggle(
            format!("{} ({})", d.name, d.serial),
            d.proxied,
            DeviceAction::AndroidToggleProxy(d.serial.clone()),
        ));
    }
    rows.extend([
        device("Push CA to Android devices", DeviceAction::AndroidInstallCa),
        Row::Separator,
        // rarely needed, so grouped into one submenu instead of four rows
        Row::Copy(
            "Shell environment".into(),
            crate::services::devices::env_snippet(port, s.cert),
        ),
        Row::Copy(
            "proxyproxy run command".into(),
            format!("proxyproxy run --port {port} -- "),
        ),
        Row::Copy("CA certificate path".into(), s.cert.to_string()),
        Row::Copy(
            format!("REST API URL (127.0.0.1:{api_port})"),
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
    let label = setup
        .devices
        .sources_label()
        .unwrap_or_else(|| "Sources".into());
    toolbar_menu(
        ui,
        ph::DEVICE_MOBILE,
        if compact { "" } else { &label },
        true,
        |ui| {
            let rows = device_rows(setup);
            let labels: Vec<&str> = rows
                .iter()
                .filter_map(|row| match row {
                    Row::Note(l) | Row::Device(l, _) | Row::Toggle(l, ..) | Row::Copy(l, _) => {
                        Some(l.as_str())
                    }
                    Row::Separator => None,
                })
                .collect();
            menu_width(ui, &labels);
            let (copies, rows): (Vec<Row>, Vec<Row>) =
                rows.into_iter().partition(|r| matches!(r, Row::Copy(..)));
            for row in rows {
                match row {
                    Row::Note(text) => {
                        ui.label(muted(text));
                    }
                    Row::Separator => {
                        ui.separator();
                    }
                    Row::Device(label, action) => {
                        if menu_item(ui, &label, None).clicked() {
                            out.push(ToolbarIntent::Device(action));
                            ui.close();
                        }
                    }
                    Row::Toggle(label, on, action) => {
                        let icon = if on { ph::CHECK_SQUARE } else { ph::SQUARE };
                        // stays open so the flipped checkbox confirms the change
                        if menu_item(ui, &ico(icon, &label), None).clicked() {
                            out.push(ToolbarIntent::Device(action));
                        }
                    }
                    Row::Copy(..) => {}
                }
            }
            submenu_item(ui, &ico(ph::COPY, "Copy"), |ui| {
                let labels: Vec<&str> = copies
                    .iter()
                    .filter_map(|r| {
                        if let Row::Copy(l, _) = r {
                            Some(l.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                menu_width(ui, &labels);
                for row in copies {
                    if let Row::Copy(label, text) = row
                        && menu_item(ui, &label, None).clicked()
                    {
                        ctx.copy_text(text);
                        ui.close();
                    }
                }
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::services::app_icons::IconStore;
    use crate::services::flow_store::FlowStore;
    use crate::viewmodels::status::NoRepaint;
    use crate::views::layout_test::*;
    use std::sync::{Arc, Mutex};

    /// Width the bar needs when the window gives it `width` px.
    fn needed(width: f32) -> f32 {
        let ctx = ctx();
        let cfg = Config {
            scope_only: true,
            scope: vec![HOST.into()],
            saved: vec![(WORDS.into(), WORDS.into())],
            ..Default::default()
        };
        let cfg = Arc::new(Mutex::new(cfg));
        let icons = IconStore::new(std::env::temp_dir(), Arc::new(NoRepaint));
        let mut vm = CaptureVm::new(FlowStore::default(), cfg.clone(), icons);
        vm.filter = format!("host:{HOST} path:{PATH}");
        vm.query.error = Some(format!("invalid regex: {WORDS}{WORDS}"));
        let devices = DevicesVm::new(8080, "/path/to/ca.pem".into(), cfg, Arc::new(NoRepaint));
        let status = format!("proxy on port 8080 stopped: {WORDS}{WORDS}");
        let props = Props {
            port: 8080,
            cert: "/path/to/ca.pem",
            devices: &devices,
            show_rules: true,
            active_rules: 12,
            status: &status,
            api_port: 8081,
            paused_breakpoints: 3,
            show_help: true,
        };
        used_width(&ctx, width, |ui| {
            let ctx = ui.ctx().clone();
            bar(ui, &ctx, &mut vm, props, &mut vec![]);
        })
    }

    #[test]
    fn the_bar_fits_every_window_width() {
        let leaks: Vec<String> = [560.0, 640.0, 800.0, 1000.0, 1240.0, 1360.0, 1800.0]
            .into_iter()
            .filter_map(|w| {
                let used = needed(w);
                (used > w + 1.0).then(|| format!("{w} px window: the bar needs {used}"))
            })
            .collect();
        assert!(leaks.is_empty(), "{}", leaks.join("\n"));
    }
}
