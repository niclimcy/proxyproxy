use crate::domain::flow::Stage;
use crate::viewmodels::intent::RuleIntent;
use crate::viewmodels::rules_vm::RulesVm;
use crate::views::theme::{
    PAD, accent, action_button_sized, canvas, cmd, danger, fg_muted, muted, overline, panel_frame,
    pill, warning,
};
use eframe::egui::{self, Frame, Margin, ScrollArea, TextEdit};
use egui_phosphor::regular as ph;

pub fn show(ctx: &egui::Context, vm: &mut RulesVm) -> Option<RuleIntent> {
    let total = vm.paused.len();
    let raise = std::mem::take(&mut vm.focus_breakpoint);
    let d = vm.paused.first_mut()?;
    let mut decision = None;
    let stage = match d.stage {
        Stage::Request => "Request",
        Stage::Response => "Response",
    };
    let builder = egui::ViewportBuilder::default()
        .with_title(format!("Breakpoint · {stage} #{}", d.id))
        .with_inner_size([900.0, 560.0])
        .with_min_inner_size([520.0, 360.0]);
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("breakpoint"),
        builder,
        |ui, _| {
            if raise {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            let (execute, abort, closed) = ui.input(|i| {
                let cmd = i.modifiers.command;
                (
                    cmd && i.key_pressed(egui::Key::Enter),
                    cmd && i.key_pressed(egui::Key::Period),
                    i.viewport().close_requested(),
                )
            });
            // closing must not strand the connection, so it lets traffic continue
            if execute || closed {
                decision = Some(true);
            } else if abort {
                decision = Some(false);
            }
            egui::Panel::top(egui::Id::unique("bp-top"))
                .frame(panel_frame().inner_margin(Margin::symmetric(14, 10)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        pill(ui, &format!("Paused · {stage}"), warning());
                        if total > 1 {
                            pill(ui, &format!("{} more waiting", total - 1), fg_muted());
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| match d.stage {
                        Stage::Request => {
                            ui.add(
                                TextEdit::singleline(&mut d.method)
                                    .font(egui::TextStyle::Monospace)
                                    .margin(Margin::symmetric(10, 6))
                                    .background_color(canvas())
                                    .desired_width(96.0),
                            );
                            ui.add(
                                TextEdit::singleline(&mut d.url)
                                    .font(egui::TextStyle::Monospace)
                                    .margin(Margin::symmetric(10, 6))
                                    .background_color(canvas())
                                    .desired_width(f32::INFINITY),
                            );
                        }
                        Stage::Response => {
                            ui.label(muted("Status"));
                            ui.add(
                                TextEdit::singleline(&mut d.status)
                                    .font(egui::TextStyle::Monospace)
                                    .margin(Margin::symmetric(10, 6))
                                    .background_color(canvas())
                                    .desired_width(80.0),
                            );
                        }
                    });
                });
            egui::Panel::bottom(egui::Id::unique("bp-actions"))
                .frame(panel_frame().inner_margin(Margin::symmetric(14, 10)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if action_button_sized(ui, ph::X, "Abort", danger(), 112.0, true)
                            .on_hover_text(format!(
                                "Drop this {} ({}.)",
                                stage.to_lowercase(),
                                cmd()
                            ))
                            .clicked()
                        {
                            decision = Some(false);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if action_button_sized(ui, ph::PLAY, "Execute", accent(), 120.0, true)
                                .on_hover_text(format!("Continue with your edits ({}Enter)", cmd()))
                                .clicked()
                            {
                                decision = Some(true);
                            }
                            ui.label(muted("Closing the window also continues"));
                        });
                    });
                });
            egui::CentralPanel::default()
                .frame(Frame::new().fill(canvas()).inner_margin(Margin::same(PAD)))
                .show(ui, |ui| {
                    ui.columns(2, |cols| {
                        editor(&mut cols[0], "HEADERS", "bp-h", &mut d.headers);
                        editor(&mut cols[1], "BODY", "bp-b", &mut d.body);
                    });
                });
        },
    );
    decision.map(RuleIntent::Resolve)
}

fn editor(ui: &mut egui::Ui, title: &str, salt: &str, text: &mut String) {
    ui.label(overline(title));
    ScrollArea::vertical()
        .id_salt(salt)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                TextEdit::multiline(text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .min_size(ui.available_size()),
            );
        });
}
