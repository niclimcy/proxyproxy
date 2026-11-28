use crate::domain::pac;
use crate::domain::rule::MapStorage;
use crate::services::mocks;
use crate::viewmodels::intent::{ListKind, MapSwitchChoice, RuleIntent};
use crate::viewmodels::rules_vm::{
    HIGHLIGHT, KINDS, RuleForm, RuleGroup, RuleKind, RuleRow, RulesTab, RulesVm, Tone,
};
use crate::views::theme::{
    FS_CAPTION, HIT, Tint, accent, action_button, border, card, card_panel, checkbox,
    code_layouter, danger, fg, fg_muted, ico, icon_button, icon_only, info, menu_item, menu_width,
    method_color, muted, overflow_menu, overline, pill, resize_grip, resize_line, row_within,
    segmented, success, switch, tint, violet, warning,
};
use eframe::egui::{self, RichText, ScrollArea, TextEdit};
use egui_phosphor::regular as ph;

const FORM_COL_GAP: f32 = 10.0;
const SCOPE_INPUT_ROW: f32 = 340.0;
const FORM_LABELS: [&str; 21] = [
    "Apply to",
    "Body",
    "Code",
    "Collection",
    "File name",
    "File path",
    "Find",
    "Header",
    "Latency (ms)",
    "Max KB/s",
    "Method",
    "Name",
    "Pause on",
    "Replace",
    "Response body",
    "Response from",
    "Status",
    "Target",
    "Type",
    "URL pattern",
    "Value",
];

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
    ui.horizontal(|ui| {
        ui.label(overline("RULES"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_only(ui, ph::FOLDER_SIMPLE_PLUS, "New collection").clicked() {
                out.push(RuleIntent::AddCollection);
            }
        });
    });
    let list = vm.rule_list();
    let grouped = !list.collections.is_empty();
    let scroll_to = vm.scroll_to.take();
    if vm
        .highlight
        .as_ref()
        .is_some_and(|(_, at)| at.elapsed() < HIGHLIGHT)
    {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    ScrollArea::vertical()
        .max_height((ui.ctx().content_rect().height() * 0.45).max(200.0))
        .id_salt("rule-list")
        .show(ui, |ui| {
            if !grouped && list.groups.iter().all(|g| g.rows.is_empty()) {
                ui.add(egui::Label::new(muted(EMPTY)).wrap());
            }
            for g in &list.groups {
                let (mut guide_x, mut left) = (None, 0.0);
                if let Some(id) = g.id {
                    let xs = folder_row(ui, vm, g, id, out);
                    if vm.collapsed.contains(&g.id) {
                        continue;
                    }
                    left = xs.0 - ui.max_rect().left() + GUIDE_GAP;
                    guide_x = (!g.rows.is_empty()).then_some(xs.0);
                }
                let body = ui.horizontal_top(|ui| {
                    ui.add_space(left);
                    ui.vertical(|ui| {
                        if g.id.is_some() && g.rows.is_empty() {
                            ui.add(egui::Label::new(muted(EMPTY)).wrap());
                        }
                        let last = g.rows.len().saturating_sub(1);
                        for (pos, r) in g.rows.iter().enumerate() {
                            let cx = RowCtx {
                                order: Order {
                                    can_up: pos > 0,
                                    can_down: pos < last,
                                },
                                collection_off: !g.enabled,
                                highlight: vm.highlighted(r.id),
                                scroll: scroll_to == Some(r.id),
                                editing: vm.editing == Some(r.id),
                                collections: &list.collections,
                            };
                            rule_card(ui, r, &cx, out);
                        }
                    });
                });
                if let Some(x) = guide_x {
                    let r = body.response.rect;
                    ui.painter().line_segment(
                        [egui::pos2(x, r.top()), egui::pos2(x, r.bottom())],
                        egui::Stroke::new(1.0, border()),
                    );
                }
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
        if vm.form.kind == RuleKind::MapLocal && vm.form.ml.pending() {
            ui.label(RichText::new("Unsaved").color(warning()));
        }
        if !vm.form.pattern.trim().is_empty() {
            ui.label(muted(format!(
                "matches {live} captured request{}",
                if live == 1 { "" } else { "s" }
            )));
        }
    });
}

/// Space between the collection's guide line and the rules beside it.
const GUIDE_GAP: f32 = 8.0;

const EMPTY: &str = "No rules yet. Right-click a request → Mock response.";

struct Order {
    can_up: bool,
    can_down: bool,
}

struct RowCtx<'a> {
    order: Order,
    collection_off: bool,
    highlight: bool,
    scroll: bool,
    editing: bool,
    collections: &'a [(u64, String)],
}

fn rule_card(ui: &mut egui::Ui, r: &RuleRow, cx: &RowCtx, out: &mut Vec<RuleIntent>) {
    // the controls are always laid out so nothing shifts; they only show while the row is in use
    let state = ui.make_persistent_id(("rule-row", r.id));
    let shown_before: bool = ui.data(|d| d.get_temp(state)).unwrap_or(false);
    let mut focused = false;
    let mut frame = card().inner_margin(8);
    if cx.highlight {
        frame = frame.fill(tint(accent(), Tint::Subtle));
    }
    let row = frame.show(ui, |ui| {
        let w = ui.available_width();
        // the controls sit first at the right edge, so a long title can only take the space left
        row_within(ui, w, true, |ui| {
            ui.scope(|ui| {
                if !shown_before {
                    ui.set_opacity(0.0);
                }
                let delete = icon_only(ui, ph::X, "Delete");
                if delete.clicked() {
                    out.push(RuleIntent::Delete(r.id));
                }
                let edit = icon_only(ui, ph::PENCIL, "Edit");
                if edit.clicked() {
                    out.push(RuleIntent::Edit(r.id));
                }
                let menu = overflow_menu(ui, "Rule actions", |ui| {
                    rule_menu(ui, r.id, &cx.order, cx.collections, out);
                });
                focused = delete.has_focus() || edit.has_focus() || menu.has_focus();
            });
            if r.hits > 0 {
                ui.label(muted(format!("{} hits", r.hits)));
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let mut enabled = r.enabled;
                let on = checkbox(ui, &mut enabled, "");
                let who = if r.name.is_empty() {
                    &r.pattern
                } else {
                    &r.name
                };
                let spoken = if cx.collection_off {
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
                // a rule in an off collection reads as off through muted tokens, so its text keeps its contrast
                let ink = |c| if cx.collection_off { fg_muted() } else { c };
                let tone = match r.tone {
                    Tone::Warning => warning(),
                    Tone::Danger => danger(),
                    Tone::Violet => violet(),
                    Tone::Info => info(),
                    Tone::Success => success(),
                };
                pill(ui, r.label, ink(tone));
                if !r.method.is_empty() {
                    let m = r.method.to_uppercase();
                    pill(ui, &m, ink(method_color(&m)));
                }
                let title = if r.name.is_empty() {
                    RichText::new(pac::host_of(&r.pattern).unwrap_or_else(|| r.pattern.clone()))
                        .monospace()
                } else {
                    RichText::new(&r.name)
                };
                let pattern = ui.add(
                    egui::Label::new(title.color(ink(fg())))
                        .truncate()
                        .sense(egui::Sense::click()),
                );
                if pattern
                    .on_hover_text(format!("{} — click to edit", r.pattern))
                    .clicked()
                {
                    out.push(RuleIntent::Edit(r.id));
                }
            });
        });
        if let Some(d) = &r.description {
            ui.label(muted(d));
        }
    });
    let rect = row.response.rect;
    let shown = ui.rect_contains_pointer(rect)
        || focused
        || cx.editing
        || (shown_before && egui::Popup::is_any_open(ui.ctx()));
    if shown != shown_before {
        ui.data_mut(|d| d.insert_temp(state, shown));
        ui.ctx().request_repaint();
    }
    if cx.scroll {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
    }
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

/// One line, like a rule row: caret, switch, name, then (collapsed only) the rule count and the menu.
/// Returns the caret's centre x and the name's left x, which the rules below line up with.
fn folder_row(
    ui: &mut egui::Ui,
    vm: &mut RulesVm,
    g: &RuleGroup,
    id: u64,
    out: &mut Vec<RuleIntent>,
) -> (f32, f32) {
    let collapsed = vm.collapsed.contains(&g.id);
    let mut start_rename = false;
    let (mut caret_x, mut name_x) = (0.0, 0.0);
    let n = g.rows.len();
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), HIT),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            overflow_menu(ui, "Collection actions", |ui| {
                menu_width(
                    ui,
                    &[
                        "Use only this collection",
                        "New rule here",
                        "Rename",
                        "Delete…",
                    ],
                );
                if menu_item(ui, "Use only this collection", None).clicked() {
                    out.push(RuleIntent::SoloCollection(id));
                }
                if menu_item(ui, "New rule here", None).clicked() {
                    out.push(RuleIntent::NewRuleIn(g.id));
                }
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
            if collapsed {
                ui.label(muted(format!("{n} rule{}", if n == 1 { "" } else { "s" })));
            }
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), HIT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let (icon, tip) = if collapsed {
                        (ph::CARET_RIGHT, "Expand")
                    } else {
                        (ph::CARET_DOWN, "Collapse")
                    };
                    let caret = icon_only(ui, icon, tip);
                    caret_x = caret.rect.center().x;
                    if caret.clicked() && !vm.collapsed.remove(&g.id) {
                        vm.collapsed.insert(g.id);
                    }
                    let mut on = g.enabled;
                    let label = format!("Collection {}, {}", g.name, if on { "on" } else { "off" });
                    if switch(ui, &mut on, &label).changed() {
                        out.push(RuleIntent::SetCollectionEnabled(id, on));
                    }
                    name_x = ui.cursor().left();
                    if vm.renaming == Some(id) {
                        rename_field(ui, vm, id, out);
                    } else {
                        let text = RichText::new(&g.name).strong();
                        let name = ui.add(
                            egui::Label::new(if g.enabled {
                                text.color(fg())
                            } else {
                                text.color(fg_muted())
                            })
                            .truncate()
                            .sense(egui::Sense::click()),
                        );
                        if name.double_clicked() {
                            start_rename = true;
                        }
                    }
                },
            );
        },
    );
    if start_rename {
        vm.renaming = Some(id);
        vm.rename_input.clone_from(&g.name);
        vm.rename_focus = true;
    }
    if vm.confirm_delete == Some(id) {
        delete_confirm(ui, vm, id, g, out);
    }
    (caret_x, name_x)
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

fn map_local_fields(ui: &mut egui::Ui, f: &mut RuleForm, w: f32, out: &mut Vec<RuleIntent>) {
    let ml = &mut f.ml;
    let stored = ml.storage == MapStorage::AppData;
    field(ui, "Response from", |ui| {
        ui.vertical(|ui| {
            let mut choice = ml.storage;
            segmented(
                ui,
                &mut choice,
                &[
                    (MapStorage::Disk, "File on disk"),
                    (MapStorage::AppData, "In proxyproxy"),
                ],
            );
            ui.label(muted(if stored {
                "A copy kept with this rule in proxyproxy's data folder. Switch to File on disk to export it."
            } else {
                "Serves and edits your file directly"
            }));
            if let Some(live) = &ml.live {
                ui.label(muted(format!("Serving now: {}", live.label)));
                if live.storage != ml.storage {
                    ui.label(muted(format!(
                        "Will switch to {} when you save the rule.",
                        if stored { "the stored copy" } else { "the file on disk" }
                    )));
                }
            } else if ml.pending() {
                ui.label(muted("Applies when you add the rule."));
            }
            if choice != ml.storage {
                // nothing to switch back to yet: ask where the stored content should go
                let asked = choice == MapStorage::Disk && ml.file.trim().is_empty();
                let target = asked
                    .then(|| {
                        rfd::FileDialog::new()
                            .set_file_name(mocks::clean_name(&ml.name))
                            .save_file()
                    })
                    .flatten();
                if asked && target.is_none() {
                    out.push(RuleIntent::ResolveMapSwitch(MapSwitchChoice::Cancel));
                } else {
                    out.push(RuleIntent::SetMapStorage(choice, target));
                }
            }
            if let Some(p) = &ml.prompt {
                ui.label(
                    RichText::new(ico(
                        ph::WARNING,
                        if p.to == MapStorage::Disk {
                            "The file already holds different content"
                        } else {
                            "A different stored copy already exists"
                        },
                    ))
                    .color(warning()),
                );
                ui.label(muted(&p.summary));
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add(action_button(ico(ph::WARNING, &p.replace), danger()))
                        .clicked()
                    {
                        out.push(RuleIntent::ResolveMapSwitch(MapSwitchChoice::Replace));
                    }
                    if icon_button(ui, ph::CHECK, &p.keep, true).clicked() {
                        out.push(RuleIntent::ResolveMapSwitch(MapSwitchChoice::Adopt));
                    }
                    if icon_button(ui, ph::X, "Cancel switch", true).clicked() {
                        out.push(RuleIntent::ResolveMapSwitch(MapSwitchChoice::Cancel));
                    }
                });
            }
        });
    });
    if stored {
        field(ui, "File name", |ui| {
            ui.vertical(|ui| {
                ui.add(
                    TextEdit::singleline(&mut ml.name)
                        .desired_width(w)
                        .hint_text("response.json"),
                );
                ui.label(muted(format!("Served as {}", ml.served_as())));
                if let Some(note) = ml.name_note() {
                    ui.label(RichText::new(note).color(warning()));
                }
                if let Some(path) = ml.stored_path() {
                    row_within(ui, w, true, |ui| {
                        if icon_only(ui, ph::FOLDER_OPEN, "Show in the file manager").clicked() {
                            out.push(RuleIntent::RevealMapFile);
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(muted(&path)).truncate())
                                .on_hover_text(&path);
                        });
                    });
                }
            });
        });
    } else {
        field(ui, "File path", |ui| {
            row_within(ui, w, true, |ui| {
                if icon_button(ui, ph::FOLDER_OPEN, "Browse…", true).clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_file()
                {
                    ml.file = p.display().to_string();
                    out.push(RuleIntent::LoadMapLocalFile);
                }
                let path = ui.add(
                    TextEdit::singleline(&mut ml.file)
                        .desired_width(ui.available_width())
                        .hint_text("/path/to/response.json"),
                );
                if path.lost_focus() {
                    out.push(RuleIntent::LoadMapLocalFile);
                }
            });
        });
    }
    field(ui, "Status", |ui| {
        ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
    });
    field(ui, "Response body", |ui| {
        ui.vertical(|ui| {
            if !stored {
                ui.horizontal(|ui| {
                    if icon_button(ui, ph::FOLDER_OPEN, "Load file", true)
                        .on_hover_text("Read the selected file from disk")
                        .clicked()
                    {
                        out.push(RuleIntent::LoadMapLocalFile);
                    }
                    if icon_button(ui, ph::FLOPPY_DISK, "Save to disk", ml.editable())
                        .on_hover_text(
                            "Writes your file immediately, without waiting for Save rule",
                        )
                        .clicked()
                    {
                        out.push(RuleIntent::SaveMapLocalFile);
                    }
                });
            }
            if let Some(error) = &ml.error {
                ui.label(RichText::new(ico(ph::WARNING, error)).color(danger()));
            } else if let Some(status) = &ml.status {
                ui.label(RichText::new(status).color(success()));
            }
            if let Some(limit) = &ml.limit {
                ui.label(muted(limit));
            }
            if ml.editable() {
                let ext = ml.ext();
                ui.label(muted(if ext.is_empty() {
                    "Text file".to_string()
                } else {
                    format!("{} file", ext.to_uppercase())
                }));
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                    code_layouter(ui, text, wrap_width, &ext)
                };
                let editor = TextEdit::multiline(&mut ml.content)
                    .code_editor()
                    .desired_rows(10)
                    .desired_width(w)
                    .layouter(&mut layouter);
                if ui.add(editor).changed() {
                    ml.dirty = true;
                    ml.status = None;
                    ml.error = None;
                }
                if ml.dirty {
                    ui.label(muted(if stored {
                        "Edited — saved with the rule"
                    } else {
                        "Unsaved changes — Save to disk will update the file."
                    }));
                }
            } else if ml.limit.is_none() && !stored && !ml.file.trim().is_empty() {
                ui.label(muted("Load file to edit and save its contents."));
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
    // the label column is as wide as its widest label, so measure instead of assuming
    let label_w = FORM_LABELS
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(
                    (*l).to_string(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    fg(),
                )
                .size()
                .x
        })
        .fold(0.0, f32::max);
    let w = (ui.available_width() - label_w - FORM_COL_GAP).clamp(120.0, 700.0);
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
        .spacing([FORM_COL_GAP, 8.0])
        .show(ui, |ui| {
            field(ui, "Type", |ui| kind_picker(ui, &mut f.kind));
            field(ui, "", |ui| {
                // a Grid cell has no width of its own, so an unwrapped hint set the column's floor
                ui.allocate_ui(egui::vec2(w, 0.0), |ui| {
                    ui.set_width(w);
                    ui.add(egui::Label::new(muted(hint)).wrap());
                });
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
                )
                .on_hover_text("Plain text matches anywhere in the URL; * and ? are wildcards");
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
                RuleKind::MapLocal => map_local_fields(ui, f, w, out),
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
                        row_within(ui, w, true, |ui| {
                            checkbox(ui, &mut f.hremove, "Remove");
                            ui.add_enabled(
                                !f.hremove,
                                TextEdit::singleline(&mut f.hvalue)
                                    .desired_width(ui.available_width()),
                            );
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
                        row_within(ui, w, false, |ui| {
                            ui.add(TextEdit::singleline(&mut f.status).desired_width(60.0));
                            ui.label("Content-Type");
                            ui.add(
                                TextEdit::singleline(&mut f.ctype)
                                    .desired_width(ui.available_width()),
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
        let w = ui.available_width();
        for (i, h) in list.iter().enumerate() {
            row_within(ui, w, true, |ui| {
                if icon_only(ui, ph::X, &format!("Remove {h}")).clicked() {
                    out.push(RuleIntent::RemoveHost(kind, i));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(h).monospace()).truncate())
                        .on_hover_text(h);
                });
            });
        }
        row_within(ui, w.min(SCOPE_INPUT_ROW), true, |ui| {
            let add = icon_button(ui, ph::PLUS, "Add", true);
            let input = &mut vm.list_inputs[slot];
            let r = ui.add(
                TextEdit::singleline(input)
                    .hint_text("domain, host/path/* or https://*.host/path")
                    .desired_width(ui.available_width()),
            );
            if add.clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::domain::rule::{Action, Rule, RuleCollection};
    use crate::services::flow_store::FlowStore;
    use crate::viewmodels::rules_vm::RuleKind;
    use crate::views::layout_test::*;
    use std::sync::{Arc, Mutex};

    const KIND_ALL: [RuleKind; 9] = [
        RuleKind::Breakpoint,
        RuleKind::Mock,
        RuleKind::MapLocal,
        RuleKind::Redirect,
        RuleKind::Block,
        RuleKind::Throttle,
        RuleKind::Header,
        RuleKind::Body,
        RuleKind::Script,
    ];

    /// Rules of every kind with long names and patterns, in and out of long-named collections.
    fn long_config() -> Config {
        let actions = [
            Action::Breakpoint {
                request: true,
                response: true,
            },
            Action::Mock {
                status: 200,
                content_type: "application/vnd.company.resource.v2+json".into(),
                body: WORDS.into(),
            },
            Action::MapLocal {
                path: FILE.into(),
                status: 200,
                storage: crate::domain::rule::MapStorage::default(),
                managed: String::new(),
                embedded: None,
            },
            Action::MapRemote { target: url() },
            Action::Block { status: 503 },
            Action::Delay { ms: 1000, kbps: 64 },
            Action::SetHeader {
                response: true,
                name: WORDS.into(),
                value: WORDS.into(),
                remove: false,
            },
            Action::ReplaceBody {
                response: true,
                find: WORDS.into(),
                replace: WORDS.into(),
                regex: false,
            },
            Action::Script {
                code: String::new(),
            },
        ];
        let list = |n: &str| (0..3).map(|i| format!("{i}-{n}")).collect::<Vec<_>>();
        Config {
            rules: actions
                .into_iter()
                .enumerate()
                .map(|(i, action)| Rule {
                    id: i as u64 + 1,
                    enabled: true,
                    name: if i % 2 == 0 {
                        WORDS.into()
                    } else {
                        String::new()
                    },
                    pattern: format!("{HOST}{PATH}"),
                    method: "DELETE".into(),
                    action,
                    collection: [None, Some(1), Some(2)][i % 3],
                    hits: 12_345,
                })
                .collect(),
            collections: ["First", "Second"]
                .map(|n| RuleCollection {
                    id: if n == "First" { 1 } else { 2 },
                    name: format!("{n} {WORDS}"),
                    enabled: true,
                })
                .to_vec(),
            scope_only: true,
            scope: list(&format!("{HOST}/some/long/path/*")),
            exclude: list(HOST),
            muted: list(WORDS),
            bypass_tls: list(HOST),
            proxy_only: true,
            proxy_only_hosts: list(HOST),
            ..Default::default()
        }
    }

    /// A form with every field full of long text.
    fn fill(vm: &mut RulesVm, kind: RuleKind) {
        let f = &mut vm.form;
        f.kind = kind;
        f.name = WORDS.into();
        f.pattern = format!("{HOST}{PATH}");
        f.method = "DELETE".into();
        f.status = "503".into();
        f.target = url();
        f.hname = WORDS.into();
        f.hvalue = WORDS.into();
        f.find = WORDS.into();
        f.replace = WORDS.into();
        f.ctype = "application/vnd.company.resource.v2+json; charset=utf-8".into();
        f.body = WORDS.repeat(3);
        f.ml.file = FILE.into();
        f.ml.name = WORDS.into();
        f.ml.status = Some(WORDS.into());
    }

    /// Width the panel ends up at; `drag` is the width the user dragged it to first.
    fn panel_width(tab: RulesTab, kind: RuleKind, editing: bool, drag: Option<f32>) -> f32 {
        panel_width_cfg(long_config(), tab, kind, editing, drag)
    }

    fn panel_width_cfg(
        cfg: Config,
        tab: RulesTab,
        kind: RuleKind,
        editing: bool,
        drag: Option<f32>,
    ) -> f32 {
        let ctx = ctx();
        let mut vm = RulesVm::new(Arc::new(Mutex::new(cfg)), FlowStore::default());
        vm.show = true;
        vm.tab = tab;
        fill(&mut vm, kind);
        vm.console = (0..4).map(|_| format!("[error] {WORDS}{WORDS}")).collect();
        if editing {
            vm.editing = Some(1);
        }
        let mut width = 0.0;
        frames(&ctx, egui::vec2(1600.0, 900.0), 30, |ui, i| {
            if i == 0
                && let Some(d) = drag
            {
                drag_panel(ui, "rules", egui::vec2(d, 900.0));
            }
            show(ui, &mut vm);
            width = taken(ui);
        });
        width
    }

    #[test]
    fn collection_rename_and_delete_fit_the_narrowest_panel() {
        let ctx = ctx();
        let mut vm = RulesVm::new(Arc::new(Mutex::new(long_config())), FlowStore::default());
        vm.show = true;
        vm.renaming = Some(1);
        vm.rename_input = WORDS.into();
        vm.confirm_delete = Some(2);
        let mut width = 0.0;
        frames(&ctx, egui::vec2(1600.0, 900.0), 30, |ui, i| {
            if i == 0 {
                drag_panel(ui, "rules", egui::vec2(360.0, 900.0));
            }
            show(ui, &mut vm);
            width = taken(ui);
        });
        assert!(
            width <= 361.0,
            "renaming or confirming a delete held the panel at {width}"
        );
    }

    #[test]
    fn rule_forms_keep_the_panel_at_its_default_width() {
        for editing in [false, true] {
            for kind in KIND_ALL {
                let w = panel_width(RulesTab::Rules, kind, editing, None);
                assert!(
                    w <= 441.0,
                    "{kind:?} (editing: {editing}) widened the panel to {w}"
                );
            }
        }
        for tab in [RulesTab::Scope, RulesTab::Console] {
            let w = panel_width(tab, RuleKind::Block, false, None);
            assert!(w <= 441.0, "the {tab:?} tab widened the panel to {w}");
        }
    }

    #[test]
    fn every_tab_fits_the_narrowest_panel() {
        for kind in KIND_ALL {
            let w = panel_width(RulesTab::Rules, kind, false, Some(360.0));
            assert!(w <= 361.0, "{kind:?} would not shrink below {w}");
        }
        for tab in [RulesTab::Scope, RulesTab::Console] {
            let w = panel_width(tab, RuleKind::Block, false, Some(360.0));
            assert!(w <= 361.0, "the {tab:?} tab would not shrink below {w}");
        }
    }
}
