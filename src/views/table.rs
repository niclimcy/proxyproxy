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
    let follow = vm.follow && vm.sort.is_none();
    let sort = vm.sort;
    let has_flows = vm.has_flows();

    egui::CentralPanel::default().frame(Frame::new().fill(canvas()).inner_margin(Margin::same(PAD))).show(ui, |ui| {
        if !has_flows {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() - 280.0).max(24.0) / 2.0);
                let card_width = ui.clip_rect().width().min(500.0);
                card().show(ui, |ui| {
                    ui.set_width((card_width - 2.0 * PAD as f32).max(0.0));
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.label(overline("GET STARTED"));
                        ui.add_space(5.0);
                        ui.label(RichText::new("Start capturing traffic").size(FS_TITLE).strong());
                        ui.add_space(6.0);
                        ui.label(muted(format!("Proxy is ready on 127.0.0.1:{port}.")));
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            let copied = ui.add(action_button(ico(ph::COPY, "Copy test command"), accent())).clicked();
                            if copied {
                                ctx.copy_text(format!("proxyproxy run --port {port} -- curl https://example.com"));
                            }
                            ui.label(muted("or use Capture → Set system proxy"));
                        });
                        ui.add_space(12.0);
                        ui.label(muted("The first request will appear here. Select it to inspect the request and response."));
                    });
                });
            });
            return;
        }
        if vm.visible.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() - 190.0).max(24.0) / 2.0);
                card().show(ui, |ui| {
                    ui.set_min_width(520.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.label(RichText::new("No requests match this search").size(FS_TITLE).strong());
                        ui.add_space(6.0);
                        ui.label(muted(format!("Nothing matches “{}”. Clear the search to see all captured traffic.", vm.filter.trim())));
                        ui.add_space(12.0);
                        if ui.add(action_button(ico(ph::X, "Clear search"), accent())).clicked() {
                            out.push(RowIntent::ClearFilter);
                        }
                    });
                });
            });
            return;
        }
        ui.style_mut().interaction.selectable_labels = false;
        ui.style_mut().interaction.multi_widget_text_select = false;
        let mut tb = TableBuilder::new(ui).striped(true).sense(egui::Sense::click()).cell_layout(egui::Layout::left_to_right(egui::Align::Center)).stick_to_bottom(follow);
        for &c in &cols {
            tb = tb.column(match ALL_COLS[c] {
                "#" => Column::exact(92.0),
                "App" => Column::initial(110.0).resizable(true).clip(true),
                "Method" => Column::exact(84.0),
                "Status" => Column::exact(72.0),
                "Host" => Column::initial(210.0).resizable(true).clip(true),
                "Path" => Column::remainder().clip(true),
                "Time" => Column::exact(72.0),
                _ => Column::exact(80.0),
            });
        }
        tb.header(HIT, |mut h| {
            for &c in &cols {
                let (_, r) = h.col(|ui| {
                    let arrow = match sort {
                        Some((sc, asc)) if sc == c => if asc { " ▲" } else { " ▼" },
                        _ => "",
                    };
                    ui.label(overline(format!("{}{arrow}", ALL_COLS[c].to_uppercase())));
                });
                let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Click to sort");
                if r.clicked() {
                    out.push(RowIntent::Sort(c));
                }
            }
        })
        .body(|body| {
            let st = vm.store.lock();
            body.rows(HIT, vm.visible.len(), |mut row| {
                let f = &st.flows[vm.visible[row.index()]];
                row.set_selected(vm.selected == Some(f.id));
                for &c in &cols {
                    row.col(|ui| match ALL_COLS[c] {
                        "#" => {
                            status_dot(ui, f.status.map(status_color).unwrap_or_else(fg_muted));
                            if let Some(i) = f.color {
                                ui.label(RichText::new(ph::CIRCLE).color(tag_colors()[i as usize % 5].1).size(FS_CAPTION));
                            }
                            if f.pinned {
                                ui.label(RichText::new(ph::PUSH_PIN).color(warning()).size(FS_CAPTION));
                            }
                            if diff_base == Some(f.id) {
                                ui.label(RichText::new(ph::GIT_DIFF).color(accent()).size(FS_CAPTION));
                            }
                            ui.label(RichText::new(f.id.to_string()).monospace().color(fg_muted()));
                        }
                        "App" => {
                            ui.label(RichText::new(f.app_label()).color(fg_muted()));
                        }
                        "Method" => {
                            badge(ui, if f.is_ws { "WS" } else { &f.method }, if f.is_ws { violet() } else { method_color(&f.method) });
                        }
                        "Status" => match f.status {
                            Some(s) => {
                                badge(ui, &s.to_string(), status_color(s));
                            }
                            None => {
                                ui.spinner();
                            }
                        },
                        "Host" => {
                            ui.label(&f.host);
                        }
                        "Path" => {
                            ui.label(RichText::new(&f.path).color(fg_muted()));
                            if !f.tag.is_empty() {
                                pill(ui, &f.tag, violet());
                            }
                            if !f.comment.is_empty() {
                                ui.label(RichText::new(ph::NOTE).color(warning())).on_hover_text(&f.comment);
                            }
                        }
                        "Time" => {
                            ui.label(RichText::new(f.duration_ms.map(|d| format!("{d} ms")).unwrap_or_default()).monospace().color(fg_muted()));
                        }
                        _ => {
                            ui.label(RichText::new(f.response.as_ref().map(|r| fmt_size(r.body.len())).unwrap_or_default()).monospace().color(fg_muted()));
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
                    if b(ui, &ico(ph::PLAY, &format!("Replay / edit…  ({}R)", cmd()))) {
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
                        for (i, (n, _)) in tag_colors().iter().enumerate() {
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
