use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::intent::RowIntent;
use crate::viewmodels::rules_vm::RuleKind;
use crate::views::theme::*;
use eframe::egui::{self, Frame, Margin, RichText};
use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular as ph;

pub fn show(ui: &mut egui::Ui, ctx: &egui::Context, vm: &CaptureVm, diff_base: Option<u64>, port: u16) -> Vec<RowIntent> {
    let mut out = vec![];
    let hidden = vm.hidden_cols();
    let cols: Vec<usize> = (0..ALL_COLS.len()).filter(|&i| i == 0 || !hidden.iter().any(|h| h == ALL_COLS[i])).collect();
    let selected = vm.selected;
    let follow = vm.follow && vm.sort.is_none();
    let sort = vm.sort;
    let has_flows = vm.has_flows();

    egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(Margin::same(PAD))).show(ui, |ui| {
        if !has_flows {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(format!("Waiting for traffic…\n\nPoint a client at 127.0.0.1:{port}, use the Capture menu,\nor run:  proxyproxy run -- curl https://example.com")).color(MUTED).size(15.0));
            });
            return;
        }
        if vm.visible.is_empty() {
            ui.centered_and_justified(|ui| ui.label(RichText::new("No requests match the filter").color(MUTED).size(14.0)));
            return;
        }
        let mut tb = TableBuilder::new(ui).striped(true).sense(egui::Sense::click()).cell_layout(egui::Layout::left_to_right(egui::Align::Center)).stick_to_bottom(follow);
        for &c in &cols {
            tb = tb.column(match ALL_COLS[c] {
                "#" => Column::exact(64.0),
                "App" => Column::initial(110.0).resizable(true).clip(true),
                "Method" => Column::exact(72.0),
                "Status" => Column::exact(64.0),
                "Host" => Column::initial(210.0).resizable(true).clip(true),
                "Path" => Column::remainder().clip(true),
                "Time" => Column::exact(72.0),
                _ => Column::exact(80.0),
            });
        }
        tb.header(26.0, |mut h| {
            for &c in &cols {
                let (_, r) = h.col(|ui| {
                    let arrow = match sort {
                        Some((sc, asc)) if sc == c => if asc { " ▲" } else { " ▼" },
                        _ => "",
                    };
                    ui.label(RichText::new(format!("{}{arrow}", ALL_COLS[c].to_uppercase())).size(11.0).strong().color(MUTED));
                });
                if r.clicked() {
                    out.push(RowIntent::Sort(c));
                }
            }
        })
        .body(|body| {
            let st = vm.store.lock();
            body.rows(26.0, vm.visible.len(), |mut row| {
                let f = &st.flows[vm.visible[row.index()]];
                row.set_selected(selected == Some(f.id));
                for &c in &cols {
                    row.col(|ui| match ALL_COLS[c] {
                        "#" => {
                            if let Some(i) = f.color {
                                ui.label(RichText::new(ph::CIRCLE).color(COLORS[i as usize % 5].1).size(10.0));
                            }
                            if f.pinned {
                                ui.label(RichText::new(ph::PUSH_PIN).color(AMBER).size(12.0));
                            }
                            if diff_base == Some(f.id) {
                                ui.label(RichText::new(ph::GIT_DIFF).color(ACCENT).size(12.0));
                            }
                            ui.label(RichText::new(f.id.to_string()).color(MUTED));
                        }
                        "App" => {
                            ui.label(RichText::new(f.app_label()).color(MUTED));
                        }
                        "Method" => {
                            ui.label(RichText::new(if f.is_ws { "WS" } else { &f.method }).strong().color(if f.is_ws { PURPLE } else { method_color(&f.method) }));
                        }
                        "Status" => match f.status {
                            Some(s) => {
                                ui.label(RichText::new(s.to_string()).strong().color(status_color(s)));
                            }
                            None => {
                                ui.spinner();
                            }
                        },
                        "Host" => {
                            ui.label(&f.host);
                        }
                        "Path" => {
                            ui.label(RichText::new(&f.path).color(MUTED));
                            if !f.tag.is_empty() {
                                pill(ui, &f.tag, PURPLE);
                            }
                            if !f.comment.is_empty() {
                                ui.label(RichText::new(ph::NOTE).color(AMBER)).on_hover_text(&f.comment);
                            }
                        }
                        "Time" => {
                            ui.label(RichText::new(f.duration_ms.map(|d| format!("{d} ms")).unwrap_or_default()).color(MUTED));
                        }
                        _ => {
                            ui.label(RichText::new(f.response.as_ref().map(|r| fmt_size(r.body.len())).unwrap_or_default()).color(MUTED));
                        }
                    });
                }
                let resp = row.response();
                if resp.clicked() {
                    out.push(RowIntent::Select(f.id));
                }
                resp.context_menu(|ui| {
                    let id = f.id;
                    let b = |ui: &mut egui::Ui, l: &str| {
                        let c = ui.button(l).clicked();
                        if c {
                            ui.close();
                        }
                        c
                    };
                    if b(ui, &ico(ph::PLAY, "Replay / edit…  (⌘R)")) {
                        out.push(RowIntent::Replay(id));
                    }
                    ui.menu_button(ico(ph::SLIDERS_HORIZONTAL, "Create rule from this request"), |ui| {
                        use RuleKind as K;
                        for (k, l) in [(K::Breakpoint, "Breakpoint"), (K::Mock, "Mock this response"), (K::Block, "Block"), (K::Throttle, "Throttle / delay"), (K::Redirect, "Redirect to…"), (K::Header, "Rewrite header"), (K::Script, "Script")] {
                            if b(ui, l) {
                                out.push(RowIntent::Rule(id, k));
                            }
                        }
                    });
                    ui.separator();
                    if b(ui, &ico(ph::PUSH_PIN, if f.pinned { "Unpin" } else { "Pin" })) {
                        out.push(RowIntent::Pin(id));
                    }
                    ui.menu_button(ico(ph::PALETTE, "Color"), |ui| {
                        if b(ui, "None") {
                            out.push(RowIntent::Color(id, None));
                        }
                        for (i, (n, _)) in COLORS.iter().enumerate() {
                            if b(ui, n) {
                                out.push(RowIntent::Color(id, Some(i as u8)));
                            }
                        }
                    });
                    if b(ui, &ico(ph::NOTE, "Comment…")) {
                        out.push(RowIntent::Comment(id));
                    }
                    if b(ui, &ico(ph::GIT_DIFF, "Use as diff base")) {
                        out.push(RowIntent::DiffBase(id));
                    }
                    ui.separator();
                    ui.menu_button(ico(ph::COPY, "Copy as"), |ui| {
                        use crate::services::exporter as ex;
                        for (l, s) in [("URL", f.url()), ("cURL", ex::curl(f)), ("fetch", ex::fetch(f)), ("Python requests", ex::python(f)), ("HTTPie", ex::httpie(f)), ("Raw HTTP", ex::raw_http(f))] {
                            if b(ui, l) {
                                ctx.copy_text(s);
                            }
                        }
                    });
                    let host = f.hostname().to_string();
                    if b(ui, &ico(ph::EYE_SLASH, &format!("Mute {host}"))) {
                        out.push(RowIntent::Mute(host.clone()));
                    }
                    if b(ui, &ico(ph::TARGET, &format!("Capture only {host}"))) {
                        out.push(RowIntent::Scope(host.clone()));
                    }
                    if b(ui, &ico(ph::LOCK_OPEN, "Don't decrypt this host")) {
                        out.push(RowIntent::NoTls(host));
                    }
                    ui.separator();
                    if b(ui, &ico(ph::TRASH, "Delete")) {
                        out.push(RowIntent::Delete(id));
                    }
                });
            });
        });
    });
    out
}
