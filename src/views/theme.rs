//! Design tokens. Views use these, never raw colours or font sizes.
//!
//! Colours are semantic and resolve per theme (dark/light, follows the OS).
//! Every text colour must hold WCAG AA (4.5:1) on every surface; see tests.

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Palette {
    pub dark: bool,
    pub backdrop: Color32,
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
    // cards are near-black and recessed into a lighter backdrop; controls lift above the cards
    backdrop: Color32::from_rgb(24, 24, 27),
    canvas: Color32::from_rgb(0, 0, 0),
    panel: Color32::from_rgb(9, 9, 10),
    surface: Color32::from_rgb(32, 32, 36),
    hover: Color32::from_rgb(48, 48, 54),
    faint: Color32::from_rgb(18, 18, 20),
    border: Color32::from_rgb(42, 42, 46),
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
    backdrop: Color32::from_rgb(240, 242, 246),
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

pub fn backdrop() -> Color32 {
    pal().backdrop
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
pub const RADIUS_CARD: u8 = 10;
/// Space between workspace cards and the window edge.
pub const GAP: i8 = 6;
pub const ITEM_SPACING: egui::Vec2 = egui::vec2(8.0, 6.0);
pub const HIT: f32 = 28.0;
/// `switch` track size and the clear space on each side of it.
pub const SWITCH_W: f32 = 30.0;
pub const SWITCH_H: f32 = 18.0;
pub const SWITCH_PAD: f32 = 4.0;
pub const ICON_SIZE: f32 = 18.0;

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
    // light enough that badges in a selected row keep their own tint; rows add an edge bar
    let sel = if p.dark { 0.26 } else { 0.18 };
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
        use egui::{
            FontFamily::{Monospace, Proportional},
            FontId, TextStyle,
        };
        s.interaction.resize_grab_radius_side = 6.0;
        s.spacing.item_spacing = ITEM_SPACING;
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = HIT;
        s.spacing.icon_width = 18.0;
        s.spacing.icon_width_inner = 12.0;
        s.spacing.icon_spacing = 8.0;
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
    // no trailing gap, or an icon-only button sits off-centre in its highlight
    if text.is_empty() {
        return icon.to_string();
    }
    if icon.is_empty() {
        return text.to_string();
    }
    format!("{icon}  {text}")
}

/// Label for screen readers: icon glyphs live in the private use area and read as noise.
pub fn spoken(s: &str) -> String {
    s.chars()
        .filter(|c| !('\u{E000}'..='\u{F8FF}').contains(c))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Keyboard focus outline for hand-drawn widgets; egui only draws one for its own.
pub fn focus_ring(ui: &egui::Ui, resp: &egui::Response, radius: u8) {
    if resp.has_focus() {
        ui.painter().rect_stroke(
            resp.rect,
            CornerRadius::same(radius),
            Stroke::new(2.0, accent()),
            egui::StrokeKind::Inside,
        );
    }
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, cur: &mut T, items: &[(T, &str)]) {
    Frame::new()
        .fill(canvas())
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            ui.horizontal_wrapped(|ui| {
                for (t, name) in items {
                    let on = *cur == *t;
                    let g = ui.painter().layout_no_wrap(
                        (*name).to_string(),
                        egui::FontId::proportional(FS_SMALL),
                        if on { fg() } else { fg_muted() },
                    );
                    // width from the label alone, so selection never changes geometry
                    let tab_width = (g.size().x + 28.0).max(56.0);
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(tab_width, HIT), egui::Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Tab, true, on, spoken(name))
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
                    focus_ring(ui, &resp, RADIUS_SM);
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
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, spoken(text)));
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
    focus_ring(ui, &resp, RADIUS_SM);
    resp
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(fg_muted()).size(FS_CAPTION)
}

pub fn checkbox(ui: &mut egui::Ui, checked: &mut bool, text: &str) -> egui::Response {
    let box_size = 18.0;
    let text_x = box_size + 8.0;
    // wraps to the space left, so a long label can't hold its panel wider than the user set it
    let wrap = (ui.available_width() - text_x - 4.0).max(80.0);
    let galley = ui.painter().layout(
        text.to_string(),
        egui::FontId::proportional(FS_BODY),
        fg(),
        wrap,
    );
    let row_height = HIT.max(galley.size().y + 10.0);
    let row_width = text_x + galley.size().x + 4.0;
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::CheckBox,
            ui.is_enabled(),
            *checked,
            spoken(text),
        )
    });

    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }

    let box_rect = egui::Rect::from_center_size(
        rect.left_top() + egui::vec2(text_x / 2.0, HIT / 2.0),
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

    let checked = *checked;
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

    painter.rect_filled(box_rect, CornerRadius::same(RADIUS_SM), fill);
    painter.rect_stroke(
        box_rect,
        CornerRadius::same(RADIUS_SM),
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

    focus_ring(ui, &response, RADIUS_SM);
    response
}

/// Lays a row out inside `w`. Sizing a field as "w minus a guess" let a
/// row end up wider than the panel, and the panel grew to fit it a little more every frame.
pub fn row_within(ui: &mut egui::Ui, w: f32, rtl: bool, add: impl FnOnce(&mut egui::Ui)) {
    let layout = if rtl {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    ui.allocate_ui_with_layout(egui::vec2(w, HIT), layout, add);
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

/// Secondary look: a quiet surface fill with a soft hairline. Skipped when already flat (inside a capsule).
fn secondary<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if ui.visuals().widgets.inactive.weak_bg_fill == Color32::TRANSPARENT {
        return add(ui);
    }
    ui.scope(|ui| {
        let (surface, hover, accent) = (surface(), pal().hover, accent());
        let hairline = Stroke::new(1.0, pal().border);
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = surface;
        w.inactive.bg_stroke = hairline;
        w.hovered.weak_bg_fill = hover;
        w.hovered.bg_stroke = hairline;
        w.active.weak_bg_fill = tint(accent, Tint::Normal);
        w.active.bg_stroke = Stroke::new(1.0, tint(accent, Tint::Strong));
        w.open.weak_bg_fill = hover;
        w.open.bg_stroke = hairline;
        add(ui)
    })
    .inner
}

/// Secondary labelled button.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool) -> egui::Response {
    let resp = secondary(ui, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(RichText::new(ico(icon, label)).size(FS_SMALL))
                .min_size(egui::vec2(0.0, HIT)),
        )
    });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, spoken(label)));
    resp
}

/// Flat glyph button. `name` is required: it is the tooltip and the only thing a screen reader can announce.
pub fn icon_only(ui: &mut egui::Ui, icon: &str, name: &str) -> egui::Response {
    let resp = flat(ui, |ui| {
        ui.add(egui::Button::new(RichText::new(icon).size(FS_SMALL)).min_size(egui::vec2(HIT, HIT)))
    });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    resp.on_hover_text(name)
}

/// Groups related toolbar actions in one rounded capsule of flat buttons.
pub fn capsule<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .fill(surface())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::symmetric(2, 0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            flat(ui, |ui| ui.horizontal(add).inner)
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
    panel_toggle(ui, open, false, "", tip)
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
    let gap = if label.is_empty() { 0.0 } else { 6.0 };
    let size = egui::vec2(10.0 + icon_size.x + gap + g.size().x + 10.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    let name = if label.is_empty() { "Sidebar" } else { label };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, open, name));
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
            rect.min.x + 10.0 + icon_size.x + gap,
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
    focus_ring(ui, &resp, RADIUS);
    resp
}

/// Toolbar menu trigger, styled like `icon_button`.
/// `keep_open`: clicks inside don't close the menu, so rows must call `ui.close()` themselves.
pub fn toolbar_menu(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    keep_open: bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    let close = if keep_open {
        egui::PopupCloseBehavior::CloseOnClickOutside
    } else {
        egui::PopupCloseBehavior::CloseOnClick
    };
    secondary(ui, |ui| {
        if label.is_empty() {
            // same square as `icon_only`, so a collapsed row has even gaps
            ui.spacing_mut().interact_size.x = HIT;
        }
        egui::containers::menu::MenuButton::new(RichText::new(ico(icon, label)).size(FS_SMALL))
            .config(egui::containers::menu::MenuConfig::new().close_behavior(close))
            .ui(ui, add);
    });
}

/// On/off switch for a whole group such as a rule collection; checkboxes stay for per-item choices.
/// `label` is what a screen reader announces.
pub fn switch(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let enabled = ui.is_enabled();
    let (rect, mut resp) = ui.allocate_exact_size(
        egui::vec2(SWITCH_W + 2.0 * SWITCH_PAD, HIT),
        egui::Sense::click(),
    );
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, enabled, *on, label));
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(SWITCH_W, SWITCH_H));
    let painter = ui.painter();
    painter.rect_filled(
        track,
        CornerRadius::same(RADIUS_PILL),
        if *on { accent() } else { pal().border_strong },
    );
    // outside the track, in a colour that differs from both track fills
    if resp.has_focus() {
        painter.rect_stroke(
            track.expand(2.0),
            CornerRadius::same(RADIUS_PILL),
            Stroke::new(2.0, fg()),
            egui::StrokeKind::Outside,
        );
    } else if resp.hovered() && enabled {
        painter.rect_stroke(
            track,
            CornerRadius::same(RADIUS_PILL),
            Stroke::new(1.5, fg_muted()),
            egui::StrokeKind::Outside,
        );
    }
    let knob_x = egui::lerp(
        (track.left() + SWITCH_H / 2.0)..=(track.right() - SWITCH_H / 2.0),
        t,
    );
    painter.circle_filled(
        egui::pos2(knob_x, track.center().y),
        SWITCH_H / 2.0 - 3.0,
        pal().panel,
    );
    resp
}

/// Icon-only "…" menu. `name` is the tooltip and what a screen reader announces.
pub fn overflow_menu(
    ui: &mut egui::Ui,
    name: &str,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let resp = flat(ui, |ui| {
        ui.spacing_mut().interact_size.x = HIT;
        ui.spacing_mut().interact_size.y = HIT;
        egui::containers::menu::MenuButton::new(
            RichText::new(egui_phosphor::regular::DOTS_THREE).size(FS_SMALL),
        )
        .ui(ui, add)
        .0
    });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    resp.on_hover_text(name)
}

/// Popup-menu row with a restrained rounded hover fill and no widget border.
/// `hint` is a shortcut drawn on the right; screen readers only get the label.
pub fn menu_item(ui: &mut egui::Ui, label: &str, hint: Option<&str>) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(FS_SMALL),
        fg(),
    );
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HIT), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MenuItem, true, spoken(label)));
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
    if let Some(hint) = hint {
        ui.painter().text(
            egui::pos2(rect.max.x - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            hint,
            egui::FontId::proportional(FS_SMALL),
            fg_muted(),
        );
    }
    focus_ring(ui, &resp, RADIUS_SM);
    resp
}

/// Size a popup to its longest label instead of inheriting toolbar width.
/// A `menu_item` that opens a submenu; stays highlighted while it is open so the
/// parent row still reads as the origin.
pub fn submenu_item(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    // reserve a slot below the row's own shapes, filled once we know the submenu is open
    let open_bg = ui.painter().add(egui::Shape::Noop);
    let resp = menu_item(ui, label, None);
    ui.painter().text(
        egui::pos2(resp.rect.max.x - 14.0, resp.rect.center().y),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::CARET_RIGHT,
        egui::FontId::proportional(FS_SMALL),
        fg_muted(),
    );
    if egui::containers::menu::SubMenu::new()
        .show(ui, &resp, add)
        .is_some()
    {
        ui.painter().set(
            open_bg,
            egui::Shape::rect_filled(
                resp.rect,
                CornerRadius::same(RADIUS_SM),
                ui.visuals().widgets.hovered.weak_bg_fill,
            ),
        );
    }
}

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

/// Inset card for workspace panels. Outer margins of neighbours add up to `GAP`;
/// the workspace shrinks its area by half a gap so the window edge gets the same.
pub fn card_panel() -> Frame {
    Frame::new()
        .fill(panel())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .outer_margin(Margin::same(GAP / 2))
        .inner_margin(Margin::same(PAD))
}

/// A `card_panel` painted at the slot's size, not the content's: content that can't shrink
/// further is clipped instead of stretching the card into the gap, over a neighbour's
/// resize grip. Use inside a panel with `Frame::NONE`; the slot is the whole `ui.max_rect()`.
pub fn clipped_card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let frame = card_panel();
    let slot = ui.max_rect().shrink(f32::from(GAP) / 2.0);
    ui.painter().rect(
        slot,
        frame.corner_radius,
        frame.fill,
        frame.stroke,
        egui::StrokeKind::Inside,
    );
    let inner = slot.shrink(f32::from(PAD));
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    body.set_clip_rect(inner.intersect(ui.clip_rect()));
    let r = add(&mut body);
    // The child doesn't advance its parent; claim the slot so a sized panel keeps its size.
    ui.expand_to_include_rect(ui.max_rect());
    r
}

/// Root frame for secondary windows: the same backdrop the main workspace sits on, so
/// `pane` cards inside get the same gaps as the main window's panels.
pub fn window_frame() -> Frame {
    Frame::new()
        .fill(backdrop())
        .inner_margin(Margin::same(GAP / 2))
}

/// A `card_panel` for secondary windows. Callers zero the outer item spacing so cards
/// are exactly `GAP` apart; the spacing is restored for the content.
pub fn pane<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_panel()
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = ITEM_SPACING;
            add(ui)
        })
        .inner
}

/// egui paints a panel's resize line with the parent's `fg_stroke` (hover/drag), a full-height
/// edge that overshoots rounded cards. A zero-width stroke suppresses it; the resize cursor
/// still signals the handle. Call with `true` on the parent before showing a resizable panel,
/// `false` after, and at the top of its contents so widgets inside keep their text strokes.
pub fn resize_line(ui: &mut egui::Ui, hidden: bool) {
    let stroke = if hidden {
        Stroke::NONE
    } else {
        Stroke::new(1.0, fg())
    };
    let w = &mut ui.visuals_mut().widgets;
    w.hovered.fg_stroke = stroke;
    w.active.fg_stroke = stroke;
}

/// Three dots centred in the gap between cards, marking a resizable edge. `edge` is the panel's
/// outer-rect side (a line, the middle of the gap). They light up when the pointer is anywhere
/// on the edge, matching egui's grab area, and stay lit for the whole drag. `name` keys the flag.
pub fn resize_grip(ui: &egui::Ui, name: &str, edge: egui::Rect) {
    let vertical = edge.height() > edge.width();
    let axis = if vertical {
        egui::vec2(0.0, 1.0)
    } else {
        egui::vec2(1.0, 0.0)
    };
    let near = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
        edge.expand(ui.style().interaction.resize_grab_radius_side)
            .contains(p)
    });
    let id = ui.make_persistent_id(("resize-grip", name));
    let (pressed, down) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down()));
    let dragging =
        (pressed && near) || (down && ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false));
    ui.data_mut(|d| d.insert_temp(id, dragging));
    let color = if near || dragging {
        accent()
    } else {
        pal().border_strong
    };
    // Own layer so a neighbouring card that overflows its slot (the table at narrow widths) can't cover the dots.
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(egui::Order::Middle, id));
    for step in [-1.0, 0.0, 1.0] {
        painter.circle_filled(edge.center() + axis * (step * 5.0), 1.5, color);
    }
}

/// Toolbar frame: no fill, so controls sit directly on the canvas.
pub fn bar_frame() -> Frame {
    Frame::new()
        .outer_margin(Margin::same(GAP / 2))
        .inner_margin(Margin::symmetric(0, 4))
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

/// Leading slot of a `list_row`: a glyph, or caller-painted content (e.g. an app icon).
#[derive(Clone, Copy)]
pub enum RowIcon<'a> {
    Glyph(&'a str),
    Paint(&'a dyn Fn(&egui::Painter, egui::Rect)),
}

/// Full-width list row: label left, count right, filled when selected.
pub fn list_row(
    ui: &mut egui::Ui,
    icon: Option<RowIcon>,
    label: &str,
    count: Option<usize>,
    selected: bool,
    dim: bool,
) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HIT), egui::Sense::click());
    let name = match count {
        Some(n) => format!("{}, {n}", spoken(label)),
        None => spoken(label),
    };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &name));
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
    match icon {
        Some(RowIcon::Glyph(g)) => {
            p.text(
                egui::pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                g,
                egui::FontId::proportional(FS_BODY),
                fg_muted(),
            );
            x += 22.0;
        }
        Some(RowIcon::Paint(f)) => {
            let r = egui::Rect::from_min_size(
                egui::pos2(x, rect.center().y - ICON_SIZE / 2.0),
                egui::vec2(ICON_SIZE, ICON_SIZE),
            );
            f(p, r);
            x += ICON_SIZE + 6.0;
        }
        None => {}
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
    focus_ring(ui, &resp, RADIUS_SM);
    resp
}

/// Compact monospace badge for table cells (method, status).
/// `on_selection`: the row's highlight stacks under the tint, so the text switches to `fg` to stay AA.
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32, on_selection: bool) {
    let ink = if on_selection { fg() } else { color };
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
                    .color(ink),
            );
        });
}

/// `label` carries what the colour means, for the tooltip and screen readers.
pub fn status_dot(ui: &mut egui::Ui, color: Color32, label: &str) {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, label));
    resp.on_hover_text(label);
}

/// Filled action button (primary/destructive). Text stays `fg` for contrast.
pub fn action_button(text: impl Into<String>, color: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).strong())
        .fill(tint(color, Tint::Strong))
        .stroke(Stroke::new(1.0, color))
}

/// Fixed-width `action_button` drawn by hand so hover and press keep the same geometry.
pub fn action_button_sized(
    ui: &mut egui::Ui,
    icon: &str,
    text: &str,
    color: Color32,
    width: f32,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, HIT), sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));
    let (fill, stroke, ink) = if !enabled {
        (
            tint(fg_muted(), Tint::Subtle),
            Stroke::new(1.0, border()),
            fg_muted(),
        )
    } else if resp.is_pointer_button_down_on() {
        (tint(color, Tint::Normal), Stroke::new(1.5, color), fg())
    } else if resp.hovered() {
        (tint(color, Tint::Strong), Stroke::new(1.5, color), fg())
    } else {
        (tint(color, Tint::Strong), Stroke::new(1.0, color), fg())
    };
    let g = ui
        .painter()
        .layout_no_wrap(ico(icon, text), egui::FontId::proportional(FS_BODY), ink);
    ui.painter().rect(
        rect,
        CornerRadius::same(RADIUS_SM),
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - g.size() / 2.0, g, ink);
    focus_ring(ui, &resp, RADIUS_SM);
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
        1024..=1_048_575 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
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
        let c = f64::from(c) / 255.0;
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
        let m = |f: u8, b: u8| (f32::from(f) * a + f32::from(b) * (1.0 - a)).round() as u8;
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
                ("backdrop", p.backdrop),
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
                ("backdrop", p.backdrop),
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
    fn switch_knob_stands_out_on_both_tracks() {
        for p in [&DARK, &LIGHT] {
            for (n, track) in [("accent", p.accent), ("border_strong", p.border_strong)] {
                let r = ratio(p.panel, track);
                assert!(r >= 3.0, "knob on {n} track (dark={}) = {r:.2}", p.dark);
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
    fn hint_and_weak_text_meet_aa() {
        for p in [&DARK, &LIGHT] {
            let weak = visuals(p).weak_text_color();
            for (n, bg) in [
                ("canvas", p.canvas),
                ("panel", p.panel),
                ("surface", p.surface),
            ] {
                let r = ratio(weak, bg);
                assert!(r >= 4.5, "weak text on {n} (dark={}) = {r:.2}", p.dark);
            }
        }
    }

    // selected table rows: plain text, badges (hue on its own tint) and status glyphs
    #[test]
    fn selected_rows_keep_text_and_glyph_contrast() {
        let mut bad = vec![];
        for p in [&DARK, &LIGHT] {
            let v = visuals(p);
            for base in [p.panel, p.faint] {
                let sel = over(p.accent, f32::from(v.selection.bg_fill.a()) / 255.0, base);
                for (n, c) in [("fg", p.fg), ("fg_muted", p.fg_muted)] {
                    let r = ratio(c, sel);
                    if r < 4.5 {
                        bad.push(format!("{n} text (dark={}) = {r:.2}", p.dark));
                    }
                }
                for (n, c) in hues(p) {
                    let badge = ratio(p.fg, over(c, tint_alpha(p.dark, Tint::Normal), sel));
                    if badge < 4.5 {
                        bad.push(format!("{n} badge (dark={}) = {badge:.2}", p.dark));
                    }
                    let glyph = ratio(c, sel);
                    if glyph < 3.0 {
                        bad.push(format!("{n} glyph (dark={}) = {glyph:.2}", p.dark));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
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
