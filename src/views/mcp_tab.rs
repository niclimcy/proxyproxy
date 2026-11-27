use crate::viewmodels::intent::{McpCopy, McpIntent};
use crate::viewmodels::mcp_vm::McpVm;
use crate::views::theme::{checkbox, icon_only, muted, overline, status_dot, success, warning};
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
    copy_row(
        ui,
        vm,
        &mut out,
        McpCopy::Endpoint,
        "HTTP endpoint",
        &vm.endpoint,
    );
    copy_row(
        ui,
        vm,
        &mut out,
        McpCopy::ClaudeCode,
        "Claude Code",
        &vm.claude_command,
    );
    copy_row(
        ui,
        vm,
        &mut out,
        McpCopy::Stdio,
        "Command for stdio clients",
        &vm.stdio_command,
    );
    out
}

fn copy_row(
    ui: &mut egui::Ui,
    vm: &McpVm,
    out: &mut Vec<McpIntent>,
    what: McpCopy,
    label: &str,
    value: &str,
) {
    ui.add_space(6.0);
    ui.label(overline(label));
    ui.horizontal(|ui| {
        let copied = vm.copied(what);
        let (icon, tip) = if copied {
            (ph::CHECK, "Copied".to_string())
        } else {
            (ph::COPY, format!("Copy {label}"))
        };
        if icon_only(ui, icon, &tip).clicked() {
            ui.ctx().copy_text(value.to_string());
            out.push(McpIntent::Copied(what));
        }
        if copied {
            ui.ctx().request_repaint_after(Duration::from_millis(2100));
        }
        ui.add(egui::Label::new(RichText::new(value).monospace()).wrap());
    });
}
