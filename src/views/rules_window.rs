use crate::domain::rule::Action;
use crate::viewmodels::intent::{ListKind, RuleIntent};
use crate::viewmodels::rules_vm::{KINDS, RuleForm, RuleKind, RulesTab, RulesVm};
use crate::views::theme::*;
use eframe::egui::{self, Color32, RichText, ScrollArea, Stroke, TextEdit};
use egui_phosphor::regular as ph;

fn field(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(label);
    add(ui);
    ui.end_row();
}

// `tab_body` draws the Schemas / Network tabs, which belong to other ViewModels
pub fn show(ctx: &egui::Context, vm: &mut RulesVm, tab_body: impl FnOnce(&mut egui::Ui, RulesTab)) -> Vec<RuleIntent> {
    let mut out = vec![];
    let mut open = vm.show;
    egui::Window::new("Rules & scope").open(&mut open).default_width(620.0).default_height(640.0).collapsible(false).show(ctx, |ui| {
        segmented(ui, &mut vm.tab, &[(RulesTab::Rules, "Rules"), (RulesTab::Scope, "Scope & mute"), (RulesTab::Console, "Script console"), (RulesTab::Schemas, "Schemas"), (RulesTab::Network, "Network")]);
        ui.add_space(6.0);
        match vm.tab {
            RulesTab::Rules => rules_tab(ui, vm, &mut out),
            RulesTab::Scope => scope_tab(ui, vm, &mut out),
            RulesTab::Console => console_tab(ui, vm, &mut out),
            t @ (RulesTab::Schemas | RulesTab::Network) => tab_body(ui, t),
        }
    });
    vm.show = open;
    out
}

fn rules_tab(ui: &mut egui::Ui, vm: &mut RulesVm, out: &mut Vec<RuleIntent>) {
    ui.label(muted("Match = URL pattern (plain text matches anywhere; * and ? are wildcards) + optional method. Right-click any request → Create rule from this request."));
    ui.add_space(6.0);
    let rules = vm.rules_snapshot();
    ScrollArea::vertical().max_height(220.0).id_salt("rule-list").show(ui, |ui| {
        if rules.is_empty() {
            ui.label(RichText::new("No rules yet").color(MUTED));
        }
        let n = rules.len();
        for (i, r) in rules.iter().enumerate() {
            card().inner_margin(8).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut enabled = r.enabled;
                    if ui.checkbox(&mut enabled, "").changed() {
                        out.push(RuleIntent::SetEnabled(r.id, enabled));
                    }
                    let c = match r.action {
                        Action::Breakpoint { .. } => AMBER,
                        Action::Block { .. } => RED,
                        Action::Script { .. } => PURPLE,
                        Action::Delay { .. } => BLUE,
                        _ => GREEN,
                    };
                    pill(ui, r.action.label(), c);
                    if !r.method.is_empty() {
                        pill(ui, &r.method.to_uppercase(), method_color(&r.method.to_uppercase()));
                    }
                    ui.label(RichText::new(&r.pattern).monospace());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let flat = |s: &str| egui::Button::new(s).fill(Color32::TRANSPARENT).stroke(Stroke::NONE);
                        if ui.add(flat(ph::X)).on_hover_text("Delete").clicked() {
                            out.push(RuleIntent::Delete(r.id));
                        }
                        if ui.add(flat(ph::PENCIL)).on_hover_text("Edit").clicked() {
                            out.push(RuleIntent::Edit(r.id));
                        }
                        if i + 1 < n && ui.add(flat(ph::ARROW_DOWN)).on_hover_text("Lower priority").clicked() {
                            out.push(RuleIntent::Move(i, 1));
                        }
                        if i > 0 && ui.add(flat(ph::ARROW_UP)).on_hover_text("Higher priority").clicked() {
                            out.push(RuleIntent::Move(i, -1));
                        }
                        if r.hits > 0 {
                            ui.label(muted(format!("{} hits", r.hits)));
                        }
                    });
                });
                ui.label(muted(if r.name.is_empty() { r.action.describe() } else { format!("{} — {}", r.name, r.action.describe()) }));
            });
        }
    });

    ui.add_space(6.0);
    ui.separator();
    let editing = vm.editing.is_some();
    ui.horizontal(|ui| {
        ui.label(RichText::new(if editing { "Edit rule" } else { "New rule" }).strong());
        if editing && ui.small_button("Cancel edit").clicked() {
            out.push(RuleIntent::CancelEdit);
        }
    });
    let live = vm.live_matches();
    form_ui(ui, &mut vm.form);
    ui.horizontal(|ui| {
        let label = if editing { "Save rule" } else { "Add rule" };
        if ui.add_enabled(vm.form.valid(), egui::Button::new(RichText::new(ico(ph::PLUS, label)).strong()).fill(ACCENT.gamma_multiply(0.5))).clicked() {
            out.push(RuleIntent::Submit);
        }
        if !vm.form.pattern.trim().is_empty() {
            ui.label(muted(format!("matches {live} captured request{}", if live == 1 { "" } else { "s" })));
        }
    });
}

fn form_ui(ui: &mut egui::Ui, f: &mut RuleForm) {
    ui.horizontal_wrapped(|ui| {
        for (k, l) in KINDS {
            if ui.selectable_label(f.kind == k, l).clicked() {
                f.kind = k;
            }
        }
    });
    let hint = match f.kind {
        RuleKind::Breakpoint => "Pause matching traffic and edit it before it continues",
        RuleKind::Mock => "Reply with a fixed response; the server is never contacted",
        RuleKind::MapLocal => "Reply with the contents of a local file",
        RuleKind::Redirect => "Send the request to a different host (e.g. localhost:3000), keeping path",
        RuleKind::Block => "Reply immediately with an error status",
        RuleKind::Throttle => "Add latency and/or cap response bandwidth",
        RuleKind::Header => "Add, replace or remove one header",
        RuleKind::Body => "Find & replace text in the body",
        RuleKind::Script => "JavaScript hooks: onRequest(ctx) / onResponse(ctx)",
    };
    ui.label(muted(hint));
    egui::Grid::new("rule-form").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        field(ui, "Name", |ui| {
            ui.add(TextEdit::singleline(&mut f.name).desired_width(380.0).hint_text("optional"));
        });
        field(ui, "URL pattern", |ui| {
            ui.add(TextEdit::singleline(&mut f.pattern).desired_width(380.0).hint_text("api.example.com/users*"));
        });
        field(ui, "Method", |ui| {
            ui.add(TextEdit::singleline(&mut f.method).desired_width(100.0).hint_text("any"));
        });
        match f.kind {
            RuleKind::Breakpoint => field(ui, "Pause on", |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut f.bp_request, "Request");
                    ui.checkbox(&mut f.bp_response, "Response");
                });
            }),
            RuleKind::MapLocal => {
                field(ui, "Local file", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(TextEdit::singleline(&mut f.file).desired_width(300.0).hint_text("/path/to/response.json"));
                        if ui.button("Browse…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().pick_file() {
                                f.file = p.display().to_string();
                            }
                        }
                    });
                });
                field(ui, "Status", |ui| {
                    ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
                });
            }
            RuleKind::Redirect => field(ui, "Target", |ui| {
                ui.add(TextEdit::singleline(&mut f.target).desired_width(380.0).hint_text("http://localhost:3000"));
            }),
            RuleKind::Block => field(ui, "Status", |ui| {
                ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
            }),
            RuleKind::Throttle => {
                field(ui, "Latency (ms)", |ui| {
                    ui.add(TextEdit::singleline(&mut f.ms).desired_width(80.0));
                });
                field(ui, "Max KB/s", |ui| {
                    ui.add(TextEdit::singleline(&mut f.kbps).desired_width(80.0).hint_text("0 = unlimited"));
                });
            }
            RuleKind::Header => {
                field(ui, "Apply to", |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut f.resp, false, "Request");
                        ui.selectable_value(&mut f.resp, true, "Response");
                    });
                });
                field(ui, "Header", |ui| {
                    ui.add(TextEdit::singleline(&mut f.hname).desired_width(220.0).hint_text("Authorization"));
                });
                field(ui, "Value", |ui| {
                    ui.horizontal(|ui| {
                        ui.add_enabled(!f.hremove, TextEdit::singleline(&mut f.hvalue).desired_width(240.0));
                        ui.checkbox(&mut f.hremove, "Remove");
                    });
                });
            }
            RuleKind::Body => {
                field(ui, "Apply to", |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut f.resp, false, "Request");
                        ui.selectable_value(&mut f.resp, true, "Response");
                        ui.checkbox(&mut f.regex, "Regex");
                    });
                });
                field(ui, "Find", |ui| {
                    ui.add(TextEdit::singleline(&mut f.find).desired_width(380.0));
                });
                field(ui, "Replace", |ui| {
                    ui.add(TextEdit::singleline(&mut f.replace).desired_width(380.0));
                });
            }
            RuleKind::Mock => {
                field(ui, "Status", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
                        ui.label("Content-Type");
                        ui.add(TextEdit::singleline(&mut f.ctype).desired_width(200.0));
                    });
                });
                field(ui, "Body", |ui| {
                    ui.add(TextEdit::multiline(&mut f.body).code_editor().desired_rows(6).desired_width(380.0));
                });
            }
            RuleKind::Script => field(ui, "Code", |ui| {
                ui.add(TextEdit::multiline(&mut f.code).code_editor().desired_rows(12).desired_width(420.0));
            }),
        }
    });
}

fn scope_tab(ui: &mut egui::Ui, vm: &mut RulesVm, out: &mut Vec<RuleIntent>) {
    let sv = vm.scope_view();
    let mut scope_only = sv.scope_only;
    if ui.checkbox(&mut scope_only, "Capture only the scope domains below (everything else passes through unrecorded)").changed() {
        out.push(RuleIntent::SetScopeOnly(scope_only));
    }
    let mut section = |ui: &mut egui::Ui, title: &str, hint: &str, kind: ListKind, list: &[String], slot: usize| {
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong());
        ui.label(muted(hint));
        for (i, h) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(h).monospace());
                if ui.small_button(ph::X).clicked() {
                    out.push(RuleIntent::RemoveHost(kind, i));
                }
            });
        }
        ui.horizontal(|ui| {
            let input = &mut vm.list_inputs[slot];
            let r = ui.add(TextEdit::singleline(input).hint_text("domain or *.wildcard").desired_width(260.0));
            if ui.button("Add").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                out.push(RuleIntent::AddHost(kind));
            }
        });
    };
    section(ui, "Scope", "Domains to capture when scope is on", ListKind::Scope, &sv.scope, 0);
    section(ui, "Muted", "Never captured or shown (analytics, telemetry…). Traffic still passes through.", ListKind::Muted, &sv.muted, 1);
    section(ui, "No TLS decryption", "Tunnelled as-is — for pinned apps. Applies to new connections.", ListKind::BypassTls, &sv.bypass_tls, 2);
}

fn console_tab(ui: &mut egui::Ui, vm: &RulesVm, out: &mut Vec<RuleIntent>) {
    ui.horizontal(|ui| {
        ui.label(muted("console.log() output and script errors"));
        if ui.small_button("Clear").clicked() {
            out.push(RuleIntent::ClearConsole);
        }
    });
    ScrollArea::vertical().id_salt("console").stick_to_bottom(true).max_height(480.0).auto_shrink([false, false]).show(ui, |ui| {
        if vm.console.is_empty() {
            ui.label(muted("Nothing logged yet. Add a Script rule with onRequest/onResponse."));
        }
        for l in &vm.console {
            let c = if l.starts_with("[error]") { RED } else { TEXT };
            ui.label(RichText::new(l).monospace().size(12.0).color(c));
        }
    });
}
