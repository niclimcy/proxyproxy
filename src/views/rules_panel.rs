use crate::viewmodels::intent::{ListKind, RuleIntent};
use crate::viewmodels::rules_vm::{
    KINDS, RuleForm, RuleGroup, RuleKind, RuleRow, RulesTab, RulesVm, Tone,
};
use crate::views::theme::{
    FS_CAPTION, accent, action_button, canvas, card, card_panel, checkbox, code_layouter, danger,
    fg, fg_muted, ico, icon_button, icon_only, info, menu_item, menu_width, method_color, muted,
    overflow_menu, overline, pill, resize_grip, resize_line, segmented, success, violet, warning,
};
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
    resize_line(ui, true);
    let shown = egui::Panel::right("rules")
        .resizable(open)
        .default_size(440.0)
        .size_range(360.0..=900.0)
        .show_separator_line(false)
        .frame(card_panel())
        .show_collapsible(ui, &mut open, |ui| {
            resize_line(ui, false);
            ui.horizontal(|ui| {
                ui.label(overline("RULES & SCOPE"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_only(ui, ph::X, "Close rules panel").clicked() {
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
    resize_line(ui, false);
    if let Some(r) = shown {
        let r = r.response.rect;
        resize_grip(
            ui,
            "rules",
            egui::Rect::from_two_pos(r.left_top(), r.left_bottom()),
        );
    }
    vm.show = open && !close;
    out
}

fn rules_tab(ui: &mut egui::Ui, vm: &mut RulesVm, out: &mut Vec<RuleIntent>) {
    ui.label(muted("Match = URL pattern (plain text matches anywhere; * and ? are wildcards) + optional method. Right-click any request → Create rule from this request."));
    ui.add_space(6.0);
    let list = vm.rule_list();
    let grouped = !list.collections.is_empty();
    new_collection_row(ui, vm, out);
    if let Some(summary) = &list.summary {
        ui.horizontal(|ui| {
            ui.label(muted(summary));
            if !list.all_on && ui.link("Turn all on").clicked() {
                out.push(RuleIntent::EnableAllCollections);
            }
        });
    }
    ui.add_space(4.0);
    ScrollArea::vertical()
        .max_height(260.0)
        .id_salt("rule-list")
        .show(ui, |ui| {
            if !grouped && list.groups.iter().all(|g| g.rows.is_empty()) {
                ui.label(RichText::new("No rules yet").color(fg_muted()));
                ui.label(muted(
                    "Group rules into collections to switch between scenarios.",
                ));
            }
            for g in &list.groups {
                if grouped {
                    group_header(ui, vm, g, out);
                    if vm.collapsed.contains(&g.id) {
                        continue;
                    }
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: if grouped { 16 } else { 0 },
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        if grouped && g.rows.is_empty() && ui.link("+ Add a rule").clicked() {
                            out.push(RuleIntent::NewRuleIn(g.id));
                        }
                        let last = g.rows.len().saturating_sub(1);
                        for (pos, r) in g.rows.iter().enumerate() {
                            let order = Order {
                                can_up: pos > 0,
                                can_down: pos < last,
                            };
                            rule_card(ui, r, &order, !g.enabled, &list.collections, out);
                        }
                    });
            }
        });

    ui.add_space(6.0);
    ui.separator();
    let editing = vm.editing.is_some();
    let target = |c: Option<u64>| {
        c.and_then(|id| list.collections.iter().find(|(i, _)| *i == id))
            .map(|(_, n)| n.as_str())
    };
    ui.horizontal(|ui| {
        let title = if editing {
            "Edit rule".to_string()
        } else if !grouped {
            "New rule".to_string()
        } else if let Some(name) = target(vm.form.collection) {
            format!("New rule in {name}")
        } else {
            "New rule with no collection".to_string()
        };
        ui.label(RichText::new(title).strong());
        if editing && icon_button(ui, ph::X, "Cancel edit", true).clicked() {
            out.push(RuleIntent::CancelEdit);
        }
    });
    let live = vm.live_matches();
    form_ui(ui, &mut vm.form, &list.collections, out);
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

fn new_collection_row(ui: &mut egui::Ui, vm: &mut RulesVm, out: &mut Vec<RuleIntent>) {
    if !vm.adding_collection {
        if icon_button(ui, ph::FOLDER_SIMPLE_PLUS, "New collection", true).clicked() {
            vm.adding_collection = true;
            vm.rename_focus = true;
        }
        return;
    }
    ui.horizontal(|ui| {
        let edit = ui.add(
            TextEdit::singleline(&mut vm.new_collection)
                .desired_width(220.0)
                .hint_text("e.g. Checkout happy path"),
        );
        if std::mem::take(&mut vm.rename_focus) {
            edit.request_focus();
        }
        let ready = !vm.new_collection.trim().is_empty();
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if (icon_button(ui, ph::CHECK, "Add collection", ready).clicked() || enter) && ready {
            out.push(RuleIntent::AddCollection);
        }
        if icon_button(ui, ph::X, "Cancel", true).clicked() || escape {
            vm.adding_collection = false;
            vm.new_collection.clear();
        }
    });
}

struct Order {
    can_up: bool,
    can_down: bool,
}

fn rule_card(
    ui: &mut egui::Ui,
    r: &RuleRow,
    order: &Order,
    collection_off: bool,
    collections: &[(u64, String)],
    out: &mut Vec<RuleIntent>,
) {
    card().inner_margin(8).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mut enabled = r.enabled;
            let on = checkbox(ui, &mut enabled, "");
            let who = if r.name.is_empty() {
                &r.pattern
            } else {
                &r.name
            };
            let spoken = if collection_off {
                format!("Rule {who} (collection is off)")
            } else {
                format!("Rule {who}")
            };
            on.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::CheckBox, true, enabled, spoken.clone())
            });
            if on.changed() {
                out.push(RuleIntent::SetEnabled(r.id, enabled));
            }
            let c = match r.tone {
                Tone::Warning => warning(),
                Tone::Danger => danger(),
                Tone::Violet => violet(),
                Tone::Info => info(),
                Tone::Success => success(),
            };
            pill(ui, r.label, c);
            if !r.method.is_empty() {
                pill(
                    ui,
                    &r.method.to_uppercase(),
                    method_color(&r.method.to_uppercase()),
                );
            }
            ui.label(RichText::new(&r.pattern).monospace());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_only(ui, ph::X, "Delete").clicked() {
                    out.push(RuleIntent::Delete(r.id));
                }
                if icon_only(ui, ph::PENCIL, "Edit").clicked() {
                    out.push(RuleIntent::Edit(r.id));
                }
                if order.can_up || order.can_down || !collections.is_empty() {
                    overflow_menu(ui, "Rule actions", |ui| {
                        rule_menu(ui, r.id, order, collections, out);
                    });
                }
                if r.hits > 0 {
                    ui.label(muted(format!("{} hits", r.hits)));
                }
            });
        });
        ui.label(muted(if r.name.is_empty() {
            r.description.clone()
        } else {
            format!("{} — {}", r.name, r.description)
        }));
    });
}

fn rule_menu(
    ui: &mut egui::Ui,
    id: u64,
    order: &Order,
    collections: &[(u64, String)],
    out: &mut Vec<RuleIntent>,
) {
    let names: Vec<&str> = collections.iter().map(|(_, n)| n.as_str()).collect();
    menu_width(
        ui,
        &[
            &[
                "Higher priority",
                "Lower priority",
                "Move to collection",
                "No collection",
            ],
            &names[..],
        ]
        .concat(),
    );
    let priority = "Rules run top to bottom: the order is the global priority";
    if order.can_up
        && menu_item(ui, "Higher priority", None)
            .on_hover_text(priority)
            .clicked()
    {
        out.push(RuleIntent::Move(id, -1));
    }
    if order.can_down
        && menu_item(ui, "Lower priority", None)
            .on_hover_text(priority)
            .clicked()
    {
        out.push(RuleIntent::Move(id, 1));
    }
    if !collections.is_empty() {
        ui.label(muted("Move to collection"));
        let targets = std::iter::once((None, "No collection"))
            .chain(collections.iter().map(|(id, n)| (Some(*id), n.as_str())));
        for (target, name) in targets {
            if menu_item(ui, name, None).clicked() {
                out.push(RuleIntent::SetRuleCollection(id, target));
            }
        }
    }
}

fn group_header(ui: &mut egui::Ui, vm: &mut RulesVm, g: &RuleGroup, out: &mut Vec<RuleIntent>) {
    let collapsed = vm.collapsed.contains(&g.id);
    let mut start_rename = false;
    let strip = card()
        .fill(canvas())
        .stroke(egui::Stroke::NONE)
        .inner_margin(4)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_only(ui, ph::PLUS, "New rule in this collection").clicked() {
                    out.push(RuleIntent::NewRuleIn(g.id));
                }
                if let Some(id) = g.id {
                    overflow_menu(ui, "Collection actions", |ui| {
                        menu_width(ui, &["Rename", "Delete…"]);
                        if menu_item(ui, "Rename", None).clicked() {
                            start_rename = true;
                        }
                        if menu_item(ui, "Delete…", None).clicked() {
                            if g.rows.is_empty() {
                                out.push(RuleIntent::DeleteCollection(id, false));
                            } else {
                                vm.confirm_delete = Some(id);
                            }
                        }
                    });
                    let only = icon_button(ui, "", "Only", true).on_hover_text(
                        "Turn this collection on and every other collection off. Rules with no collection stay on.",
                    );
                    if only.clicked() {
                        out.push(RuleIntent::SoloCollection(id));
                    }
                }
                let n = g.rows.len();
                let plural = if n == 1 { "" } else { "s" };
                ui.label(muted(match (g.id, g.enabled) {
                    (None, _) => format!("{n} rule{plural}"),
                    (Some(_), true) => format!("{n} rule{plural} · {} on", g.on),
                    (Some(_), false) => format!("Off · {n} rule{plural}"),
                }));
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let (icon, tip) = if collapsed {
                        (ph::CARET_RIGHT, "Expand")
                    } else {
                        (ph::CARET_DOWN, "Collapse")
                    };
                    if icon_only(ui, icon, tip).clicked() && !vm.collapsed.remove(&g.id) {
                        vm.collapsed.insert(g.id);
                    }
                    if let Some(id) = g.id {
                        let mut on = g.enabled;
                        let toggle = checkbox(ui, &mut on, "")
                            .on_hover_text("Turn every rule in this collection on or off");
                        toggle.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::CheckBox,
                                true,
                                g.enabled,
                                format!("Turn on collection {}", g.name),
                            )
                        });
                        if toggle.changed() {
                            out.push(RuleIntent::SetCollectionEnabled(id, on));
                        }
                    }
                    if let Some(id) = g.id.filter(|id| vm.renaming == Some(*id)) {
                        rename_field(ui, vm, id, out);
                    } else {
                        let name = if g.id.is_some() {
                            g.name.clone()
                        } else {
                            format!("{} · always on", g.name)
                        };
                        let text = RichText::new(name).strong();
                        let label = ui.add(
                            egui::Label::new(if g.enabled { text.color(fg()) } else { text.color(fg_muted()) })
                                .truncate()
                                .sense(egui::Sense::click()),
                        );
                        if g.id.is_some() && label.double_clicked() {
                            start_rename = true;
                        }
                    }
                    if !g.enabled {
                        pill(ui, "Off", fg_muted());
                    }
                });
            });
        });
    if vm.working == g.id {
        let r = strip.response.rect;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(r.left_top(), egui::vec2(3.0, r.height())),
            0.0,
            accent(),
        );
    }
    if start_rename && let Some(id) = g.id {
        vm.renaming = Some(id);
        vm.rename_input.clone_from(&g.name);
        vm.rename_focus = true;
    }
    if let Some(id) = g.id.filter(|id| vm.confirm_delete == Some(*id)) {
        delete_confirm(ui, vm, id, g, out);
    }
}

fn rename_field(ui: &mut egui::Ui, vm: &mut RulesVm, id: u64, out: &mut Vec<RuleIntent>) {
    let mut edit = TextEdit::singleline(&mut vm.rename_input)
        .desired_width(160.0)
        .show(ui);
    if std::mem::take(&mut vm.rename_focus) {
        edit.response.request_focus();
        let end = vm.rename_input.chars().count();
        edit.state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(end),
            )));
        edit.state.store(ui.ctx(), edit.response.id);
    }
    // Escape also drops focus this frame, so it has to win over the lost-focus commit
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        vm.renaming = None;
    } else if edit.response.lost_focus() {
        out.push(RuleIntent::RenameCollection(
            id,
            std::mem::take(&mut vm.rename_input),
        ));
        vm.renaming = None;
    }
}

fn delete_confirm(
    ui: &mut egui::Ui,
    vm: &mut RulesVm,
    id: u64,
    g: &RuleGroup,
    out: &mut Vec<RuleIntent>,
) {
    let n = g.rows.len();
    let rules = format!("{n} rule{}", if n == 1 { "" } else { "s" });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        vm.confirm_delete = None;
        return;
    }
    card().inner_margin(8).show(ui, |ui| {
        ui.label(format!("Delete collection \"{}\"?", g.name));
        ui.horizontal_wrapped(|ui| {
            if icon_button(
                ui,
                ph::FOLDER_SIMPLE,
                &format!("Move {rules} to No collection"),
                true,
            )
            .clicked()
            {
                out.push(RuleIntent::DeleteCollection(id, false));
            }
            if ui
                .add(action_button(
                    ico(ph::TRASH, &format!("Delete collection and {rules}")),
                    danger(),
                ))
                .clicked()
            {
                out.push(RuleIntent::DeleteCollection(id, true));
            }
            if icon_button(ui, ph::X, "Cancel", true).clicked() {
                vm.confirm_delete = None;
            }
        });
    });
}

fn form_ui(
    ui: &mut egui::Ui,
    f: &mut RuleForm,
    collections: &[(u64, String)],
    out: &mut Vec<RuleIntent>,
) {
    let w = (ui.available_width() - 100.0).clamp(120.0, 700.0);
    let kind_picker = |ui: &mut egui::Ui, kind: &mut RuleKind| {
        let current = KINDS.iter().find(|(k, _)| k == kind).map_or("", |(_, l)| l);
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
            if !collections.is_empty() {
                field(ui, "Collection", |ui| {
                    let current = collections
                        .iter()
                        .find(|(id, _)| Some(*id) == f.collection)
                        .map_or("No collection", |(_, n)| n.as_str());
                    egui::ComboBox::from_id_salt("rule-collection")
                        .selected_text(current)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.collection, None, "No collection");
                            for (id, name) in collections {
                                ui.selectable_value(&mut f.collection, Some(*id), name);
                            }
                        });
                });
            }
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
                if icon_only(ui, ph::X, &format!("Remove {h}")).clicked() {
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
    section(
        ui,
        "Proxy only these hosts",
        "Network level: with the system proxy on, only these hosts go through proxyproxy (via a PAC file); everything else connects directly. Hosts only: URL paths are dropped. Open connections keep their route.",
        ListKind::ProxyOnly,
        &sv.proxy_only_hosts,
        4,
    );
    let mut proxy_only = sv.proxy_only;
    if checkbox(
        ui,
        &mut proxy_only,
        "Route only the hosts above (needs macOS)",
    )
    .changed()
    {
        out.push(RuleIntent::SetProxyOnly(proxy_only));
    }
    if icon_button(ui, ph::TARGET, "Add hosts from Scope", true).clicked() {
        out.push(RuleIntent::SeedProxyOnlyFromScope);
    }
    if sv.scope_has_paths {
        ui.label(muted("Scope path patterns are reduced to their host."));
    }
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
