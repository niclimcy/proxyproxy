//! Design tokens. Views use these, never raw colours or font sizes.
//!
//! Colours are semantic and resolve per theme (dark/light, follows the OS).
//! Every text colour must hold WCAG AA (4.5:1) on every surface; see tests.

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Palette {
    pub dark: bool,
    pub canvas: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub hover: Color32,
    pub faint: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub fg: Color32,
    pub fg_muted: Color32,
    pub accent: Color32,
    pub success: Color32,
    pub info: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub violet: Color32,
}

pub const DARK: Palette = Palette {
    dark: true,
    canvas: Color32::from_rgb(24, 24, 26),
    panel: Color32::from_rgb(31, 31, 34),
    surface: Color32::from_rgb(43, 43, 47),
    hover: Color32::from_rgb(56, 56, 61),
    faint: Color32::from_rgb(35, 35, 38),
    border: Color32::from_rgb(58, 58, 63),
    border_strong: Color32::from_rgb(122, 122, 132),
    fg: Color32::from_rgb(234, 234, 238),
    fg_muted: Color32::from_rgb(182, 182, 192),
    accent: Color32::from_rgb(140, 174, 255),
    success: Color32::from_rgb(74, 201, 128),
    info: Color32::from_rgb(110, 182, 255),
    warning: Color32::from_rgb(240, 180, 70),
    danger: Color32::from_rgb(255, 144, 144),
    violet: Color32::from_rgb(200, 166, 255),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    canvas: Color32::from_rgb(244, 246, 249),
    panel: Color32::from_rgb(255, 255, 255),
    surface: Color32::from_rgb(238, 241, 246),
    hover: Color32::from_rgb(226, 231, 240),
    faint: Color32::from_rgb(240, 243, 248),
    border: Color32::from_rgb(208, 213, 223),
    border_strong: Color32::from_rgb(112, 120, 140),
    fg: Color32::from_rgb(22, 25, 33),
    fg_muted: Color32::from_rgb(80, 88, 106),
    accent: Color32::from_rgb(37, 84, 214),
    success: Color32::from_rgb(14, 110, 60),
    info: Color32::from_rgb(10, 94, 170),
    warning: Color32::from_rgb(140, 82, 0),
    danger: Color32::from_rgb(185, 28, 28),
    violet: Color32::from_rgb(108, 56, 196),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

/// Call once per frame, before drawing.
pub fn sync(ctx: &egui::Context) {
    IS_DARK.store(ctx.theme() == egui::Theme::Dark, Ordering::Relaxed);
}

pub fn pal() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn canvas() -> Color32 {
    pal().canvas
}
pub fn panel() -> Color32 {
    pal().panel
}
pub fn surface() -> Color32 {
    pal().surface
}
pub fn border() -> Color32 {
    pal().border
}
pub fn fg() -> Color32 {
    pal().fg
}
pub fn fg_muted() -> Color32 {
    pal().fg_muted
}
pub fn accent() -> Color32 {
    pal().accent
}
pub fn success() -> Color32 {
    pal().success
}
pub fn info() -> Color32 {
    pal().info
}
pub fn warning() -> Color32 {
    pal().warning
}
pub fn danger() -> Color32 {
    pal().danger
}
pub fn violet() -> Color32 {
    pal().violet
}

/// Translucent wash of a hue for fills behind fg text or the hue itself.
pub fn tint(c: Color32, strength: Tint) -> Color32 {
    c.gamma_multiply(tint_alpha(pal().dark, strength))
}

fn tint_alpha(dark: bool, strength: Tint) -> f32 {
    match (dark, strength) {
        (true, Tint::Subtle) => 0.12,
        (true, Tint::Normal) => 0.18,
        (true, Tint::Strong) => 0.40,
        (false, Tint::Subtle) => 0.08,
        (false, Tint::Normal) => 0.14,
        (false, Tint::Strong) => 0.22,
    }
}

#[derive(Clone, Copy)]
pub enum Tint {
    Subtle,
    Normal,
    Strong,
}

/// Font sizes (pt). Nothing below CAPTION.
pub const FS_CAPTION: f32 = 12.0;
pub const FS_SMALL: f32 = 13.0;
pub const FS_BODY: f32 = 14.0;
pub const FS_TITLE: f32 = 16.0;

/// Spacing and shape.
pub const PAD: i8 = 12;
pub const RADIUS: u8 = 8;
pub const RADIUS_SM: u8 = 6;
pub const RADIUS_PILL: u8 = 20;
pub const HIT: f32 = 28.0;

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.canvas;
    v.faint_bg_color = p.faint;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    let sel = if p.dark { 0.45 } else { 0.28 };
    v.selection.bg_fill = p.accent.gamma_multiply(sel);
    v.selection.stroke = Stroke::new(1.5, p.fg); // also colours text in selected rows
    let r = CornerRadius::same(RADIUS);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
        w.fg_stroke = Stroke::new(1.0, p.fg);
    }
    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill = p.surface;
    v.widgets.inactive.weak_bg_fill = p.surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border_strong);
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.bg_fill = p.accent.gamma_multiply(if p.dark { 0.5 } else { 0.25 });
    v.widgets.active.weak_bg_fill = v.widgets.active.bg_fill;
    v.widgets.active.bg_stroke = Stroke::new(1.5, p.accent);
    v.widgets.open.bg_fill = p.hover;
    v.widgets.open.weak_bg_fill = p.hover;
    v
}

/// Prefer the platform UI and mono fonts when present; egui's bundled fonts stay as fallback.
fn load_system_fonts(fonts: &mut egui::FontDefinitions) {
    let candidates = [
        (
            "sys-ui",
            egui::FontFamily::Proportional,
            [
                "/System/Library/Fonts/SFNS.ttf",
                "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
                "C:\\Windows\\Fonts\\segoeui.ttf",
            ],
        ),
        (
            "sys-mono",
            egui::FontFamily::Monospace,
            [
                "/System/Library/Fonts/SFNSMono.ttf",
                "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
                "C:\\Windows\\Fonts\\consola.ttf",
            ],
        ),
    ];
    for (name, family, paths) in candidates {
        let Some(bytes) = paths.iter().find_map(|p| std::fs::read(p).ok()) else {
            continue;
        };
        fonts.font_data.insert(
            name.into(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    load_system_fonts(&mut fonts);
    ctx.set_fonts(fonts);
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK));
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT));
    ctx.set_theme(egui::ThemePreference::System);
    ctx.all_styles_mut(|s| {
        s.interaction.resize_grab_radius_side = 6.0;
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = HIT;
        s.spacing.icon_width = 18.0;
        s.spacing.icon_width_inner = 12.0;
        s.spacing.icon_spacing = 8.0;
        use egui::{
            FontFamily::{Monospace, Proportional},
            FontId, TextStyle,
        };
        s.text_styles
            .insert(TextStyle::Body, FontId::new(FS_BODY, Proportional));
        s.text_styles
            .insert(TextStyle::Button, FontId::new(FS_BODY, Proportional));
        s.text_styles
            .insert(TextStyle::Small, FontId::new(FS_CAPTION, Proportional));
        s.text_styles
            .insert(TextStyle::Monospace, FontId::new(FS_SMALL, Monospace));
    });
    sync(ctx);
}

pub fn ico(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, cur: &mut T, items: &[(T, &str)]) {
    let tab_width = items
        .iter()
        .map(|(_, name)| {
            ui.painter()
                .layout_no_wrap(
                    (*name).to_string(),
                    egui::FontId::proportional(FS_SMALL),
                    fg(),
                )
                .size()
                .x
                + 16.0
        })
        .fold(72.0_f32, f32::max);
    Frame::new()
        .fill(canvas())
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (t, name) in items {
                    let on = *cur == *t;
                    let g = ui.painter().layout_no_wrap(
                        (*name).to_string(),
                        egui::FontId::proportional(FS_SMALL),
                        if on { fg() } else { fg_muted() },
                    );
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(tab_width, HIT), egui::Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, on, *name)
                    });
                    let fill = if resp.is_pointer_button_down_on() {
                        if on {
                            if pal().dark { surface() } else { panel() }
                        } else {
                            ui.visuals().widgets.active.weak_bg_fill
                        }
                    } else if resp.hovered() {
                        if on {
                            tint(accent(), Tint::Subtle)
                        } else {
                            ui.visuals().widgets.hovered.weak_bg_fill
                        }
                    } else if on {
                        if pal().dark { surface() } else { panel() }
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(RADIUS_SM), fill);
                    ui.painter().galley(
                        egui::pos2(
                            rect.center().x - g.size().x / 2.0,
                            rect.center().y - g.size().y / 2.0,
                        ),
                        g,
                        if on { fg() } else { fg_muted() },
                    );
                    if resp.clicked() {
                        *cur = *t;
                    }
                }
            });
        });
}

/// Filter tab: selected = filled pill, otherwise plain text (fill is the non-colour cue).
pub fn chip(ui: &mut egui::Ui, text: &str, on: bool, color: Color32) -> egui::Response {
    let ink = if on { color } else { fg_muted() };
    let g =
        ui.painter()
            .layout_no_wrap(text.to_string(), egui::FontId::proportional(FS_SMALL), ink);
    let size = egui::vec2(g.size().x + 16.0, HIT);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, text));
    let fill = if resp.is_pointer_button_down_on() {
        if on {
            tint(color, Tint::Strong)
        } else {
            ui.visuals().widgets.active.weak_bg_fill
        }
    } else if resp.hovered() {
        if on {
            tint(color, Tint::Normal)
        } else {
            ui.visuals().widgets.hovered.weak_bg_fill
        }
    } else if on {
        tint(color, Tint::Normal)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS_SM), fill);
    ui.painter().galley(
        egui::pos2(
            rect.center().x - g.size().x / 2.0,
            rect.center().y - g.size().y / 2.0,
        ),
        g,
        ink,
    );
    resp
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(fg_muted()).size(FS_CAPTION)
}

pub fn checkbox(ui: &mut egui::Ui, checked: &mut bool, text: &str) -> egui::Response {
    ui.add(StyledCheckbox::new(checked, text))
}

struct StyledCheckbox<'a> {
    checked: &'a mut bool,
    text: &'a str,
}

impl<'a> StyledCheckbox<'a> {
    fn new(checked: &'a mut bool, text: &'a str) -> Self {
        Self { checked, text }
    }
}

impl egui::Widget for StyledCheckbox<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let box_size = 18.0;
        let text_x = box_size + 8.0;
        let galley = ui.painter().layout_no_wrap(
            self.text.to_string(),
            egui::FontId::proportional(FS_BODY),
            fg(),
        );
        let row_height = HIT.max(galley.size().y + 10.0);
        let row_width = text_x + galley.size().x + 4.0;
        let (rect, mut response) =
            ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::CheckBox,
                ui.is_enabled(),
                *self.checked,
                self.text,
            )
        });

        let toggled = response.clicked()
            || (response.has_focus()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Space) || input.key_pressed(egui::Key::Enter)
                }));
        if toggled {
            *self.checked = !*self.checked;
            response.mark_changed();
        }

        let box_rect = egui::Rect::from_center_size(
            rect.left_top() + egui::vec2(text_x / 2.0, row_height / 2.0),
            egui::vec2(box_size, box_size),
        );
        let painter = ui.painter();

        if response.hovered() {
            painter.rect_filled(
                rect.shrink(2.0),
                CornerRadius::same(RADIUS_SM),
                tint(accent(), Tint::Subtle),
            );
        }

        let checked = *self.checked;
        let fill = if checked {
            accent()
        } else if response.is_pointer_button_down_on() {
            ui.visuals().widgets.active.weak_bg_fill
        } else {
            pal().surface
        };
        let stroke = if checked || response.hovered() {
            Stroke::new(1.5, accent())
        } else {
            Stroke::new(1.0, pal().border_strong)
        };
        painter.rect_filled(box_rect, CornerRadius::same(5), fill);
        painter.rect_stroke(
            box_rect,
            CornerRadius::same(5),
            stroke,
            egui::StrokeKind::Inside,
        );

        if checked {
            let check_color = if pal().dark {
                pal().canvas
            } else {
                pal().panel
            };
            painter.add(egui::epaint::Shape::line(
                vec![
                    egui::pos2(box_rect.left() + 4.0, box_rect.center().y),
                    egui::pos2(box_rect.center().x - 0.5, box_rect.bottom() - 4.5),
                    egui::pos2(box_rect.right() - 3.5, box_rect.top() + 4.5),
                ],
                Stroke::new(2.2, check_color),
            ));
        }

        painter.galley(
            egui::pos2(
                rect.left() + text_x,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            fg(),
        );

        if response.has_focus() {
            painter.rect_stroke(
                rect,
                CornerRadius::same(RADIUS_SM),
                Stroke::new(2.0, accent()),
                egui::StrokeKind::Inside,
            );
        }

        response
    }
}

/// Section label: small, caps, muted.
pub fn overline(text: impl Into<String>) -> RichText {
    RichText::new(text)
        .size(FS_CAPTION)
        .strong()
        .color(fg_muted())
}

/// Row tag colours, in menu order.
pub fn tag_colors() -> [(&'static str, Color32); 5] {
    [
        ("Red", danger()),
        ("Amber", warning()),
        ("Green", success()),
        ("Blue", info()),
        ("Purple", violet()),
    ]
}

/// Borderless, transparent-at-rest widgets that still show hover/press fills.
pub fn flat<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
        w.noninteractive.bg_stroke = Stroke::NONE;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        w.hovered.bg_stroke = Stroke::NONE;
        w.active.bg_stroke = Stroke::NONE;
        w.open.bg_stroke = Stroke::NONE;
        add(ui)
    })
    .inner
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, label: &str) -> egui::Response {
    let text = if label.is_empty() {
        icon.to_string()
    } else {
        ico(icon, label)
    };
    flat(ui, |ui| {
        ui.add(egui::Button::new(RichText::new(text).size(FS_SMALL)))
    })
}

pub fn icon_button_enabled(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let text = ico(icon, label);
    flat(ui, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(RichText::new(text).size(FS_SMALL))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE),
        )
    })
}

/// Groups related toolbar actions in one rounded capsule.
pub fn capsule<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .fill(canvas())
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::symmetric(2, 0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(add).inner
        })
        .inner
}

/// Sidebar toggle. State is shown by shape (filled left panel = open), not colour alone.
pub fn sidebar_toggle(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let tip = format!(
        "{} sidebar ({}B)",
        if open { "Hide" } else { "Show" },
        cmd()
    );
    panel_toggle(ui, open, false, "Capture", tip)
}

/// Mirror of `sidebar_toggle` for the right edge: label first, then icon.
pub fn right_panel_toggle(
    ui: &mut egui::Ui,
    open: bool,
    label: &str,
    tip: String,
) -> egui::Response {
    panel_toggle(ui, open, true, label, tip)
}

fn panel_toggle(
    ui: &mut egui::Ui,
    open: bool,
    right: bool,
    label: &str,
    tip: String,
) -> egui::Response {
    let ink = if open { fg() } else { fg_muted() };
    let g =
        ui.painter()
            .layout_no_wrap(label.to_string(), egui::FontId::proportional(FS_SMALL), ink);
    let icon_size = egui::vec2(18.0, 14.0);
    let size = egui::vec2(10.0 + icon_size.x + 6.0 + g.size().x + 10.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, open, label));
    let fill = if resp.is_pointer_button_down_on() {
        ui.visuals().widgets.active.weak_bg_fill
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(RADIUS), fill);
    let (icon_x, text_x) = if right {
        (rect.max.x - 10.0 - icon_size.x / 2.0, rect.min.x + 10.0)
    } else {
        (
            rect.min.x + 10.0 + icon_size.x / 2.0,
            rect.min.x + 10.0 + icon_size.x + 6.0,
        )
    };
    let icon = egui::Rect::from_center_size(egui::pos2(icon_x, rect.center().y), icon_size);
    p.galley(
        egui::pos2(text_x, rect.center().y - g.size().y / 2.0),
        g,
        ink,
    );
    let stroke = Stroke::new(1.5, ink);
    p.rect_stroke(icon, 3.0, stroke, egui::StrokeKind::Inside);
    if right {
        let panel = egui::Rect::from_min_max(egui::pos2(icon.max.x - 6.0, icon.min.y), icon.max);
        if open {
            p.rect_filled(
                panel.shrink(1.0),
                CornerRadius {
                    nw: 0,
                    sw: 0,
                    ne: 2,
                    se: 2,
                },
                ink,
            );
        } else {
            p.line_segment([panel.left_top(), panel.left_bottom()], stroke);
        }
    } else {
        let panel = egui::Rect::from_min_max(icon.min, egui::pos2(icon.min.x + 6.0, icon.max.y));
        if open {
            p.rect_filled(
                panel.shrink(1.0),
                CornerRadius {
                    nw: 2,
                    sw: 2,
                    ne: 0,
                    se: 0,
                },
                ink,
            );
        } else {
            p.line_segment([panel.right_top(), panel.right_bottom()], stroke);
        }
    }
    if resp.has_focus() {
        p.rect_stroke(
            rect,
            CornerRadius::same(RADIUS),
            Stroke::new(2.0, accent()),
            egui::StrokeKind::Inside,
        );
    }
    resp
}

/// Toolbar menu trigger, styled like `icon_button`.
pub fn toolbar_menu(ui: &mut egui::Ui, icon: &str, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    flat(ui, |ui| {
        ui.menu_button(RichText::new(ico(icon, label)).size(FS_SMALL), add);
    });
}

/// Popup-menu row with a restrained rounded hover fill and no widget border.
pub fn menu_item(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(FS_SMALL),
        fg(),
    );
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HIT), egui::Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        ui.visuals().widgets.active.weak_bg_fill
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS_SM), fill);
    ui.painter().galley(
        egui::pos2(rect.min.x + 8.0, rect.center().y - g.size().y / 2.0),
        g,
        fg(),
    );
    resp
}

/// Size a popup to its longest label instead of inheriting toolbar width.
pub fn menu_width(ui: &mut egui::Ui, labels: &[&str]) {
    let width = labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    (*label).to_string(),
                    egui::FontId::proportional(FS_SMALL),
                    fg(),
                )
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    ui.set_width(width + 32.0);
}

/// Modifier-key glyph for shortcut hints.
pub fn cmd() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    }
}

pub fn card() -> Frame {
    Frame::new()
        .fill(surface())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(PAD))
}

/// Frame for side/top/bottom panels.
pub fn panel_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .stroke(Stroke::new(1.0, border()))
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(tint(color, Tint::Normal))
        .corner_radius(CornerRadius::same(RADIUS_PILL))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(FS_CAPTION).strong());
        });
}

/// Full-width list row: label left, count right, filled when selected.
pub fn list_row(
    ui: &mut egui::Ui,
    icon: Option<&str>,
    label: &str,
    count: Option<usize>,
    selected: bool,
    dim: bool,
) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HIT), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    let fill = if selected {
        tint(accent(), Tint::Normal)
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(RADIUS_SM), fill);
    let ink = if dim { fg_muted() } else { fg() };
    let mut x = rect.left() + 8.0;
    if let Some(icon) = icon {
        p.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(FS_BODY),
            fg_muted(),
        );
        x += 22.0;
    }
    let right = count.map(|n| {
        let g = p.layout_no_wrap(
            n.to_string(),
            egui::FontId::monospace(FS_CAPTION),
            fg_muted(),
        );
        let w = g.size().x;
        p.galley(
            egui::pos2(rect.right() - 8.0 - w, rect.center().y - g.size().y / 2.0),
            g,
            fg_muted(),
        );
        w + 8.0
    });
    let max_w = rect.right() - x - right.unwrap_or(0.0) - 8.0;
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_string(),
        egui::FontId::proportional(FS_SMALL),
        ink,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(0.0));
    let g = p.layout_job(job);
    p.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, ink);
    resp
}

/// Compact monospace badge for table cells (method, status).
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(tint(color, Tint::Normal))
        .corner_radius(CornerRadius::same(RADIUS_SM))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .monospace()
                    .strong()
                    .size(FS_CAPTION)
                    .color(color),
            );
        });
}

pub fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

/// Filled action button (primary/destructive). Text stays `fg` for contrast.
pub fn action_button(text: impl Into<String>, color: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).strong())
        .fill(tint(color, Tint::Strong))
        .stroke(Stroke::new(1.0, color))
}

/// Fixed-size filled action button with an explicit hover/press state.
pub fn action_button_sized(
    ui: &mut egui::Ui,
    text: &str,
    color: Color32,
    width: f32,
) -> egui::Response {
    let g =
        ui.painter()
            .layout_no_wrap(text.to_string(), egui::FontId::proportional(FS_BODY), fg());
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, HIT), egui::Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        color.gamma_multiply(if pal().dark { 0.70 } else { 0.82 })
    } else if resp.hovered() {
        color.gamma_multiply(if pal().dark { 0.85 } else { 1.12 })
    } else {
        tint(color, Tint::Strong)
    };
    let stroke = if resp.hovered() {
        Stroke::new(1.5, color)
    } else {
        Stroke::new(1.0, color)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(RADIUS_SM),
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(
        egui::pos2(
            rect.center().x - g.size().x / 2.0,
            rect.center().y - g.size().y / 2.0,
        ),
        g,
        fg(),
    );
    resp
}

/// Fixed-size icon action with a stable hover and pressed state.
pub fn action_button_icon_sized(
    ui: &mut egui::Ui,
    icon: &str,
    text: &str,
    color: Color32,
    width: f32,
) -> egui::Response {
    action_button_icon_sized_enabled(ui, icon, text, color, width, true)
}

pub fn action_button_icon_sized_enabled(
    ui: &mut egui::Ui,
    icon: &str,
    text: &str,
    color: Color32,
    width: f32,
    enabled: bool,
) -> egui::Response {
    let text = ico(icon, text);
    let g = ui
        .painter()
        .layout_no_wrap(text, egui::FontId::proportional(FS_BODY), fg());
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, HIT), sense);
    let fill = if enabled && resp.is_pointer_button_down_on() {
        color.gamma_multiply(if pal().dark { 0.70 } else { 0.82 })
    } else if enabled && resp.hovered() {
        color.gamma_multiply(if pal().dark { 0.85 } else { 1.12 })
    } else if !enabled {
        tint(fg_muted(), Tint::Subtle)
    } else {
        tint(color, Tint::Strong)
    };
    let stroke = if enabled && resp.hovered() {
        Stroke::new(1.5, color)
    } else if !enabled {
        Stroke::new(1.0, border())
    } else {
        Stroke::new(1.0, color)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(RADIUS_SM),
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(
        egui::pos2(
            rect.center().x - g.size().x / 2.0,
            rect.center().y - g.size().y / 2.0,
        ),
        g,
        fg(),
    );
    resp
}

pub fn method_color(m: &str) -> Color32 {
    match m {
        "GET" => success(),
        "POST" => warning(),
        "PUT" | "PATCH" => info(),
        "DELETE" => danger(),
        _ => violet(),
    }
}

pub fn status_color(s: u16) -> Color32 {
    match s {
        200..=299 => success(),
        300..=399 => info(),
        400..=499 => warning(),
        _ => danger(),
    }
}

pub fn fmt_size(n: usize) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1048575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1048576.0),
    }
}

pub fn code_layouter(
    ui: &egui::Ui,
    text: &dyn egui::TextBuffer,
    wrap_width: f32,
    language: &str,
) -> std::sync::Arc<egui::text::Galley> {
    let theme = CodeTheme::from_style(ui.style());
    let mut job = highlight(ui.ctx(), ui.style(), &theme, text.as_str(), language);
    job.wrap.max_width = wrap_width;
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lin(c: u8) -> f64 {
        let c = c as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn lum(c: Color32) -> f64 {
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    fn ratio(a: Color32, b: Color32) -> f64 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    fn over(fg: Color32, a: f32, bg: Color32) -> Color32 {
        let m = |f: u8, b: u8| (f as f32 * a + b as f32 * (1.0 - a)).round() as u8;
        Color32::from_rgb(m(fg.r(), bg.r()), m(fg.g(), bg.g()), m(fg.b(), bg.b()))
    }

    fn hues(p: &Palette) -> [(&'static str, Color32); 7] {
        [
            ("accent", p.accent),
            ("success", p.success),
            ("info", p.info),
            ("warning", p.warning),
            ("danger", p.danger),
            ("violet", p.violet),
            ("fg_muted", p.fg_muted),
        ]
    }

    #[test]
    fn text_meets_aa_on_every_surface() {
        for p in [&DARK, &LIGHT] {
            for (bg_name, bg) in [
                ("canvas", p.canvas),
                ("panel", p.panel),
                ("surface", p.surface),
                ("hover", p.hover),
                ("faint", p.faint),
            ] {
                for (n, c) in hues(p).into_iter().chain([("fg", p.fg)]) {
                    let r = ratio(c, bg);
                    assert!(r >= 4.5, "{n} on {bg_name} (dark={}) = {r:.2}", p.dark);
                }
            }
        }
    }

    #[test]
    fn control_outlines_meet_3_to_1() {
        for p in [&DARK, &LIGHT] {
            for (n, bg) in [
                ("panel", p.panel),
                ("canvas", p.canvas),
                ("surface", p.surface),
            ] {
                let r = ratio(p.border_strong, bg);
                assert!(r >= 3.0, "border_strong on {n} (dark={}) = {r:.2}", p.dark);
            }
        }
    }

    #[test]
    fn text_meets_aa_on_tints() {
        for p in [&DARK, &LIGHT] {
            for (n, c) in hues(p) {
                for base in [p.panel, p.surface, p.canvas] {
                    let r = ratio(c, over(c, tint_alpha(p.dark, Tint::Normal), base));
                    assert!(
                        r >= 4.5,
                        "{n} on own tint over base (dark={}) = {r:.2}",
                        p.dark
                    );
                }
            }
        }
    }

    #[test]
    fn fg_readable_on_action_fills() {
        for p in [&DARK, &LIGHT] {
            for (n, c) in hues(p) {
                let r = ratio(p.fg, over(c, tint_alpha(p.dark, Tint::Strong), p.panel));
                assert!(r >= 4.5, "fg on {n} action fill (dark={}) = {r:.2}", p.dark);
            }
        }
    }
}
