use crate::views::theme::*;
use eframe::egui::{self, Frame, Margin, RichText, Stroke};

pub fn show(ui: &mut egui::Ui, text: &str, api_port: u16, paused: usize) {
    egui::Panel::bottom("status").exact_size(34.0).frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(PAD, 7)).stroke(Stroke::new(1.0, BORDER))).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(text).size(13.0).color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("API http://127.0.0.1:{api_port}")).size(13.0).color(MUTED));
                if paused > 0 {
                    pill(ui, &format!("{paused} paused"), AMBER);
                }
            });
        });
    });
}
