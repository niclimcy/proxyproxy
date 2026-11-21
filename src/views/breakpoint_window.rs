use crate::domain::flow::Stage;
use crate::viewmodels::intent::RuleIntent;
use crate::viewmodels::rules_vm::RulesVm;
use crate::views::theme::*;
use eframe::egui::{self, RichText, ScrollArea, TextEdit};
use egui_phosphor::regular as ph;

pub fn show(ctx: &egui::Context, vm: &mut RulesVm) -> Option<RuleIntent> {
    let total = vm.paused.len();
    let d = vm.paused.first_mut()?;
    let mut decision = None;
    let title = match d.stage {
        Stage::Request => ico(ph::PAUSE_CIRCLE, "Breakpoint · Request"),
        Stage::Response => ico(ph::PAUSE_CIRCLE, "Breakpoint · Response"),
    };
    egui::Window::new(title)
        .default_size([640.0, 520.0])
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                pill(ui, &format!("#{}", d.id), warning());
                if total > 1 {
                    pill(ui, &format!("{} more waiting", total - 1), fg_muted());
                }
            });
            ui.add_space(4.0);
            match d.stage {
                Stage::Request => {
                    ui.horizontal(|ui| {
                        ui.add(TextEdit::singleline(&mut d.method).desired_width(80.0));
                        ui.add(TextEdit::singleline(&mut d.url).desired_width(f32::INFINITY));
                    });
                }
                Stage::Response => {
                    ui.horizontal(|ui| {
                        ui.label("Status");
                        ui.add(TextEdit::singleline(&mut d.status).desired_width(70.0));
                    });
                }
            }
            ui.label(RichText::new("Headers").strong());
            ui.add(
                TextEdit::multiline(&mut d.headers)
                    .code_editor()
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            );
            ui.label(RichText::new("Body").strong());
            ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.add(
                    TextEdit::multiline(&mut d.body)
                        .code_editor()
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add(action_button(ico(ph::PLAY, "Execute"), success()))
                    .clicked()
                {
                    decision = Some(true);
                }
                if ui
                    .add(action_button(ico(ph::X, "Abort"), danger()))
                    .clicked()
                {
                    decision = Some(false);
                }
            });
        });
    decision.map(RuleIntent::Resolve)
}
