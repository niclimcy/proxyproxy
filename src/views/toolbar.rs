use crate::domain::filter;
use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::intent::{DeviceAction, SessionIntent, ToolbarIntent};
use crate::views::theme::*;
use eframe::egui::{self, Color32, Frame, Margin, RichText, Stroke, TextEdit};
use egui_phosphor::regular as ph;

pub struct Props<'a> {
    pub port: u16,
    pub cert: &'a str,
    pub logo: &'a egui::TextureHandle,
    pub show_rules: &'a mut bool,
    pub active_rules: usize,
}

pub fn show(ui: &mut egui::Ui, ctx: &egui::Context, vm: &mut CaptureVm, p: Props) -> Vec<ToolbarIntent> {
    let mut out = vec![];
    let Props { port, cert, logo, show_rules, active_rules } = p;
    egui::Panel::top("top").frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(14, 8)).stroke(Stroke::new(1.0, BORDER))).show(ui, |ui| {
        ui.horizontal(|ui| {
            let on = vm.show_sidebar;
            let toggle = egui::Button::new(RichText::new(ph::SIDEBAR).size(17.0).color(if on { ACCENT } else { MUTED }))
                .min_size(egui::vec2(32.0, 32.0))
                .fill(if on { ACCENT.gamma_multiply(0.15) } else { Color32::TRANSPARENT })
                .stroke(Stroke::NONE);
            if ui.add(toggle).on_hover_text("Toggle sidebar (⌘B)").clicked() {
                vm.show_sidebar = !vm.show_sidebar;
            }
            ui.add_space(2.0);
            ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(30.0, 30.0)));
            ui.label(RichText::new("proxyproxy").size(16.0).strong());
            let paused = vm.capture_paused();
            let label = if paused { "paused".to_string() } else { format!("127.0.0.1:{port}") };
            if ui.add(egui::Button::new(RichText::new(ico(if paused { ph::PLAY } else { ph::PAUSE }, &label)).size(12.0).color(if paused { AMBER } else { GREEN })).corner_radius(20)).on_hover_text("Pause / resume capturing").clicked() {
                out.push(ToolbarIntent::ToggleCapture);
            }
            ui.add_space(8.0);
            let search = ui.add(
                TextEdit::singleline(&mut vm.filter)
                    .id(egui::Id::unique("search"))
                    .hint_text(ico(ph::MAGNIFYING_GLASS, "host:api.* status:>=400 method:POST body:\"userId\"  (⌘F)"))
                    .desired_width(420.0)
                    .margin(Margin::symmetric(12, 7))
                    .background_color(BG),
            );
            if vm.focus_search {
                search.request_focus();
                vm.focus_search = false;
            }
            search.on_hover_text(format!("Fields: {}\nOperators: -negate  =exact  *wildcard  /regex/  >=num\nis: pinned ws error mocked pending json", filter::FIELDS.join(" ")));
            saved_menu(ui, vm, &mut out);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if active_rules > 0 { format!("Rules ({active_rules})") } else { "Rules".into() };
                let b = egui::Button::new(RichText::new(ico(ph::SLIDERS_HORIZONTAL, &label)).size(13.5)).fill(if *show_rules { ACCENT.gamma_multiply(0.3) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE);
                if ui.add(b).clicked() {
                    *show_rules = !*show_rules;
                }
                devices_menu(ui, ctx, port, cert, &mut out);
                session_menu(ui, &mut out);
                if icon_button(ui, ph::PLUS, "Compose").clicked() {
                    out.push(ToolbarIntent::Compose);
                }
                if icon_button(ui, ph::TRASH, "Clear").on_hover_text("Clear unpinned requests").clicked() {
                    out.push(ToolbarIntent::ClearUnpinned);
                }
            });
        });
        ui.horizontal(|ui| {
            for (label, tok, c) in [("Errors", "is:error", RED), ("Pinned", "is:pinned", AMBER), ("JSON", "is:json", GREEN), ("WebSocket", "is:ws", PURPLE), ("Modified", "is:mocked", BLUE), ("Pending", "is:pending", MUTED)] {
                if chip(ui, label, vm.token_on(tok), c).clicked() {
                    out.push(ToolbarIntent::ToggleToken(tok));
                }
            }
            if let Some(e) = &vm.query.error {
                ui.label(RichText::new(format!("{} {e}", ph::WARNING)).color(RED).size(12.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut vm.follow, "Follow");
                let hidden = vm.hidden_cols();
                ui.menu_button(RichText::new(ico(ph::COLUMNS, "Columns")).size(12.5), |ui| {
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
    ui.menu_button(RichText::new(ph::STAR).size(14.0), |ui| {
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
    })
    .response
    .on_hover_text("Saved queries");
}

fn session_menu(ui: &mut egui::Ui, out: &mut Vec<ToolbarIntent>) {
    ui.menu_button(RichText::new(ico(ph::FOLDER_OPEN, "Session")).size(13.5), |ui| {
        let mut act = None::<u8>;
        for (i, l) in ["New session", "Open session…", "Save session as…", "-", "Import HAR / Charles JSON…", "-", "Export HAR (visible)", "Export CSV (visible)", "Export rules…", "Import rules…"].iter().enumerate() {
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
            _ => None,
        };
        if let Some(i) = intent {
            out.push(ToolbarIntent::Session(i));
        }
    });
}

fn devices_menu(ui: &mut egui::Ui, ctx: &egui::Context, port: u16, cert: &str, out: &mut Vec<ToolbarIntent>) {
    ui.menu_button(RichText::new(ico(ph::DEVICE_MOBILE, "Capture")).size(13.5), |ui| {
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
    });
}
