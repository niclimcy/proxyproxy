use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::intent::RowIntent;
use crate::viewmodels::rules_vm::RuleKind;
use crate::views::app_icon::{app_icon, app_label_hover};
use crate::views::theme::*;
use eframe::egui::{self, Frame, Margin, RichText};
use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular as ph;

const APP: usize = 1;
const PATH_MIN: f32 = 160.0;

fn fixed_width(name: &str) -> f32 {
    match name {
        "#" => 76.0,
        "Method" => 84.0,
        "Status" | "Time" => 72.0,
        _ => 80.0,
    }
}

/// Caps for the resizable columns so Path keeps `PATH_MIN` when the table is narrow.
struct ColumnWidths {
    show_app: bool,
    app_max: f32,
    host_max: f32,
}

impl ColumnWidths {
    fn fit(ui: &egui::Ui, cols: &[usize]) -> Self {
        let gap = ui.spacing().item_spacing.x;
        let fixed: f32 = cols
            .iter()
            .map(|&c| ALL_COLS[c])
            .filter(|n| !matches!(*n, "App" | "Host" | "Path"))
            .map(|n| fixed_width(n) + gap)
            .sum();
        let flexible = ui.available_width() - fixed - 3.0 * gap;
        // App is the least useful column, so it goes first when space runs out
        let show_app = flexible >= 480.0;
        let app_max = if show_app { (flexible * 0.18).max(60.0) } else { 0.0 };
        let host_max = (flexible - PATH_MIN - app_max).max(90.0);
        Self {
            show_app,
            app_max,
            host_max,
        }
    }
}

pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &CaptureVm,
    diff_base: Option<u64>,
    port: u16,
) -> Vec<RowIntent> {
    let mut out = vec![];
    let hidden = vm.hidden_cols();
    let cols: Vec<usize> = (0..ALL_COLS.len())
        .filter(|&i| i == 0 || !hidden.iter().any(|h| h == ALL_COLS[i]))
        .collect();
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
                            let command = format!("proxyproxy run --port {port} -- curl https://example.com");
                            let (icon, label, color) = if vm.test_command_copied.on() {
                                (ph::CHECK, "Copied", success())
                            } else {
                                (ph::COPY, "Copy test command", accent())
                            };
                            if action_button_sized(ui, icon, label, color, 188.0, true)
                                .on_hover_text(format!("Copy: {command}"))
                                .clicked()
                            {
                                ctx.copy_text(command);
                                out.push(RowIntent::TestCommandCopied);
                            }
                            ui.label(muted("or use Sources → System proxy"));
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
        let widths = ColumnWidths::fit(ui, &cols);
        let cols: Vec<usize> = cols.into_iter().filter(|&c| c != APP || widths.show_app).collect();
        // the dot only stands in for the status badge when that column is hidden
        let status_dot_on = !cols.iter().any(|&c| ALL_COLS[c] == "Status");
        let mut tb = TableBuilder::new(ui).striped(true).sense(egui::Sense::click()).cell_layout(egui::Layout::left_to_right(egui::Align::Center)).stick_to_bottom(follow);
        for &c in &cols {
            tb = tb.column(match ALL_COLS[c] {
                "App" => Column::initial(110.0).range(60.0..=widths.app_max).resizable(true).clip(true),
                "Host" => Column::initial(210.0).range(90.0..=widths.host_max).resizable(true).clip(true),
                "Path" => Column::remainder().at_least(PATH_MIN).clip(true),
                name => Column::exact(fixed_width(name)).clip(true),
            });
        }
        tb.header(HIT, |mut h| {
            for &c in &cols {
                let (_, r) = h.col(|ui| {
                    let hovered = ui.rect_contains_pointer(ui.max_rect());
                    let arrow = match sort {
                        Some((sc, asc)) if sc == c => (if asc { " ▲" } else { " ▼" }).to_string(),
                        // hover hint that the column is sortable, in place of a pointer cursor
                        _ if hovered => format!(" {}", egui_phosphor::regular::CARET_UP_DOWN),
                        _ => String::new(),
                    };
                    ui.label(overline(format!("{}{arrow}", ALL_COLS[c].to_uppercase())));
                });
                let r = r.on_hover_text("Click to sort");
                if r.clicked() {
                    out.push(RowIntent::Sort(c));
                }
            }
        })
        .body(|body| {
            let st = vm.store.lock();
            body.rows(HIT, vm.visible.len(), |mut row| {
                let f = &st.flows[vm.visible[row.index()]];
                let selected = vm.selected == Some(f.id);
                row.set_selected(selected);
                for &c in &cols {
                    row.col(|ui| match ALL_COLS[c] {
                        "#" => {
                            if selected {
                                let r = ui.max_rect();
                                let bar = egui::Rect::from_min_max(r.left_top(), egui::pos2(r.left() + 3.0, r.bottom()));
                                ui.painter().rect_filled(bar, 1.5, accent());
                            }
                            // room for the selection bar, kept on every row so ids don't shift
                            ui.add_space(4.0);
                            if status_dot_on {
                                status_dot(ui, f.status.map(status_color).unwrap_or_else(fg_muted));
                            }
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
                            let name = f.app_label();
                            // a flow whose own lookup failed borrows the icon of the same-named app
                            let own = f.display_identity();
                            let ident = own
                                .as_deref()
                                .filter(|i| !i.icon_key.is_empty())
                                .or_else(|| vm.app_ids.get(&name))
                                .or(own.as_deref());
                            let icon = app_icon(ui, &vm.icons, &name, ident);
                            if app_label_hover(icon, &name, ident).clicked() {
                                out.push(RowIntent::FilterApp(name.clone()));
                            }
                            ui.label(RichText::new(name).color(fg_muted()));
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
                        let c = menu_item(ui, l).clicked();
                        if c {
                            ui.close();
                        }
                        c
                    };
                    let host = f.hostname().to_string();
                    let (route_only, route_add) = (
                        ico(ph::ARROWS_SPLIT, "Route only this host through proxy"),
                        ico(ph::ARROWS_SPLIT, "Add to proxy-only list"),
                    );
                    let (replay, mute, only, only_path, exclude) = (
                        ico(ph::PLAY, &format!("Replay / edit…  ({}R)", cmd())),
                        ico(ph::EYE_SLASH, &format!("Mute {host}")),
                        ico(ph::TARGET, "Scope to this host"),
                        ico(ph::TARGET, "Scope to this URL path"),
                        ico(ph::EYE_SLASH, "Exclude this URL"),
                    );
                    menu_width(ui, &[&replay, &mute, &only, &only_path, &exclude, &route_only, &route_add, &ico(ph::SLIDERS_HORIZONTAL, "Create rule from this request")]);
                    if b(ui, &replay) {
                        out.push(RowIntent::Replay(id));
                    }
                    ui.menu_button(ico(ph::SLIDERS_HORIZONTAL, "Create rule from this request"), |ui| {
                        use RuleKind as K;
                        menu_width(ui, &["Mock this response", "Throttle / delay"]);
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
                        menu_width(ui, &["Purple"]);
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
                        menu_width(ui, &["Python requests"]);
                        for (l, s) in [("URL", f.url()), ("cURL", ex::curl(f)), ("fetch", ex::fetch(f)), ("Python requests", ex::python(f)), ("HTTPie", ex::httpie(f)), ("Raw HTTP", ex::raw_http(f))] {
                            if b(ui, l) {
                                ctx.copy_text(s);
                            }
                        }
                    });
                    if b(ui, &mute) {
                        out.push(RowIntent::Mute(host.clone()));
                    }
                    if b(ui, &only) {
                        out.push(RowIntent::Scope(host.clone()));
                    }
                    if b(ui, &only_path) {
                        out.push(RowIntent::ScopePath(id));
                    }
                    if b(ui, &route_only) {
                        out.push(RowIntent::ProxyOnlyHost(host.clone()));
                    }
                    if b(ui, &route_add) {
                        out.push(RowIntent::ProxyOnlyAdd(host.clone()));
                    }
                    if b(ui, &exclude) {
                        out.push(RowIntent::Exclude(id));
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
