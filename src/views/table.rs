use crate::domain::flow::Flow;
use crate::domain::keymap::{Binding, RowAction};
use crate::viewmodels::capture_vm::{ALL_COLS, CaptureVm};
use crate::viewmodels::intent::{MockTarget, RowIntent};
use crate::viewmodels::rules_vm::RuleKind;
use crate::views::app_icon::{app_icon, app_label_hover};
use crate::views::theme::{
    FS_CAPTION, FS_TITLE, HIT, PAD, accent, action_button, action_button_sized, badge, card,
    clipped_card, fg_muted, fmt_size, ico, menu_item, menu_width, method_color, muted, overline,
    pill, status_color, status_dot, submenu_item, success, tag_colors, violet, warning,
};
use eframe::egui::{self, RichText};
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
        let app_max = if show_app {
            (flexible * 0.18).max(60.0)
        } else {
            0.0
        };
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
    vm: &mut CaptureVm,
    diff_base: Option<u64>,
    port: u16,
    collections: &dyn Fn() -> Vec<(u64, String)>,
) -> Vec<RowIntent> {
    let open_menu = std::mem::take(&mut vm.row_menu);
    let vm = &*vm;
    let keymap = vm.keymap();
    let mockable_visible = || vm.mockable_visible();
    let mock = MockMenu {
        collections,
        mockable_visible: &mockable_visible,
    };
    let mut out = vec![];
    let hidden = vm.hidden_cols();
    let cols: Vec<usize> = (0..ALL_COLS.len())
        .filter(|&i| i == 0 || !hidden.iter().any(|h| h == ALL_COLS[i]))
        .collect();
    let follow = vm.follow && vm.sort.is_none();
    let sort = vm.sort;
    let has_flows = vm.has_flows();

    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| clipped_card(ui, |ui| {
        if !has_flows {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() - 280.0).max(24.0) / 2.0);
                let card_width = ui.clip_rect().width().min(500.0);
                card().show(ui, |ui| {
                    ui.set_width((card_width - 2.0 * f32::from(PAD)).max(0.0));
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
                let card_width = ui.clip_rect().width().min(520.0);
                card().show(ui, |ui| {
                    // fixed width: a long filter would stretch a min-width card off-center
                    ui.set_width((card_width - 2.0 * f32::from(PAD)).max(0.0));
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
        let mut tb = TableBuilder::new(ui).striped(true).sense(egui::Sense::click()).cell_layout(egui::Layout::left_to_right(egui::Align::Center)).min_scrolled_height(0.0).stick_to_bottom(follow);
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
            let (mut selected_row, mut stray_focus) = (None, false);
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
                                let label = f.status.map_or_else(|| "Pending".to_string(), |s| format!("Status {s}"));
                                status_dot(ui, f.status.map_or_else(fg_muted, status_color), &label);
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
                                .or_else(|| vm.app_ids.get(&*name))
                                .or(own.as_deref());
                            let icon = app_icon(ui, &vm.icons, &name, ident);
                            if app_label_hover(icon, &name, ident).clicked() {
                                out.push(RowIntent::FilterApp(name.to_string()));
                            }
                            ui.label(RichText::new(name).color(fg_muted()));
                        }
                        "Method" => {
                            badge(ui, if f.is_ws { "WS" } else { &f.method }, if f.is_ws { violet() } else { method_color(&f.method) }, selected);
                        }
                        "Status" => match f.status {
                            Some(s) => {
                                badge(ui, &s.to_string(), status_color(s), selected);
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
                            ui.label(RichText::new(f.duration_ms.map_or_default(|d| format!("{d} ms"))).monospace().color(fg_muted()));
                        }
                        _ => {
                            ui.label(RichText::new(f.response.as_ref().map_or_default(|r| fmt_size(r.body.len()))).monospace().color(fg_muted()));
                        }
                    });
                }
                let resp = row.response();
                resp.widget_info(|| {
                    let status = f.status.map_or_else(|| "pending".to_string(), |s| s.to_string());
                    egui::WidgetInfo::selected(egui::Role::Row, true, selected, format!("{} {} {status}", f.method, f.url()))
                });
                if resp.clicked() {
                    out.push(RowIntent::Select(f.id));
                }
                resp.context_menu(|ui| row_menu(ui, f, false, &keymap, &mock, &mut out));
                if !selected {
                    stray_focus |= resp.has_focus();
                    return;
                }
                selected_row = Some(resp.id);
                // anchored to the row: the right-click menu opens at the pointer, which a keyboard user doesn't have
                let menu = egui::Popup::from_response(&resp)
                    .id(resp.id.with("keyboard_menu"))
                    .kind(egui::PopupKind::Menu)
                    .layout(egui::Layout::top_down_justified(egui::Align::Min))
                    .style(egui::containers::menu::menu_style)
                    .open_memory(open_menu.then_some(egui::SetOpenCommand::Bool(true)));
                let menu_id = menu.get_id();
                let focus_first = menu_id.with("focus_first");
                if open_menu {
                    ctx.data_mut(|d| d.insert_temp(focus_first, true));
                }
                let was_open = menu.is_open();
                menu.show(|ui| {
                    // the first pass is a non-interactive sizing pass, so focus waits for the real one
                    let focus = !ui.is_sizing_pass() && ctx.data_mut(|d| d.remove_temp::<bool>(focus_first)).unwrap_or(false);
                    row_menu(ui, f, focus, &keymap, &mock, &mut out);
                });
                if was_open && !egui::Popup::is_id_open(ctx, menu_id) {
                    resp.request_focus();
                }
                if resp.has_focus() {
                    // the arrows step the selection; egui would otherwise move focus to a neighbouring widget
                    let filter = egui::EventFilter { vertical_arrows: true, ..Default::default() };
                    ctx.memory_mut(|m| m.set_focus_lock_filter(resp.id, filter));
                }
            });
            // focus follows the selection when the arrows step away from a focused row; only then, so Tab can leave the table
            let last = egui::Id::unique("flow_table_selection");
            let stepped = ctx.data_mut(|d| {
                let prev = d.get_temp::<Option<u64>>(last);
                d.insert_temp(last, vm.selected);
                prev.is_some_and(|p| p != vm.selected)
            });
            if let (true, true, Some(id)) = (stray_focus, stepped, selected_row) {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        });
    }));
    out
}

/// Looked up only while the "Mock response" submenu is open.
struct MockMenu<'a> {
    collections: &'a dyn Fn() -> Vec<(u64, String)>,
    mockable_visible: &'a dyn Fn() -> usize,
}

/// Row actions, shared by right-click and the keyboard.
fn row_menu(
    ui: &mut egui::Ui,
    f: &Flow,
    focus_first: bool,
    keymap: &[(RowAction, Binding)],
    mock: &MockMenu,
    out: &mut Vec<RowIntent>,
) {
    use RowAction as A;
    let hint = |a: A| crate::views::shortcuts::hint(a, keymap);
    let id = f.id;
    let item = |ui: &mut egui::Ui, l: &str, action: Option<A>| {
        let r = menu_item(ui, l, action.and_then(hint).as_deref());
        if r.clicked() {
            ui.close();
        }
        r
    };
    let b = |ui: &mut egui::Ui, l: &str, action: Option<A>| item(ui, l, action).clicked();
    let host = f.hostname().to_string();
    let labels = [
        (ico(ph::PLAY, "Replay / edit…"), Some(A::Replay)),
        (
            ico(ph::SLIDERS_HORIZONTAL, "Create rule from this request"),
            None,
        ),
        (
            ico(ph::PUSH_PIN, if f.pinned { "Unpin" } else { "Pin" }),
            Some(A::Pin),
        ),
        (ico(ph::NOTE, "Comment…"), Some(A::Comment)),
        (ico(ph::GIT_DIFF, "Use as diff base"), Some(A::DiffBase)),
        (ico(ph::EYE_SLASH, &format!("Mute {host}")), Some(A::Mute)),
        (ico(ph::TARGET, "Scope to this host"), None),
        (ico(ph::TARGET, "Scope to this URL path"), None),
        (
            ico(ph::ARROWS_SPLIT, "Route only this host through proxy"),
            None,
        ),
        (ico(ph::ARROWS_SPLIT, "Add to proxy-only list"), None),
        (ico(ph::EYE_SLASH, "Exclude this URL"), None),
        (ico(ph::LOCK_OPEN, "Don't decrypt this host"), None),
        (ico(ph::TRASH, "Delete"), Some(A::Delete)),
    ];
    // room for the hint column, so a long label never runs into its shortcut
    let widths: Vec<String> = labels
        .iter()
        .map(|(l, a)| format!("{l}      {}", a.and_then(hint).unwrap_or_default()))
        .collect();
    menu_width(ui, &widths.iter().map(String::as_str).collect::<Vec<_>>());
    let [
        replay,
        rule,
        pin,
        comment,
        diff,
        mute,
        only,
        only_path,
        route_only,
        route_add,
        exclude,
        no_tls,
        delete,
    ] = labels;

    let first = item(ui, &replay.0, replay.1);
    if focus_first {
        first.request_focus();
    }
    if first.clicked() {
        out.push(RowIntent::Replay(id));
    }
    submenu_item(ui, &rule.0, |ui| {
        use RuleKind as K;
        menu_width(ui, &["Mock this response", "Throttle / delay"]);
        for (k, l) in [
            (K::Breakpoint, "Breakpoint"),
            (K::Mock, "Mock this response"),
            (K::Block, "Block"),
            (K::Throttle, "Throttle / delay"),
            (K::Redirect, "Redirect to…"),
            (K::Header, "Rewrite header"),
            (K::Script, "Script"),
        ] {
            if b(ui, l, None) {
                out.push(RowIntent::Rule(id, k));
            }
        }
    });
    let mock_label = ico(ph::FLASK, "Mock response");
    if let Some(why) = f.mock_problem() {
        ui.add_enabled_ui(false, |ui| menu_item(ui, &mock_label, None))
            .inner
            .on_disabled_hover_text(why);
    } else {
        submenu_item(ui, &mock_label, |ui| {
            let collections = (mock.collections)();
            let all = (mock.mockable_visible)();
            let mock_all = format!("Mock all {all} visible…");
            let names: Vec<&str> = collections.iter().map(|(_, n)| n.as_str()).collect();
            let fixed = ["New collection…", "No collection", &mock_all];
            menu_width(ui, &[&fixed[..], &names[..]].concat());
            if b(ui, fixed[0], None) {
                out.push(RowIntent::Mock(id, MockTarget::NewCollection));
            }
            for (cid, name) in &collections {
                if b(ui, name, None) {
                    out.push(RowIntent::Mock(id, MockTarget::Collection(*cid)));
                }
            }
            if b(ui, fixed[1], None) {
                out.push(RowIntent::Mock(id, MockTarget::NoCollection));
            }
            if all > 1 {
                ui.separator();
                if b(ui, &mock_all, None) {
                    out.push(RowIntent::MockVisible);
                }
            }
        });
    }
    ui.separator();
    if b(ui, &pin.0, pin.1) {
        out.push(RowIntent::Pin(id));
    }
    submenu_item(ui, &ico(ph::PALETTE, "Color"), |ui| {
        menu_width(
            ui,
            &[&format!(
                "Purple      {}",
                hint(A::Color(None)).unwrap_or_default()
            )],
        );
        if b(ui, "None", Some(A::Color(None))) {
            out.push(RowIntent::Color(id, None));
        }
        for (i, (n, _)) in tag_colors().iter().enumerate() {
            let c = u8::try_from(i).ok();
            if b(ui, n, Some(A::Color(c))) {
                out.push(RowIntent::Color(id, c));
            }
        }
    });
    if b(ui, &comment.0, comment.1) {
        out.push(RowIntent::Comment(id));
    }
    if b(ui, &diff.0, diff.1) {
        out.push(RowIntent::DiffBase(id));
    }
    ui.separator();
    submenu_item(ui, &ico(ph::COPY, "Copy as"), |ui| {
        use crate::services::exporter as ex;
        menu_width(ui, &["Python requests"]);
        for (l, s) in [
            ("URL", f.url()),
            ("cURL", ex::curl(f)),
            ("fetch", ex::fetch(f)),
            ("Python requests", ex::python(f)),
            ("HTTPie", ex::httpie(f)),
            ("Raw HTTP", ex::raw_http(f)),
        ] {
            if b(ui, l, None) {
                ui.ctx().copy_text(s);
            }
        }
    });
    if b(ui, &mute.0, mute.1) {
        out.push(RowIntent::Mute(host.clone()));
    }
    if b(ui, &only.0, None) {
        out.push(RowIntent::Scope(host.clone()));
    }
    if b(ui, &only_path.0, None) {
        out.push(RowIntent::ScopePath(id));
    }
    if b(ui, &route_only.0, None) {
        out.push(RowIntent::ProxyOnlyHost(host.clone()));
    }
    if b(ui, &route_add.0, None) {
        out.push(RowIntent::ProxyOnlyAdd(host.clone()));
    }
    if b(ui, &exclude.0, None) {
        out.push(RowIntent::Exclude(id));
    }
    if b(ui, &no_tls.0, None) {
        out.push(RowIntent::NoTls(host));
    }
    ui.separator();
    if b(ui, &delete.0, delete.1) {
        out.push(RowIntent::Delete(id));
    }
}
