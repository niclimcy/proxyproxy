use crate::domain::about;
use crate::viewmodels::about_vm::AboutVm;
use crate::views::theme::{FS_TITLE, PAD, muted, panel};
use eframe::egui::{self, Frame, Margin, RichText};

pub fn show(ctx: &egui::Context, vm: &mut AboutVm) {
    if !vm.show {
        return;
    }
    let builder = egui::ViewportBuilder::default()
        .with_title(format!("About {}", about::NAME))
        .with_inner_size([320.0, 220.0])
        .with_resizable(false)
        .with_minimize_button(false)
        .with_maximize_button(false);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("about"), builder, |ui, _| {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(panel()).inner_margin(Margin::same(PAD)))
            .show(ui, |ui| {
                ui.vertical_centered(about_text);
            });
        if ui.input(|i| i.viewport().close_requested()) {
            vm.show = false;
        }
    });
}

fn about_text(ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.label(RichText::new(about::NAME).size(FS_TITLE).strong());
    ui.label(muted(format!("Version {}", about::VERSION)));
    ui.add_space(6.0);
    ui.label(about::DESCRIPTION);
    ui.add_space(6.0);
    ui.label(format!("Created by {}", about::AUTHOR));
    ui.label(muted(format!(
        "{} · {} License",
        about::COPYRIGHT,
        about::LICENSE
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::layout_test::*;

    #[test]
    fn the_about_text_fits_its_fixed_window() {
        // 320 px less the card's padding
        let width = 320.0 - 2.0 * f32::from(PAD);
        let used = used_width(&ctx(), width, |ui| {
            ui.vertical_centered(about_text);
        });
        assert!(
            used <= width + 1.0,
            "the about text needs {used} px of {width}"
        );
    }
}
