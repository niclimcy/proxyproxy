use crate::domain::app::{self, AppIdentity};
use crate::services::app_icons::IconStore;
use crate::views::theme::*;
use eframe::egui::{self, CornerRadius, RichText, TextureHandle};
use std::collections::HashMap;

/// Decoded icons by key. `None` marks bytes that failed to decode, so they aren't retried each frame.
#[derive(Clone, Default)]
struct Textures(HashMap<String, Option<TextureHandle>>);

fn texture(ctx: &egui::Context, icons: &IconStore, key: &str) -> Option<TextureHandle> {
    if let Some(t) = ctx.data_mut(|d| d.get_temp_mut_or_default::<Textures>(egui::Id::NULL).0.get(key).cloned()) {
        return t;
    }
    let bytes = icons.get(key)?;
    let tex = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .ok()
        .map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            ctx.load_texture(
                format!("app-icon-{key}"),
                egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                egui::TextureOptions::LINEAR,
            )
        });
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Textures>(egui::Id::NULL).0.insert(key.to_string(), tex.clone()));
    tex
}

/// Paints the app's real icon; false when none is available yet.
pub fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icons: &IconStore, identity: Option<&AppIdentity>) -> bool {
    let Some(tex) = identity.and_then(|i| texture(painter.ctx(), icons, &i.icon_key)) else {
        return false;
    };
    painter.add(
        egui::epaint::RectShape::filled(rect, CornerRadius::same(RADIUS_SM), egui::Color32::WHITE)
            .with_texture(tex.id(), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
    );
    true
}

/// Placeholder when no icon exists: a bundled app gets a letter, a bare process (curl, node) a terminal.
pub fn paint_fallback_icon(painter: &egui::Painter, rect: egui::Rect, name: &str, identity: Option<&AppIdentity>) {
    let bundled = identity.is_some_and(|i| !i.bundle_id.is_empty());
    if bundled {
        let color = tag_colors()[app::avatar_slot(name, 5)].1;
        painter.rect_filled(rect, CornerRadius::same(RADIUS_SM), tint(color, Tint::Strong));
    } else {
        painter.rect_filled(rect, CornerRadius::same(RADIUS_SM), tint(fg_muted(), Tint::Normal));
    }
    let (glyph, ink) = if bundled {
        (app::avatar_letter(name).to_string(), fg())
    } else {
        (egui_phosphor::regular::TERMINAL_WINDOW.to_string(), fg_muted())
    };
    // scaled with the rect so the hover zoom grows the glyph too
    let size = FS_CAPTION * rect.height() / ICON_SIZE;
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(size), ink);
}

/// App icon, or a letter avatar until (or unless) one is available.
pub fn app_icon(ui: &mut egui::Ui, icons: &IconStore, name: &str, identity: Option<&AppIdentity>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::click());
    // Eased so the icon doesn't snap between sizes.
    let zoom = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let rect = egui::Rect::from_center_size(rect.center(), rect.size() * (1.0 + 0.25 * zoom));
    let tex = identity.and_then(|i| texture(ui.ctx(), icons, &i.icon_key));
    if let Some(tex) = tex {
        egui::Image::new(&tex).corner_radius(CornerRadius::same(RADIUS_SM)).paint_at(ui, rect);
    } else {
        // the zoomed rect outgrows the cell's clip, which would cut the filled tile
        paint_fallback_icon(&ui.painter().with_clip_rect(rect.expand(1.0)), rect, name, identity);
    }
    resp
}

/// Click filters the table to this app; the hover shows the bundle id when known.
/// Shown to the right so it doesn't cover the next row's icon.
pub fn app_label_hover(resp: egui::Response, name: &str, identity: Option<&AppIdentity>) -> egui::Response {
    let text = match identity.map(|i| i.bundle_id.as_str()).filter(|b| !b.is_empty()) {
        Some(b) => format!("{name}\n{b}"),
        None => name.to_string(),
    };
    egui::Popup::from_response(&resp)
        .kind(egui::PopupKind::Tooltip)
        .align(egui::RectAlign::RIGHT_START)
        .gap(8.0)
        .open(egui::Tooltip::should_show_tooltip(&resp, false))
        .show(|ui| ui.label(RichText::new(text)));
    resp
}
