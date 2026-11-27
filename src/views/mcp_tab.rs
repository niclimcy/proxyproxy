use crate::viewmodels::intent::{McpCopy, McpIntent};
use crate::viewmodels::mcp_vm::{McpClient, McpVm};
use crate::views::theme::{
    RADIUS, canvas, checkbox, icon_button, muted, overline, segmented, status_dot, success, warning,
};
use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;
use std::time::Duration;

pub fn show(ui: &mut egui::Ui, vm: &McpVm) -> Vec<McpIntent> {
    let mut out = vec![];
    ui.label(RichText::new("MCP server").strong());
    ui.label(muted(
        "Let AI assistants (Claude Code, Cursor, …) inspect your traffic through the Model Context Protocol.",
    ));
    ui.label(RichText::new("A connected tool can read every captured request and response, including tokens and passwords, replay requests and change rules. Any program on this computer can connect while this is on.").color(warning()));
    ui.add_space(6.0);
    let mut on = vm.enabled();
    if checkbox(ui, &mut on, "Enable MCP server").changed() {
        out.push(McpIntent::SetEnabled(on));
    }
    ui.horizontal(|ui| {
        if on {
            status_dot(ui, success(), "On");
            ui.label(muted(format!("Listening on {}", vm.endpoint)));
        } else {
            ui.label(muted(
                "Off — AI tools are refused. Turning it off blocks new requests immediately.",
            ));
        }
    });

    ui.separator();
    ui.label(RichText::new("Connect a client").strong());
    ui.add_space(4.0);
    let id = egui::Id::unique("mcp-client");
    let mut client = ui.data(|d| d.get_temp(id)).unwrap_or(McpClient::Custom);
    segmented(ui, &mut client, &McpClient::ALL.map(|c| (c, c.label())));
    ui.data_mut(|d| d.insert_temp(id, client));
    let snippet = vm.snippet(client);
    ui.add_space(6.0);
    ui.label(muted(snippet.hint));
    ui.add_space(4.0);
    code_block(
        ui,
        vm,
        &mut out,
        McpCopy::Client(client),
        &format!(
            "Copy {} {}",
            client.label(),
            if snippet.text.starts_with('{') {
                "config"
            } else {
                "command"
            }
        ),
        &snippet.text,
    );
    ui.add_space(8.0);
    ui.label(overline("HTTP endpoint"));
    ui.label(muted("For clients that take a URL"));
    code_block(
        ui,
        vm,
        &mut out,
        McpCopy::Endpoint,
        "Copy HTTP endpoint",
        &vm.endpoint,
    );
    out
}

fn code_block(
    ui: &mut egui::Ui,
    vm: &McpVm,
    out: &mut Vec<McpIntent>,
    what: McpCopy,
    tip: &str,
    value: &str,
) {
    egui::Frame::new()
        .fill(canvas())
        .corner_radius(egui::CornerRadius::same(RADIUS))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // full width so the copy button sits at the same right edge for every snippet
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let copied = vm.copied(what);
                let (icon, label) = if copied {
                    (ph::CHECK, "Copied")
                } else {
                    (ph::COPY, "Copy")
                };
                if icon_button(ui, icon, label, true)
                    .on_hover_text(tip)
                    .clicked()
                {
                    ui.ctx().copy_text(value.to_string());
                    out.push(McpIntent::Copied(what));
                }
                if copied {
                    ui.ctx().request_repaint_after(Duration::from_millis(2100));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.add(egui::Label::new(RichText::new(value).monospace()).wrap());
                });
            });
        });
}
