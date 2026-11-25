use crate::services::decode;
use crate::viewmodels::replay_vm::{DraftTab, ReplayDraft, ReplayVm};
use crate::views::body;
use crate::views::theme::{
    PAD, accent, action_button_sized, canvas, checkbox, cmd, danger, fmt_size, muted, overline,
    panel_frame, pill, segmented, status_color,
};
use eframe::egui::{self, Frame, Margin, RichText, ScrollArea, TextEdit};
use egui_phosphor::regular as ph;

const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

// returns the ids whose Send button was pressed
pub fn show(ctx: &egui::Context, vm: &mut ReplayVm) -> Vec<u64> {
    let mut send = vec![];
    for d in &mut vm.drafts {
        let title = match d.source {
            Some(src) => format!("Replay #{src}"),
            None => "Compose".into(),
        };
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([980.0, 620.0])
            .with_min_inner_size([560.0, 360.0]);
        let vid = egui::ViewportId::from_hash_of(("replay", d.id));
        ctx.show_viewport_immediate(vid, builder, |ui, _| {
            let (submit, close) = ui.input(|i| {
                let cmd = i.modifiers.command;
                (
                    cmd && i.key_pressed(egui::Key::Enter),
                    cmd && i.key_pressed(egui::Key::W) || i.viewport().close_requested(),
                )
            });
            if close {
                d.open = false;
            }
            if request_bar(ui, d) || (submit && !d.sending) {
                send.push(d.id);
            }
            egui::CentralPanel::default()
                .frame(Frame::new().fill(canvas()).inner_margin(Margin::same(PAD)))
                .show(ui, |ui| {
                    ui.columns(2, |cols| {
                        request_pane(&mut cols[0], d);
                        response_pane(&mut cols[1], d);
                    });
                });
        });
    }
    send
}

fn request_bar(ui: &mut egui::Ui, d: &mut ReplayDraft) -> bool {
    let mut go = false;
    egui::Panel::top(egui::Id::unique(("replay-bar", d.id)))
        .frame(panel_frame().inner_margin(Margin::symmetric(14, 10)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("m", d.id))
                    .selected_text(RichText::new(&d.method).monospace().strong())
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for m in METHODS {
                            ui.selectable_value(&mut d.method, m.to_string(), m);
                        }
                    });
                let send_w = 104.0;
                let url_w = ui.available_width() - send_w - ui.spacing().item_spacing.x;
                ui.add(
                    TextEdit::singleline(&mut d.url)
                        .font(egui::TextStyle::Monospace)
                        .margin(Margin::symmetric(10, 6))
                        .background_color(canvas())
                        .desired_width(url_w),
                );
                go = action_button_sized(
                    ui,
                    ph::PAPER_PLANE_TILT,
                    "Send",
                    accent(),
                    send_w,
                    !d.sending,
                )
                .on_hover_text(format!("Send ({}Enter)", cmd()))
                .clicked();
            });
        });
    go
}

fn request_pane(ui: &mut egui::Ui, d: &mut ReplayDraft) {
    ui.label(overline("REQUEST"));
    segmented(
        ui,
        &mut d.tab,
        &[(DraftTab::Headers, "Headers"), (DraftTab::Body, "Body")],
    );
    ui.add_space(4.0);
    let (text, hint) = match d.tab {
        DraftTab::Headers => (&mut d.headers, "Name: value, one per line"),
        DraftTab::Body => (&mut d.body, "Request body"),
    };
    ScrollArea::vertical()
        .id_salt(("rq", d.id, d.tab == DraftTab::Body))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                TextEdit::multiline(text)
                    .code_editor()
                    .hint_text(hint)
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size()),
            );
        });
}

fn response_pane(ui: &mut egui::Ui, d: &mut ReplayDraft) {
    let id = d.id;
    match &d.result {
        Some(Ok(s)) => {
            ui.horizontal(|ui| {
                ui.label(overline("RESPONSE"));
                pill(ui, &s.status.to_string(), status_color(s.status));
                ui.label(muted(format!(
                    "{} ms · {}",
                    s.ms,
                    fmt_size(s.response.body.len())
                )));
                if d.prev.is_some() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        checkbox(ui, &mut d.show_diff, "Diff with original");
                    });
                }
            });
            match (&d.prev, d.show_diff) {
                (Some(p), true) => {
                    let lines = decode::diff_text(
                        &decode::pretty_text(p),
                        &decode::pretty_text(&s.response),
                    );
                    body::diff_view(ui, &format!("rp{id}"), &lines, true);
                }
                _ => body::body_view(ui, &format!("rpb{id}"), &s.response, &mut d.mode),
            }
        }
        Some(Err(e)) => {
            ui.label(overline("RESPONSE"));
            ui.label(RichText::new(format!("{} {e}", ph::WARNING)).color(danger()));
        }
        None => {
            ui.label(overline("RESPONSE"));
            ui.add_space(8.0);
            if d.sending {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(muted("Sending…"));
                });
            } else {
                ui.label(muted(format!(
                    "Send the request ({}Enter) to see the response here.",
                    cmd()
                )));
            }
        }
    }
}
