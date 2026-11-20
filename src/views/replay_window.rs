use crate::services::decode;
use crate::viewmodels::replay_vm::ReplayVm;
use crate::views::body;
use crate::views::theme::*;
use eframe::egui::{self, RichText, TextEdit};
use egui_phosphor::regular as ph;

// returns the ids whose Send button was pressed
pub fn show(ctx: &egui::Context, vm: &mut ReplayVm) -> Vec<u64> {
    let mut send = vec![];
    for d in vm.drafts.iter_mut() {
        let title = if d.source.is_some() { format!("Replay #{}", d.source.unwrap_or(0)) } else { "Compose".into() };
        egui::Window::new(ico(ph::PLAY, &title)).id(egui::Id::unique(("replay", d.id))).open(&mut d.open).default_size([720.0, 640.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("m", d.id)).selected_text(&d.method).width(90.0).show_ui(ui, |ui| {
                    for m in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
                        ui.selectable_value(&mut d.method, m.to_string(), m);
                    }
                });
                ui.add(TextEdit::singleline(&mut d.url).desired_width(ui.available_width() - 90.0));
                let go = ui.add_enabled(!d.sending, egui::Button::new(RichText::new(ico(ph::PAPER_PLANE_TILT, "Send")).strong()).fill(GREEN.gamma_multiply(0.45)));
                if go.clicked() {
                    send.push(d.id);
                }
            });
            ui.label(RichText::new("Headers").strong());
            ui.add(TextEdit::multiline(&mut d.headers).code_editor().desired_rows(5).desired_width(f32::INFINITY));
            ui.label(RichText::new("Body").strong());
            ui.add(TextEdit::multiline(&mut d.body).code_editor().desired_rows(6).desired_width(f32::INFINITY));
            ui.separator();
            if d.sending {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Sending…");
                });
            }
            let (id, mut show_diff) = (d.id, d.show_diff);
            match &d.result {
                Some(Ok(s)) => {
                    ui.horizontal(|ui| {
                        pill(ui, &s.status.to_string(), status_color(s.status));
                        ui.label(muted(format!("{} ms · {}", s.ms, fmt_size(s.response.body.len()))));
                        if d.prev.is_some() {
                            ui.checkbox(&mut show_diff, "Diff against original response");
                        }
                    });
                    if show_diff {
                        if let Some(p) = &d.prev {
                            let lines = decode::diff_text(&decode::pretty_text(p), &decode::pretty_text(&s.response));
                            ui.allocate_ui(egui::vec2(ui.available_width(), 220.0), |ui| body::diff_view(ui, &format!("rp{id}"), &lines, true));
                        }
                    } else {
                        ui.allocate_ui(egui::vec2(ui.available_width(), 240.0), |ui| {
                            ui.set_min_height(240.0);
                            body::body_view(ui, &format!("rpb{id}"), &s.response, &mut d.mode);
                        });
                    }
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("{} {e}", ph::WARNING)).color(RED));
                }
                None => {}
            }
            d.show_diff = show_diff;
        });
    }
    send
}
