use crate::domain::filter;
use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::intent::{DeviceAction, SessionIntent, ToolbarIntent};
use crate::views::theme::*;
use eframe::egui::{self, Color32, Margin, RichText, TextEdit};
use egui_phosphor::regular as ph;

pub struct Props<'a> {
    pub port: u16,
    pub cert: &'a str,
    pub show_rules: &'a mut bool,
    pub active_rules: usize,
    pub status: &'a str,
    pub api_port: u16,
    pub paused_breakpoints: usize,
}

pub fn show(ui: &mut egui::Ui, ctx: &egui::Context, vm: &mut CaptureVm, p: Props) -> Vec<ToolbarIntent> {
    let mut out = vec![];
    let Props { port, cert, show_rules, active_rules, status, api_port, paused_breakpoints } = p;
    egui::Panel::top("top").frame(panel_frame().inner_margin(Margin::symmetric(14, 8))).show(ui, |ui| {
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
            let pill_resp = flat(ui, |ui| ui.add(egui::Button::new(RichText::new(ico(icon, &label)).size(FS_SMALL).color(state)).fill(tint(state, Tint::Normal)).corner_radius(RADIUS_PILL)));
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
                let b = egui::Button::new(RichText::new(ico(ph::SLIDERS_HORIZONTAL, &label)).size(FS_SMALL)).fill(if *show_rules { tint(accent(), Tint::Normal) } else { Color32::TRANSPARENT });
                if flat(ui, |ui| ui.add(b)).clicked() {
                    *show_rules = !*show_rules;
                }
                capsule(ui, |ui| {
                    // right_to_left parent: add in reverse visual order
                    devices_menu(ui, ctx, port, api_port, cert, &mut out);
                    session_menu(ui, &mut out);
                    if icon_button(ui, ph::PLUS, "Compose").clicked() {
                        out.push(ToolbarIntent::Compose);
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
                ui.checkbox(&mut vm.follow, "Follow");
                let hidden = vm.hidden_cols();
                toolbar_menu(ui, ph::COLUMNS, "Columns", |ui| {
                    for c in ALL_COLS.iter().skip(1) {
                        let mut on = !hidden.iter().any(|h| h == c);
                        if ui.checkbox(&mut on, *c).changed() {
                            out.push(ToolbarIntent::SetColumn(c, on));
                        }
                    }
                });
                ui.label(muted(format!("{} of {}", vm.visible.len(), vm.flow_count())));
            });
        });
    });
    out
}

fn saved_menu(ui: &mut egui::Ui, vm: &CaptureVm, out: &mut Vec<ToolbarIntent>) {
    let menu = flat(ui, |ui| ui.menu_button(RichText::new(ph::STAR).size(FS_BODY), |ui| {
        if !vm.filter.trim().is_empty() && ui.button(ico(ph::PLUS, "Save current query")).clicked() {
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
    }));
    menu.response.on_hover_text("Saved queries");
}

fn session_menu(ui: &mut egui::Ui, out: &mut Vec<ToolbarIntent>) {
    toolbar_menu(ui, ph::FOLDER_OPEN, "Session", |ui| {
        let mut act = None::<u8>;
        for (i, l) in ["New session", "Open session…", "Save session as…", "-", "Import HAR / Charles JSON…", "-", "Export HAR (visible)", "Export CSV (visible)", "Export rules…", "Import rules…", "-", "Clear unpinned requests"].iter().enumerate() {
            if *l == "-" {
                ui.separator();
            } else if ui.button(*l).clicked() {
                act = Some(i as u8);
                ui.close();
            }
        }
        let Some(a) = act else { return };
        let dlg = rfd::FileDialog::new;
        let intent = match a {
            0 => Some(SessionIntent::New),
            1 => dlg().add_filter("proxyproxy session", &["pxp"]).pick_file().map(SessionIntent::Open),
            2 => dlg().add_filter("proxyproxy session", &["pxp"]).set_file_name("session.pxp").save_file().map(SessionIntent::SaveAs),
            4 => dlg().add_filter("HAR / Charles JSON", &["har", "json", "chlsj"]).pick_file().map(SessionIntent::ImportFlows),
            6 => dlg().set_file_name("export.har").save_file().map(SessionIntent::ExportHar),
            7 => dlg().set_file_name("export.csv").save_file().map(SessionIntent::ExportCsv),
            8 => dlg().set_file_name("proxyproxy-rules.json").save_file().map(SessionIntent::ExportRules),
            9 => dlg().add_filter("rules", &["json"]).pick_file().map(SessionIntent::ImportRules),
            11 => {
                out.push(ToolbarIntent::ClearUnpinned);
                None
            }
            _ => None,
        };
        if let Some(i) = intent {
            out.push(ToolbarIntent::Session(i));
        }
    });
}

fn devices_menu(ui: &mut egui::Ui, ctx: &egui::Context, port: u16, api_port: u16, cert: &str, out: &mut Vec<ToolbarIntent>) {
    toolbar_menu(ui, ph::DEVICE_MOBILE, "Capture", |ui| {
        let item = |ui: &mut egui::Ui, label: &str| {
            let c = ui.button(label).clicked();
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
        device(ui, "Set system proxy", DeviceAction::SetSystemProxy);
        device(ui, "Clear system proxy", DeviceAction::ClearSystemProxy);
        device(ui, "Launch Chrome with proxy (clean profile)", DeviceAction::LaunchChrome);
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
        device(ui, "iOS Simulator: install CA", DeviceAction::IosSimInstallCa);
        device(ui, "Android: set proxy (adb)", DeviceAction::AndroidSetProxy);
        device(ui, "Android: clear proxy", DeviceAction::AndroidClearProxy);
        device(ui, "Android: push CA", DeviceAction::AndroidInstallCa);
        ui.separator();
        if item(ui, "Copy CA path") {
            ctx.copy_text(cert.to_string());
        }
        if item(ui, &format!("Copy REST API URL (127.0.0.1:{api_port})")) {
            ctx.copy_text(format!("http://127.0.0.1:{api_port}"));
        }
    });
}
