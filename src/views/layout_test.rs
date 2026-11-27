//! Shared pieces for the headless layout tests that sit beside each view.
//!
//! Resizable egui panels grow to their content's minimum width, so one widget that refuses to
//! shrink holds a panel wider than the user set it. These tests run real frames with the real
//! fonts and long, realistic data, then measure.

use crate::domain::flow::{Flow, Message};
use eframe::egui;

pub const HOST: &str = "very-long-subdomain-name.internal.corporate-network.example-company.co.uk";
pub const PATH: &str = "/api/v2/organizations/0123456789abcdef0123456789abcdef/projects/fedcba9876543210fedcba9876543210/deployments/latest/artifacts?token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9&redirect_uri=https%3A%2F%2Fexample.com%2Fcallback";
pub const APP: &str = "Some Very Long Application Name Helper (Renderer)";
pub const FILE: &str = "/Users/someone-with-a-long-name/Documents/projects/client-work/mock-responses/v2/organizations/deployments-latest-response.json";
pub const WORDS: &str = "Supercalifragilisticexpialidocious-and-other-unbroken-words-that-never-wrap-nicely-at-all-0123456789";

pub fn url() -> String {
    format!("https://{HOST}{PATH}")
}

/// A context with the app's fonts and theme: without them text widths are wrong and bugs hide.
pub fn ctx() -> egui::Context {
    let ctx = egui::Context::default();
    crate::views::theme::apply_theme(&ctx);
    ctx
}

pub fn input(screen: egui::Vec2) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
        ..Default::default()
    }
}

/// Runs `n` frames, as many as a growing panel needs to reach its cap.
pub fn frames(
    ctx: &egui::Context,
    screen: egui::Vec2,
    n: usize,
    mut f: impl FnMut(&mut egui::Ui, usize),
) {
    for i in 0..n {
        ctx.run_ui(input(screen), |ui| f(ui, i))
            .textures_delta
            .clear();
    }
}

/// Width a side panel took from the screen. Only valid while it is the one panel in the `ui`.
pub fn taken(ui: &egui::Ui) -> f32 {
    ui.ctx().content_rect().width() - ui.available_rect_before_wrap().width()
}

/// Stores the size the user "dragged" a panel to, before its first frame.
pub fn drag_panel(ui: &egui::Ui, salt: &str, size: egui::Vec2) {
    let id = ui.scope_id().with(salt);
    let state = egui::containers::panel::PanelState {
        outer_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, size),
    };
    ui.ctx().data_mut(|d| d.insert_persisted(id, state));
}

/// Width `add` needs when given `avail` px: more than `avail` means it leaks past its parent.
pub fn used_width(ctx: &egui::Context, avail: f32, mut add: impl FnMut(&mut egui::Ui)) -> f32 {
    let mut used = 0.0;
    frames(ctx, egui::vec2(avail + 400.0, 900.0), 4, |ui, _| {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(avail, 800.0));
        used = ui
            .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| add(ui))
            .response
            .rect
            .width();
    });
    used
}

pub fn flow(id: u64) -> Flow {
    let mut f = Flow::sample(id);
    f.host = HOST.into();
    f.path = PATH.into();
    f.method = "POST".into();
    f.app = APP.into();
    f.status = Some(200);
    f.duration_ms = Some(123);
    f.tag = "a-rather-long-tag-name".into();
    f.request = Message {
        headers: vec![
            ("Authorization".into(), format!("Bearer {WORDS}{WORDS}")),
            (
                "X-Very-Long-Custom-Header-Name-That-Wraps".into(),
                WORDS.into(),
            ),
        ],
        body: bytes::Bytes::from(format!(r#"{{"key":"{WORDS}{WORDS}"}}"#)),
    };
    f.response = Some(Message {
        headers: vec![
            (
                "Content-Type".into(),
                "application/vnd.company.resource.v2+json; charset=utf-8".into(),
            ),
            (
                "Cache-Control".into(),
                format!("public, max-age=31536000, {WORDS}"),
            ),
        ],
        body: bytes::Bytes::from(format!(r#"{{"data":{{"{WORDS}":"{WORDS}"}}}}"#)),
    });
    f
}
