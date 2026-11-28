use crate::domain::flow::Stage;
use crate::viewmodels::intent::RuleIntent;
use crate::viewmodels::rules_vm::{BpDraft, RulesVm};
use crate::views::theme::{
    GAP, accent, action_button_sized, canvas, cmd, danger, fg_muted, muted, overline, pane, pill,
    warning, window_frame,
};
use eframe::egui::{self, Margin, ScrollArea, TextEdit};
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
            // top/bottom/central share one backdrop; each drops the margin facing its neighbour
            // so card gaps stay `GAP` everywhere
            let half = GAP / 2;
            let edge = |top, bottom| {
                window_frame().inner_margin(Margin {
                    left: half,
                    right: half,
                    top,
                    bottom,
                })
            };
            egui::Panel::top(egui::Id::unique("bp-top"))
                .frame(edge(half, 0))
                .show_separator_line(false)
                .show(ui, |ui| {
                    pane(ui, |ui| top_pane(ui, d, stage, total));
                });
            egui::Panel::bottom(egui::Id::unique("bp-actions"))
                .frame(edge(0, half))
                .show_separator_line(false)
                .show(ui, |ui| {
                    pane(ui, |ui| actions_pane(ui, stage, &mut decision));
                });
            egui::CentralPanel::default()
                .frame(edge(0, 0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    ui.columns(2, |cols| {
                        pane(&mut cols[0], |ui| {
                            editor(ui, "HEADERS", "bp-h", &mut d.headers);
                        });
                        pane(&mut cols[1], |ui| {
                            editor(ui, "BODY", "bp-b", &mut d.body);
                        });
                    });
                });
        },
    );
    decision.map(RuleIntent::Resolve)
}

fn top_pane(ui: &mut egui::Ui, d: &mut BpDraft, stage: &str, total: usize) {
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
}

fn actions_pane(ui: &mut egui::Ui, stage: &str, decision: &mut Option<bool>) {
    ui.horizontal(|ui| {
        if action_button_sized(ui, ph::X, "Abort", danger(), 112.0, true)
            .on_hover_text(format!("Drop this {} ({}.)", stage.to_lowercase(), cmd()))
            .clicked()
        {
            *decision = Some(false);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action_button_sized(ui, ph::PLAY, "Execute", accent(), 120.0, true)
                .on_hover_text(format!("Continue with your edits ({}Enter)", cmd()))
                .clicked()
            {
                *decision = Some(true);
            }
            ui.label(muted("Closing the window also continues"));
        });
    });
}

fn editor(ui: &mut egui::Ui, title: &str, salt: &str, text: &mut String) {
    ui.set_min_size(ui.available_size());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::domain::flow::{BpEdit, Decision, Paused};
    use crate::services::flow_store::FlowStore;
    use crate::views::layout_test::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn the_breakpoint_window_fits_its_minimum_size() {
        let ctx = ctx();
        // 520 px less the window frame; the editors split it
        let (full, half) = (520.0 - 6.0, (520.0 - 6.0) / 2.0);
        let mut leaks = vec![];
        let mut check = |what: String, width: f32, used: f32| {
            if used > width + 1.0 {
                leaks.push(format!("{what} needs {used} px of {width}"));
            }
        };
        for stage in [Stage::Request, Stage::Response] {
            let mut vm = RulesVm::new(
                Arc::new(Mutex::new(Config::default())),
                FlowStore::default(),
            );
            let (reply, _rx) = tokio::sync::oneshot::channel::<Decision>();
            vm.pause(Paused {
                id: 99_999_999,
                stage,
                edit: BpEdit {
                    method: "OPTIONS".into(),
                    url: url(),
                    status: 200,
                    headers: vec![("X-Long".into(), format!("{WORDS}{WORDS}"))],
                    body: bytes::Bytes::from(format!("{WORDS}{WORDS}")),
                },
                reply,
            });
            let d = &mut vm.paused[0];
            let name = if stage == Stage::Request {
                "Request"
            } else {
                "Response"
            };
            check(
                format!("the {name} fields"),
                full,
                used_width(&ctx, full, |ui| {
                    pane(ui, |ui| top_pane(ui, d, name, 3));
                }),
            );
            check(
                format!("the {name} actions"),
                full,
                used_width(&ctx, full, |ui| {
                    pane(ui, |ui| actions_pane(ui, name, &mut None));
                }),
            );
            for (title, salt) in [("HEADERS", "bp-h"), ("BODY", "bp-b")] {
                let mut text = format!("{WORDS}{WORDS}");
                check(
                    format!("the {title} editor"),
                    half,
                    used_width(&ctx, half, |ui| {
                        pane(ui, |ui| editor(ui, title, salt, &mut text));
                    }),
                );
            }
        }
        assert!(leaks.is_empty(), "{}", leaks.join("\n"));
    }
}
