use crate::domain::flow::{Dir, Flow, Message};
use crate::services::app_icons::IconStore;
use crate::services::decode;
use crate::services::decoders::{ModeId, PRETTY, TREE};
use crate::services::exporter as ex;
use crate::viewmodels::detail_vm::{DTab, DetailVm, Side, raw_text, side_tabs};
use crate::viewmodels::intent::DetailIntent;
use crate::views::app_icon::{app_icon, app_label_hover};
use crate::views::body;
use crate::views::theme::{
    RADIUS, badge, canvas, card_panel, checkbox, cmd, danger, fg_muted, fmt_size, icon_button,
    icon_only, menu_item, menu_width, method_color, muted, overline, pill, resize_grip,
    resize_line, segmented, status_color, success, toolbar_menu, violet, warning,
};
use eframe::egui::{self, CornerRadius, Frame, Margin, RichText, ScrollArea, TextEdit};
use egui_phosphor::regular as ph;

/// Releasing the divider this close to the bottom closes the details; near the top it snaps fully open.
const SNAP_WITHIN: f32 = 80.0;

/// Table height left visible when the details are fully open.
const TABLE_PEEK: f32 = 112.0;

fn msg_view(
    ui: &mut egui::Ui,
    id: &str,
    first: &str,
    m: &Message,
    (side, mode): (&mut Side, &mut ModeId),
    params: &[(String, String)],
    vc: &decode::Ctx,
) {
    let items = side_tabs(side, !params.is_empty());
    segmented(ui, side, &items);
    ui.add_space(4.0);
    match *side {
        Side::Headers => body::headers_view(ui, id, m),
        Side::Body => body::body_view_ctx(ui, id, m, mode, vc),
        Side::Params => {
            ScrollArea::vertical()
                .id_salt(format!("{id}-q"))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    body::kv_table(ui, params.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                });
        }
        Side::Raw => {
            ScrollArea::both()
                .id_salt(format!("{id}-raw"))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut t = raw_text(first, m);
                    ui.add(
                        TextEdit::multiline(&mut t)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .frame(egui::Frame::NONE),
                    );
                });
        }
    }
}

fn timing_bar(ui: &mut egui::Ui, ms: u128, max: u128) {
    let w = ui.available_width().min(300.0);
    let (r, _) = ui.allocate_exact_size(egui::vec2(w, 8.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 4.0, canvas());
    let frac = (ms as f32 / max.max(1) as f32).clamp(0.02, 1.0);
    let color = if frac > 0.75 {
        danger()
    } else if frac > 0.4 {
        warning()
    } else {
        success()
    };
    ui.painter().rect_filled(
        egui::Rect::from_min_size(r.min, egui::vec2(w * frac, 8.0)),
        4.0,
        color,
    );
}

pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &mut DetailVm,
    icons: &IconStore,
    selected: Option<u64>,
    has_flows: bool,
) -> Vec<DetailIntent> {
    let mut out = vec![];
    if selected.and_then(|id| vm.flow(id)).is_none() {
        if !has_flows {
            return out;
        }
        egui::Panel::bottom("detail-empty").exact_size(56.0).show_separator_line(false).frame(card_panel()).show(ui, |ui| {
            ui.centered_and_justified(|ui| ui.label(muted(format!("Select a request to inspect it  ·  ↑/↓ navigate  ·  ⇧F10 actions  ·  {}F search", cmd()))));
        });
        return out;
    }
    resize_line(ui, true);
    // Dragging the panel to a sliver closes it; a fresh id drops the stored tiny size so it reopens at default.
    let epoch_id = ui.make_persistent_id("detail-epoch");
    let epoch: u32 = ui.data(|d| d.get_temp(epoch_id)).unwrap_or(0);
    let full_id = ui.make_persistent_id("detail-full");
    let full: bool = ui.data(|d| d.get_temp(full_id)).unwrap_or(false);
    let region = ui.available_rect_before_wrap();
    // Keeps the table header visible so the divider and borders never meet the toolbar.
    let max_h = (region.height() - TABLE_PEEK).max(SNAP_WITHIN);
    // Stays selected while sliding shut; Close is sent only once the slide finishes.
    let open_id = ui.make_persistent_id("detail-open");
    let mut expanded: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(true);
    let mut close_clicked = false;
    let Some(shown) = egui::Panel::bottom(("detail", epoch))
        .resizable(true)
        .size_range(SNAP_WITHIN..=max_h)
        .default_size(if full { max_h } else { 360.0 })
        .show_separator_line(false)
        .frame(card_panel())
        .show_collapsible(ui, &mut expanded, |ui| {
            resize_line(ui, false);
            let Some(f) = selected.and_then(|id| vm.flow(id)) else {
                return;
            };
            let url = f.url();
            Frame::new()
                .fill(canvas())
                .corner_radius(CornerRadius::same(RADIUS))
                .inner_margin(Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let name = f.app_label();
                        app_label_hover(
                            app_icon(ui, icons, &name, f.app_id.as_ref()),
                            &name,
                            f.app_id.as_ref(),
                        );
                        badge(
                            ui,
                            if f.is_ws { "WS" } else { &f.method },
                            if f.is_ws {
                                violet()
                            } else {
                                method_color(&f.method)
                            },
                            false,
                        );
                        if let Some(s) = f.status {
                            badge(ui, &s.to_string(), status_color(s), false);
                        }
                        if !f.tag.is_empty() {
                            pill(ui, &f.tag, violet());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_only(ui, ph::X, "Close details (Esc)").clicked() {
                                close_clicked = true;
                            }
                            if icon_only(ui, ph::COPY, "Copy URL").clicked() {
                                ctx.copy_text(url.clone());
                            }
                            // URL reads on from the badges; the copy button stays at the edge
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&url).monospace())
                                            .truncate(),
                                    )
                                    .on_hover_text(&url);
                                },
                            );
                        });
                    });
                });
            ui.horizontal(|ui| {
                let mut tabs = vec![
                    (DTab::Split, "Request / Response"),
                    (DTab::Overview, "Overview"),
                    (DTab::Diff, "Diff"),
                ];
                if f.is_ws {
                    tabs.push((DTab::Messages, "Messages"));
                }
                segmented(ui, &mut vm.tab, &tabs);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    toolbar_menu(ui, ph::COPY, "Copy as", false, |ui| {
                        menu_width(ui, &["Python requests"]);
                        for (l, s) in [
                            ("URL", f.url()),
                            ("cURL", ex::curl(&f)),
                            ("fetch", ex::fetch(&f)),
                            ("Python requests", ex::python(&f)),
                            ("HTTPie", ex::httpie(&f)),
                            ("Raw HTTP", ex::raw_http(&f)),
                        ] {
                            if menu_item(ui, l, None).clicked() {
                                ctx.copy_text(s);
                                ui.close();
                            }
                        }
                    });
                    if icon_button(ui, ph::PLAY, "Replay / edit", true).clicked() {
                        out.push(DetailIntent::Replay(f.id));
                    }
                    if icon_button(
                        ui,
                        ph::PUSH_PIN,
                        if f.pinned { "Unpin" } else { "Pin" },
                        true,
                    )
                    .clicked()
                    {
                        out.push(DetailIntent::TogglePin(f.id));
                    }
                });
            });
            ui.add_space(4.0);
            match vm.tab {
                DTab::Split => split_ui(ui, vm, &f),
                DTab::Overview => overview_ui(ui, vm, &f, &mut out),
                DTab::Diff => diff_ui(ui, vm, &f, &mut out),
                DTab::Messages => ws_ui(ui, vm, &f, &mut out),
            }
        })
    else {
        // Fully slid shut. A fresh id drops the stored tiny size so it reopens at default.
        ui.data_mut(|d| {
            d.insert_temp(epoch_id, epoch + 1);
            d.insert_temp(full_id, false);
            d.insert_temp(open_id, true);
        });
        out.push(DetailIntent::Close);
        return out;
    };
    // egui clears `expanded` when a drag crosses the minimum size
    ui.data_mut(|d| d.insert_temp(open_id, expanded && !close_clicked));
    resize_line(ui, false);
    let r = shown.response.rect;
    // Only a drag that actually resized the panel can snap it open.
    let drag_id = ui.make_persistent_id("detail-drag");
    let prev_h_id = ui.make_persistent_id("detail-prev-h");
    let (down, pos) = ui.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
    let prev_h: f32 = ui.data(|d| d.get_temp(prev_h_id)).unwrap_or(r.height());
    let mut dragging: bool = ui.data(|d| d.get_temp(drag_id)).unwrap_or(false);
    dragging |= down && (r.height() - prev_h).abs() > 0.5;
    if dragging && !down {
        dragging = false;
        if pos.is_some_and(|p| p.y < region.top() + SNAP_WITHIN) {
            // New id so default_size applies again, now at full height
            ui.data_mut(|d| {
                d.insert_temp(epoch_id, epoch + 1);
                d.insert_temp(full_id, true);
            });
        }
    }
    ui.data_mut(|d| {
        d.insert_temp(drag_id, dragging);
        d.insert_temp(prev_h_id, r.height());
    });
    resize_grip(
        ui,
        "detail",
        egui::Rect::from_two_pos(r.left_top(), r.right_top()),
    );
    out
}

fn split_ui(ui: &mut egui::Ui, vm: &mut DetailVm, f: &Flow) {
    let (rs, ss, rm, sm) = (
        &mut vm.req_side,
        &mut vm.res_side,
        &mut vm.req_mode,
        &mut vm.res_mode,
    );
    let gql_req = decode::is_graphql_request(&f.method, &f.path, &f.request);
    let (rc, sc) = (
        decode::Ctx {
            method: &f.method,
            path: &f.path,
            response: false,
            gql_req,
        },
        decode::Ctx {
            method: &f.method,
            path: &f.path,
            response: true,
            gql_req,
        },
    );
    ui.columns(2, |cols| {
        cols[0].label(overline("REQUEST"));
        msg_view(
            &mut cols[0],
            "req",
            &format!("{} {} {}", f.method, f.path, f.version),
            &f.request,
            (rs, rm),
            &decode::query_params(&f.path),
            &rc,
        );
        match &f.response {
            Some(r) => {
                let s = f.status.unwrap_or(0);
                cols[1].label(overline(format!(
                    "RESPONSE  ·  {s}  ·  {} ms  ·  {}",
                    f.duration_ms.unwrap_or(0),
                    fmt_size(r.body.len())
                )));
                msg_view(
                    &mut cols[1],
                    "res",
                    &format!("{} {s}", f.version),
                    r,
                    (ss, sm),
                    &[],
                    &sc,
                );
            }
            None => {
                if let Some(e) = &f.error {
                    cols[1].label(RichText::new(format!("{} {e}", ph::WARNING)).color(danger()));
                } else if f.is_ws {
                    cols[1].label(
                        RichText::new(format!(
                            "WebSocket · {} messages — see Messages tab",
                            f.ws.len()
                        ))
                        .color(violet()),
                    );
                } else {
                    cols[1].label(RichText::new("Waiting for response…").color(fg_muted()));
                }
            }
        }
    });
}

fn overview_ui(ui: &mut egui::Ui, vm: &DetailVm, f: &Flow, out: &mut Vec<DetailIntent>) {
    let ov = vm.overview(f);
    let mut comment = f.comment.clone();
    ScrollArea::vertical()
        .id_salt("overview")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("ov")
                .num_columns(2)
                .spacing([20.0, 7.0])
                .show(ui, |ui| {
                    let row = |ui: &mut egui::Ui, k: &str, v: String| {
                        ui.label(muted(k));
                        ui.label(RichText::new(v).monospace());
                        ui.end_row();
                    };
                    row(ui, "URL", f.url());
                    row(ui, "Client", f.app_label());
                    row(
                        ui,
                        "Protocol",
                        format!("{} · {}", f.version, f.scheme.to_uppercase()),
                    );
                    row(ui, "Request size", fmt_size(f.request.body.len()));
                    row(ui, "Response size", fmt_size(f.res_size()));
                    if let Some(r) = &f.response {
                        row(
                            ui,
                            "Content-Type",
                            r.header("content-type").unwrap_or("-").into(),
                        );
                        if let Some(cl) = r.header("cache-control") {
                            row(ui, "Cache-Control", cl.into());
                        }
                    }
                    ui.label(muted("Duration"));
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(
                                f.duration_ms
                                    .map_or("pending".into(), |d| format!("{d} ms")),
                            )
                            .monospace(),
                        );
                        if let Some(d) = f.duration_ms {
                            timing_bar(ui, d, ov.max_ms);
                        }
                    });
                    ui.end_row();
                    if let Some(e) = &f.error {
                        row(ui, "Error", e.clone());
                    }
                });
            let mut tokens = decode::jwts(&f.request);
            if let Some(r) = &f.response {
                tokens.extend(decode::jwts(r));
            }
            tokens.dedup();
            for t in tokens.iter().take(3) {
                if let Some((_, p)) = decode::decode_jwt(t) {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("{} JWT claims", ph::KEY))
                            .strong()
                            .color(violet()),
                    );
                    let mut s = serde_json::to_string_pretty(&p).unwrap_or_default();
                    ui.add(
                        TextEdit::multiline(&mut s)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                }
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Comment").strong());
            if ui
                .add(
                    TextEdit::multiline(&mut comment)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text("Notes about this request"),
                )
                .changed()
            {
                out.push(DetailIntent::SetComment(f.id, comment.clone()));
            }
            if !ov.same.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "Same endpoint · {} other call{}",
                        ov.same.len(),
                        if ov.same.len() == 1 { "" } else { "s" }
                    ))
                    .strong(),
                );
                for o in ov.same.iter().rev().take(8) {
                    ui.horizontal(|ui| {
                        ui.label(muted(format!("#{}", o.id)));
                        if let Some(s) = o.status {
                            pill(ui, &s.to_string(), status_color(s));
                        }
                        ui.label(muted(o.duration_ms.map_or_default(|d| format!("{d} ms"))));
                        if icon_button(ui, ph::GIT_DIFF, "Diff vs this", true).clicked() {
                            out.push(DetailIntent::DiffAgainst(o.id));
                        }
                    });
                }
            }
        });
}

fn diff_ui(ui: &mut egui::Ui, vm: &mut DetailVm, f: &Flow, out: &mut Vec<DetailIntent>) {
    let Some(base) = vm.diff_base_for(f) else {
        ui.label(muted("No earlier call to the same endpoint. Right-click any request → \"Use as diff base\", then select another."));
        return;
    };
    ui.horizontal(|ui| {
        ui.label(muted(format!("Comparing #{} → #{}", base.id, f.id)));
        checkbox(ui, &mut vm.diff_only_changes, "Only changes");
        if vm.diff_base.is_some() && icon_button(ui, ph::X, "Clear base", true).clicked() {
            out.push(DetailIntent::ClearBase);
        }
    });
    let only = vm.diff_only_changes;
    let lines = vm.diff_lines(&base, f);
    if lines.iter().all(|(t, _)| *t == ' ') {
        ui.label(RichText::new("Identical").color(success()));
    }
    body::diff_view(ui, "dd", lines, only);
}

fn ws_ui(ui: &mut egui::Ui, vm: &DetailVm, f: &Flow, out: &mut Vec<DetailIntent>) {
    if f.ws.is_empty() {
        ui.label(muted("No messages yet"));
        return;
    }
    let sel = vm.ws_sel;
    ui.columns(2, |cols| {
        ScrollArea::vertical()
            .id_salt("ws-list")
            .stick_to_bottom(sel.is_none())
            .auto_shrink([false, false])
            .show_rows(&mut cols[0], 22.0, f.ws.len(), |ui, range| {
                for i in range {
                    let m = &f.ws[i];
                    let (arrow, c) = if m.dir == Dir::Out {
                        (ph::ARROW_UP, warning())
                    } else {
                        (ph::ARROW_DOWN, success())
                    };
                    let preview: String = if m.binary {
                        format!("<{} bytes>", m.data.len())
                    } else {
                        String::from_utf8_lossy(&m.data).chars().take(80).collect()
                    };
                    if ui
                        .selectable_label(
                            sel == Some(i),
                            RichText::new(format!("{arrow} {preview}"))
                                .monospace()
                                .color(c),
                        )
                        .clicked()
                    {
                        out.push(DetailIntent::SelectWs(i));
                    }
                }
            });
        if let Some(m) = sel.and_then(|i| f.ws.get(i)) {
            let msg = Message {
                headers: vec![],
                body: m.data.clone(),
            };
            let mut mode = if decode::is_json(&msg) { TREE } else { PRETTY };
            body::body_view(&mut cols[1], "ws", &msg, &mut mode);
        } else {
            cols[1].label(muted("Select a message"));
        }
    });
}
