use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke};

pub const BG: Color32 = Color32::from_rgb(17, 18, 23);
pub const PANEL: Color32 = Color32::from_rgb(24, 26, 33);
pub const CARD: Color32 = Color32::from_rgb(32, 35, 44);
pub const BORDER: Color32 = Color32::from_rgb(45, 49, 61);
pub const TEXT: Color32 = Color32::from_rgb(225, 228, 236);
pub const MUTED: Color32 = Color32::from_rgb(130, 137, 153);
pub const ACCENT: Color32 = Color32::from_rgb(99, 142, 255);
pub const GREEN: Color32 = Color32::from_rgb(74, 201, 128);
pub const BLUE: Color32 = Color32::from_rgb(90, 170, 255);
pub const AMBER: Color32 = Color32::from_rgb(240, 180, 70);
pub const RED: Color32 = Color32::from_rgb(240, 95, 95);
pub const PURPLE: Color32 = Color32::from_rgb(176, 130, 255);
pub const PAD: i8 = 12;
pub const HANDLE: Color32 = Color32::from_rgb(84, 92, 114);

pub fn apply_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = Color32::from_rgb(27, 29, 37);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(12);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.override_text_color = Some(TEXT);
    let r = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, HANDLE);
    v.widgets.hovered.fg_stroke = Stroke::new(2.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(2.0, ACCENT);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_fill = Color32::from_rgb(42, 46, 58);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 46, 58);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.7));
    v.widgets.active.bg_fill = ACCENT.gamma_multiply(0.6);
    v.widgets.active.weak_bg_fill = ACCENT.gamma_multiply(0.6);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.interaction.resize_grab_radius_side = 6.0;
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 28.0;
        use egui::{FontFamily::Proportional, FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::new(14.0, Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(14.0, Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(12.0, Proportional));
    });
}

pub fn ico(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, cur: &mut T, items: &[(T, &str)]) {
    Frame::new().fill(BG).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(3)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (t, name) in items {
                let on = *cur == *t;
                let b = egui::Button::new(RichText::new(*name).size(13.0).color(if on { TEXT } else { MUTED }))
                    .fill(if on { CARD } else { Color32::TRANSPARENT })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(6));
                if ui.add(b).clicked() {
                    *cur = *t;
                }
            }
        });
    });
}

pub fn chip(ui: &mut egui::Ui, text: &str, on: bool, color: Color32) -> egui::Response {
    let b = egui::Button::new(RichText::new(text).size(12.0).color(if on { color } else { MUTED }))
        .fill(if on { color.gamma_multiply(0.2) } else { Color32::TRANSPARENT })
        .stroke(Stroke::new(1.0, if on { color.gamma_multiply(0.7) } else { BORDER }))
        .corner_radius(CornerRadius::same(20))
        .min_size(egui::vec2(0.0, 22.0));
    ui.add(b)
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(MUTED).size(12.0)
}

pub const COLORS: [(&str, Color32); 5] = [
    ("Red", RED),
    ("Amber", AMBER),
    ("Green", GREEN),
    ("Blue", BLUE),
    ("Purple", PURPLE),
];

pub fn icon_button(ui: &mut egui::Ui, icon: &str, label: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(ico(icon, label)).size(13.5)).stroke(Stroke::NONE).fill(Color32::TRANSPARENT))
}

pub fn card() -> Frame {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(11.5).strong());
        });
}

pub fn method_color(m: &str) -> Color32 {
    match m {
        "GET" => GREEN,
        "POST" => AMBER,
        "PUT" | "PATCH" => BLUE,
        "DELETE" => RED,
        _ => PURPLE,
    }
}

pub fn status_color(s: u16) -> Color32 {
    match s {
        200..=299 => GREEN,
        300..=399 => BLUE,
        400..=499 => AMBER,
        _ => RED,
    }
}

pub fn fmt_size(n: usize) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1048575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1048576.0),
    }
}
