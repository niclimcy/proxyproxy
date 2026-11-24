use crate::domain::rule::Action;
use crate::viewmodels::intent::{ListKind, RuleIntent};
use crate::viewmodels::rules_vm::{KINDS, RuleForm, RuleKind, RulesTab, RulesVm};
use crate::views::theme::*;
use eframe::egui::{self, RichText, ScrollArea, TextEdit};
use egui_phosphor::regular as ph;

fn field(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(label);
    add(ui);
    ui.end_row();
}

pub fn show(ui: &mut egui::Ui, vm: &mut RulesVm) -> Vec<RuleIntent> {
    let mut out = vec![];
    let mut open = vm.show;
    let mut close = false;
    egui::Panel::right("rules")
        .resizable(open)
        .default_size(440.0)
        .size_range(360.0..=900.0)
        .frame(panel_frame().inner_margin(egui::Margin::same(PAD)))
        .show_collapsible(ui, &mut open, |ui| {
            ui.horizontal(|ui| {
                ui.label(overline("RULES & SCOPE"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, ph::X, "", true)
                        .on_hover_text("Close")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
            segmented(
                ui,
                &mut vm.tab,
                &[
                    (RulesTab::Rules, "Rules"),
                    (RulesTab::Scope, "Scope"),
                    (RulesTab::Console, "Console"),
                ],
            );
            ui.add_space(6.0);
            ScrollArea::vertical()
                .id_salt("rules-body")
                .auto_shrink([false, false])
                .show(ui, |ui| match vm.tab {
                    RulesTab::Rules => rules_tab(ui, vm, &mut out),
                    RulesTab::Scope => scope_tab(ui, vm, &mut out),
                    RulesTab::Console => console_tab(ui, vm, &mut out),
                });
        });
    vm.show = open && !close;
    out
}

fn rules_tab(ui: &mut egui::Ui, vm: &mut RulesVm, out: &mut Vec<RuleIntent>) {
    ui.label(muted("Match = URL pattern (plain text matches anywhere; * and ? are wildcards) + optional method. Right-click any request → Create rule from this request."));
    ui.add_space(6.0);
    let rules = vm.rules_snapshot();
    ScrollArea::vertical()
        .max_height(220.0)
        .id_salt("rule-list")
        .show(ui, |ui| {
            if rules.is_empty() {
                ui.label(RichText::new("No rules yet").color(fg_muted()));
            }
            let n = rules.len();
            for (i, r) in rules.iter().enumerate() {
                card().inner_margin(8).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut enabled = r.enabled;
                        if checkbox(ui, &mut enabled, "").changed() {
                            out.push(RuleIntent::SetEnabled(r.id, enabled));
                        }
                        let c = match r.action {
                            Action::Breakpoint { .. } => warning(),
                            Action::Block { .. } => danger(),
                            Action::Script { .. } => violet(),
                            Action::Delay { .. } => info(),
                            _ => success(),
                        };
                        pill(ui, r.action.label(), c);
                        if !r.method.is_empty() {
                            pill(
                                ui,
                                &r.method.to_uppercase(),
                                method_color(&r.method.to_uppercase()),
                            );
                        }
                        ui.label(RichText::new(&r.pattern).monospace());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let tool = |ui: &mut egui::Ui, icon: &str, tip: &str| {
                                icon_button(ui, icon, "", true).on_hover_text(tip).clicked()
                            };
                            if tool(ui, ph::X, "Delete") {
                                out.push(RuleIntent::Delete(r.id));
                            }
                            if tool(ui, ph::PENCIL, "Edit") {
                                out.push(RuleIntent::Edit(r.id));
                            }
                            if i + 1 < n && tool(ui, ph::ARROW_DOWN, "Lower priority") {
                                out.push(RuleIntent::Move(i, 1));
                            }
                            if i > 0 && tool(ui, ph::ARROW_UP, "Higher priority") {
                                out.push(RuleIntent::Move(i, -1));
                            }
                            if r.hits > 0 {
                                ui.label(muted(format!("{} hits", r.hits)));
                            }
                        });
                    });
                    ui.label(muted(if r.name.is_empty() {
                        r.action.describe()
                    } else {
                        format!("{} — {}", r.name, r.action.describe())
                    }));
                });
            }
        });

    ui.add_space(6.0);
    ui.separator();
    let editing = vm.editing.is_some();
    ui.horizontal(|ui| {
        ui.label(RichText::new(if editing { "Edit rule" } else { "New rule" }).strong());
        if editing && icon_button(ui, ph::X, "Cancel edit", true).clicked() {
            out.push(RuleIntent::CancelEdit);
        }
    });
    let live = vm.live_matches();
    form_ui(ui, &mut vm.form, out);
    ui.horizontal(|ui| {
        let (icon, label) = if editing {
            (ph::CHECK, "Save rule")
        } else {
            (ph::PLUS, "Add rule")
        };
        if ui
            .add_enabled(vm.form.valid(), action_button(ico(icon, label), accent()))
            .on_disabled_hover_text("Fill in the URL pattern and this rule's required fields")
            .clicked()
        {
            out.push(RuleIntent::Submit);
        }
        if !vm.form.pattern.trim().is_empty() {
            ui.label(muted(format!(
                "matches {live} captured request{}",
                if live == 1 { "" } else { "s" }
            )));
        }
    });
}

fn form_ui(ui: &mut egui::Ui, f: &mut RuleForm, out: &mut Vec<RuleIntent>) {
    let w = (ui.available_width() - 100.0).clamp(120.0, 700.0);
    let kind_picker = |ui: &mut egui::Ui, kind: &mut RuleKind| {
        let current = KINDS
            .iter()
            .find(|(k, _)| k == kind)
            .map_or("", |(_, l)| l);
        egui::ComboBox::from_id_salt("rule-kind")
            .selected_text(current)
            .width(160.0)
            .show_ui(ui, |ui| {
                for (k, l) in KINDS {
                    ui.selectable_value(kind, k, l);
                }
            });
    };
    let hint = match f.kind {
        RuleKind::Breakpoint => "Pause matching traffic and edit it before it continues",
        RuleKind::Mock => "Reply with a fixed response; the server is never contacted",
        RuleKind::MapLocal => "Reply with the contents of a local file",
        RuleKind::Redirect => {
            "Send the request to a different host (e.g. localhost:3000), keeping path"
        }
        RuleKind::Block => "Reply immediately with an error status",
        RuleKind::Throttle => "Add latency and/or cap response bandwidth",
        RuleKind::Header => "Add, replace or remove one header",
        RuleKind::Body => "Find & replace text in the body",
        RuleKind::Script => "JavaScript hooks: onRequest(ctx) / onResponse(ctx)",
    };
    egui::Grid::new("rule-form")
        .num_columns(2)
        .spacing([10.0, 8.0])
        .show(ui, |ui| {
            field(ui, "Type", |ui| kind_picker(ui, &mut f.kind));
            field(ui, "", |ui| {
                ui.label(muted(hint));
            });
            field(ui, "Name", |ui| {
                ui.add(
                    TextEdit::singleline(&mut f.name)
                        .desired_width(w)
                        .hint_text("optional"),
                );
            });
            field(ui, "URL pattern", |ui| {
                ui.add(
                    TextEdit::singleline(&mut f.pattern)
                        .desired_width(w)
                        .hint_text("api.example.com/users*"),
                );
            });
            field(ui, "Method", |ui| {
                ui.add(
                    TextEdit::singleline(&mut f.method)
                        .desired_width(100.0)
                        .hint_text("any"),
                );
            });
            match f.kind {
                RuleKind::Breakpoint => field(ui, "Pause on", |ui| {
                    ui.horizontal(|ui| {
                        checkbox(ui, &mut f.bp_request, "Request");
                        checkbox(ui, &mut f.bp_response, "Response");
                    });
                }),
                RuleKind::MapLocal => {
                    field(ui, "Local file", |ui| {
                        ui.horizontal(|ui| {
                            let path = ui.add(
                                TextEdit::singleline(&mut f.file)
                                    .desired_width((w - 80.0).max(80.0))
                                    .hint_text("/path/to/response.json"),
                            );
                            if icon_button(ui, ph::FOLDER_OPEN, "Browse…", true).clicked()
                                && let Some(p) = rfd::FileDialog::new().pick_file()
                            {
                                f.file = p.display().to_string();
                                out.push(RuleIntent::LoadMapLocalFile);
                            }
                            if path.lost_focus() {
                                out.push(RuleIntent::LoadMapLocalFile);
                            }
                        });
                    });
                    field(ui, "Status", |ui| {
                        ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
                    });
                    field(ui, "File contents", |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if icon_button(ui, ph::FOLDER_OPEN, "Load file", true)
                                    .on_hover_text("Read the selected file from disk")
                                    .clicked()
                                {
                                    out.push(RuleIntent::LoadMapLocalFile);
                                }
                                if icon_button(
                                    ui,
                                    ph::FLOPPY_DISK,
                                    "Save to disk",
                                    f.map_file_editable(),
                                )
                                .on_hover_text(
                                    "Write this editor's contents to the selected file on disk",
                                )
                                .clicked()
                                {
                                    out.push(RuleIntent::SaveMapLocalFile);
                                }
                            });
                            ui.label(muted(
                                "Save to disk writes directly to the selected local file.",
                            ));
                            if let Some(error) = &f.map_file_error {
                                ui.label(RichText::new(error).color(danger()));
                            } else if let Some(status) = &f.map_file_status {
                                ui.label(RichText::new(status).color(success()));
                            }
                            if f.map_file_editable() {
                                let ext = f.map_file_ext();
                                ui.label(muted(if ext.is_empty() {
                                    "Text file".to_string()
                                } else {
                                    format!("{} file", ext.to_uppercase())
                                }));
                                let mut layouter =
                                    |ui: &egui::Ui,
                                     text: &dyn egui::TextBuffer,
                                     wrap_width: f32| {
                                        code_layouter(ui, text, wrap_width, &ext)
                                    };
                                let editor = TextEdit::multiline(&mut f.map_file_content)
                                    .code_editor()
                                    .desired_rows(10)
                                    .desired_width(w)
                                    .layouter(&mut layouter);
                                if ui.add(editor).changed() {
                                    f.map_file_dirty = true;
                                    f.map_file_status = None;
                                    f.map_file_error = None;
                                }
                                if f.map_file_dirty {
                                    ui.label(muted(
                                        "Unsaved changes — Save to disk will update the file.",
                                    ));
                                }
                            } else if !f.file.trim().is_empty() {
                                ui.label(muted("Load file to edit and save its contents."));
                            }
                        });
                    });
                }
                RuleKind::Redirect => field(ui, "Target", |ui| {
                    ui.add(
                        TextEdit::singleline(&mut f.target)
                            .desired_width(w)
                            .hint_text("http://localhost:3000"),
                    );
                }),
                RuleKind::Block => field(ui, "Status", |ui| {
                    ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
                }),
                RuleKind::Throttle => {
                    field(ui, "Latency (ms)", |ui| {
                        ui.add(TextEdit::singleline(&mut f.ms).desired_width(80.0));
                    });
                    field(ui, "Max KB/s", |ui| {
                        ui.add(
                            TextEdit::singleline(&mut f.kbps)
                                .desired_width(80.0)
                                .hint_text("0 = unlimited"),
                        );
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
                        ui.add(
                            TextEdit::singleline(&mut f.hname)
                                .desired_width(220.0)
                                .hint_text("Authorization"),
                        );
                    });
                    field(ui, "Value", |ui| {
                        ui.horizontal(|ui| {
                            ui.add_enabled(
                                !f.hremove,
                                TextEdit::singleline(&mut f.hvalue)
                                    .desired_width((w - 90.0).max(80.0)),
                            );
                            checkbox(ui, &mut f.hremove, "Remove");
                        });
                    });
                }
                RuleKind::Body => {
                    field(ui, "Apply to", |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut f.resp, false, "Request");
                            ui.selectable_value(&mut f.resp, true, "Response");
                            checkbox(ui, &mut f.regex, "Regex");
                        });
                    });
                    field(ui, "Find", |ui| {
                        ui.add(TextEdit::singleline(&mut f.find).desired_width(w));
                    });
                    field(ui, "Replace", |ui| {
                        ui.add(TextEdit::singleline(&mut f.replace).desired_width(w));
                    });
                }
                RuleKind::Mock => {
                    field(ui, "Status", |ui| {
                        ui.horizontal(|ui| {
                            ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
                            ui.label("Content-Type");
                            ui.add(
                                TextEdit::singleline(&mut f.ctype)
                                    .desired_width((w - 150.0).max(80.0)),
                            );
                        });
                    });
                    field(ui, "Body", |ui| {
                        ui.add(
                            TextEdit::multiline(&mut f.body)
                                .code_editor()
                                .desired_rows(6)
                                .desired_width(w),
                        );
                    });
                }
                RuleKind::Script => field(ui, "Code", |ui| {
                    let mut layouter =
                        |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                            code_layouter(ui, text, wrap_width, "javascript")
                        };
                    ui.add(
                        TextEdit::multiline(&mut f.code)
                            .code_editor()
                            .desired_rows(12)
                            .desired_width(w)
                            .layouter(&mut layouter),
                    );
                }),
            }
        });
}

fn scope_tab(ui: &mut egui::Ui, vm: &mut RulesVm, out: &mut Vec<RuleIntent>) {
    let sv = vm.scope_view();
    let mut scope_only = sv.scope_only;
    if checkbox(
        ui,
        &mut scope_only,
        "Record only the scope entries below (everything else passes through unrecorded)",
    )
    .changed()
    {
        out.push(RuleIntent::SetScopeOnly(scope_only));
    }
    ui.label(muted(format!(
        "{} of {} recorded requests match the current scope, exclude and mute lists",
        sv.matching, sv.total
    )));
    let mut section = |ui: &mut egui::Ui,
                       title: &str,
                       hint: &str,
                       kind: ListKind,
                       list: &[String],
                       slot: usize| {
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong());
        ui.label(muted(hint));
        for (i, h) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(h).monospace());
                if icon_button(ui, ph::X, "", true).clicked() {
                    out.push(RuleIntent::RemoveHost(kind, i));
                }
            });
        }
        ui.horizontal(|ui| {
            let input = &mut vm.list_inputs[slot];
            let r = ui.add(
                TextEdit::singleline(input)
                    .hint_text("domain, host/path/* or https://*.host/path")
                    .desired_width(260.0),
            );
            if icon_button(ui, ph::PLUS, "Add", true).clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                out.push(RuleIntent::AddHost(kind));
            }
        });
    };
    section(
        ui,
        "Scope",
        "Hosts or URL patterns (* is a wildcard) to record when scope is on. A bare host covers its subdomains.",
        ListKind::Scope,
        &sv.scope,
        0,
    );
    section(
        ui,
        "Exclude",
        "Never recorded, even inside the scope. Same patterns as Scope. Traffic still passes through.",
        ListKind::Exclude,
        &sv.exclude,
        1,
    );
    section(
        ui,
        "Muted",
        "Never captured or shown (analytics, telemetry…). Traffic still passes through.",
        ListKind::Muted,
        &sv.muted,
        2,
    );
    section(
        ui,
        "No TLS decryption",
        "Tunnelled as-is — for pinned apps. Applies to new connections.",
        ListKind::BypassTls,
        &sv.bypass_tls,
        3,
    );
}

fn console_tab(ui: &mut egui::Ui, vm: &RulesVm, out: &mut Vec<RuleIntent>) {
    ui.horizontal(|ui| {
        ui.label(muted("console.log() output and script errors"));
        if icon_button(ui, ph::TRASH, "Clear", true).clicked() {
            out.push(RuleIntent::ClearConsole);
        }
    });
    ScrollArea::vertical()
        .id_salt("console")
        .stick_to_bottom(true)
        .max_height(480.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if vm.console.is_empty() {
                ui.label(muted(
                    "Nothing logged yet. Add a Script rule with onRequest/onResponse.",
                ));
            }
            for l in &vm.console {
                let c = if l.starts_with("[error]") {
                    danger()
                } else {
                    fg()
                };
                ui.label(RichText::new(l).monospace().size(FS_CAPTION).color(c));
            }
        });
}
