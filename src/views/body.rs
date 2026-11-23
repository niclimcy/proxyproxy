use crate::domain::flow::Message;
use crate::services::decode::{self, Ctx};
use crate::services::decoders::{self, ModeId, Rendered};
use crate::views::theme::*;
use eframe::egui::{self, RichText, ScrollArea, TextEdit};
use serde_json::Value;

fn forced_key(id: &str) -> egui::Id {
    egui::Id::NULL.with(("schema-type", id))
}

fn json_scalar(v: &Value) -> (String, egui::Color32) {
    match v {
        Value::String(s) => (
            format!("\"{}\"", s.chars().take(300).collect::<String>()),
            success(),
        ),
        Value::Number(n) => (n.to_string(), warning()),
        Value::Bool(b) => (b.to_string(), violet()),
        _ => ("null".into(), fg_muted()),
    }
}

fn tree_node(
    ui: &mut egui::Ui,
    key: &str,
    v: &Value,
    path: &str,
    depth: usize,
    open_all: Option<bool>,
) {
    let leaf = |ui: &mut egui::Ui, v: &Value| {
        let (t, c) = json_scalar(v);
        let r = ui.add(
            egui::Label::new(RichText::new(t).monospace().color(c)).sense(egui::Sense::click()),
        );
        if r.on_hover_text(format!(
            "{path}\nclick to copy path · double-click to copy value"
        ))
        .clicked()
        {
            ui.ctx().copy_text(path.to_string());
        }
    };
    match v {
        Value::Object(_) | Value::Array(_) => {
            let n = match v {
                Value::Object(o) => o.len(),
                Value::Array(a) => a.len(),
                _ => 0,
            };
            let summary = if matches!(v, Value::Array(_)) {
                format!("[{n}]")
            } else {
                format!("{{{n}}}")
            };
            let mut h = egui::CollapsingHeader::new(
                RichText::new(format!("{key}  {summary}"))
                    .monospace()
                    .color(accent()),
            )
            .id_salt(path)
            .default_open(depth < 2);
            if let Some(o) = open_all {
                h = h.open(Some(o));
            }
            h.show(ui, |ui| match v {
                Value::Object(o) => {
                    for (k, c) in o.iter().take(500) {
                        tree_node(ui, k, c, &format!("{path}.{k}"), depth + 1, open_all);
                    }
                }
                Value::Array(a) => {
                    for (i, c) in a.iter().enumerate().take(500) {
                        tree_node(
                            ui,
                            &i.to_string(),
                            c,
                            &format!("{path}[{i}]"),
                            depth + 1,
                            open_all,
                        );
                    }
                }
                _ => {}
            });
        }
        _ => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{key}:"))
                        .monospace()
                        .color(fg_muted()),
                );
                leaf(ui, v);
            });
        }
    }
}

fn hex_view(ui: &mut egui::Ui, id: &str, data: &[u8]) {
    let rows = data.len().div_ceil(16).min(20_000);
    ScrollArea::vertical()
        .id_salt(format!("{id}-hex"))
        .auto_shrink([false, false])
        .show_rows(ui, 16.0, rows, |ui, range| {
            for r in range {
                let chunk = &data[r * 16..((r + 1) * 16).min(data.len())];
                let hex: String = (0..16)
                    .map(|i| {
                        chunk
                            .get(i)
                            .map(|b| format!("{b:02x} "))
                            .unwrap_or_else(|| "   ".into())
                    })
                    .collect();
                let asc: String = chunk
                    .iter()
                    .map(|&b| {
                        if (32..127).contains(&b) {
                            b as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                ui.label(
                    RichText::new(format!("{:08x}  {hex} {asc}", r * 16))
                        .monospace()
                        .size(FS_CAPTION),
                );
            }
        });
}

pub fn body_view(ui: &mut egui::Ui, id: &str, m: &Message, mode: &mut ModeId) {
    body_view_ctx(ui, id, m, mode, &Ctx::default());
}

fn scroll_text(ui: &mut egui::Ui, salt: String, text: &str) {
    ScrollArea::both()
        .id_salt(salt)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut t = text.to_string();
            ui.add(
                TextEdit::multiline(&mut t)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE),
            );
        });
}

pub fn body_view_ctx(ui: &mut egui::Ui, id: &str, m: &Message, mode: &mut ModeId, c: &Ctx) {
    if m.body.is_empty() {
        ui.label(muted("No body"));
        return;
    }
    let forced: Option<String> = ui.data(|d| d.get_temp(forced_key(id)));
    let avail = decoders::available(m, c, forced.as_deref());
    if !avail.iter().any(|d| d.id() == *mode) {
        *mode = decoders::default_for(&avail);
    }
    let items: Vec<(ModeId, &str)> = avail.iter().map(|d| (d.id(), d.label())).collect();
    ui.horizontal(|ui| {
        segmented(ui, mode, &items);
        ui.label(muted(format!(
            "{} · {}",
            fmt_size(m.body.len()),
            m.content_type()
        )));
        if ui
            .small_button(ph_copy())
            .on_hover_text("Copy body")
            .clicked()
        {
            ui.ctx().copy_text(decode::pretty_text(m));
        }
    });
    ui.add_space(4.0);
    let Some(d) = decoders::find(mode) else {
        return;
    };
    let rendered = d.render(m, c, forced.as_deref());
    if d.type_picker(m) {
        ui.horizontal(|ui| {
            if let Rendered::Text {
                caption: Some(cap), ..
            } = &rendered
            {
                ui.label(muted(cap.clone()));
            } else {
                ui.label(muted("Schema type"));
            }
            type_picker(ui, id, forced.as_deref());
        });
    }
    match rendered {
        Rendered::Text { text, .. } => scroll_text(ui, format!("{id}-{}", d.id()), &text),
        Rendered::Hex => hex_view(ui, id, &m.body),
        Rendered::Tree(v) => {
            let mut expand = None;
            ui.horizontal(|ui| {
                if ui.small_button("Expand all").clicked() {
                    expand = Some(true);
                }
                if ui.small_button("Collapse all").clicked() {
                    expand = Some(false);
                }
                ui.label(muted("click a value to copy its JSONPath"));
            });
            ScrollArea::both()
                .id_salt(format!("{id}-t"))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    tree_node(ui, "$", &v, "$", 0, expand);
                });
        }
        Rendered::Form(rows) => {
            ScrollArea::vertical()
                .id_salt(format!("{id}-f"))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    kv_table(ui, rows.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                });
        }
        Rendered::Sections(sections) => {
            ScrollArea::both()
                .id_salt(format!("{id}-d"))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (title, text) in sections {
                        ui.label(RichText::new(title).strong().color(violet()));
                        let mut s = text;
                        ui.add(
                            TextEdit::multiline(&mut s)
                                .code_editor()
                                .desired_width(f32::INFINITY),
                        );
                    }
                });
        }
    }
}

fn type_picker(ui: &mut egui::Ui, id: &str, cur: Option<&str>) {
    let mut sel = cur.map(String::from);
    egui::ComboBox::from_id_salt(format!("{id}-ty"))
        .selected_text(cur.unwrap_or("auto"))
        .width(260.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut sel, None, "auto");
            for t in crate::services::schemas::message_types()
                .into_iter()
                .take(2000)
            {
                ui.selectable_value(&mut sel, Some(t.clone()), t);
            }
        });
    if sel.as_deref() != cur {
        ui.data_mut(|d| match sel {
            Some(s) => {
                d.insert_temp(forced_key(id), s);
            }
            None => d.remove::<String>(forced_key(id)),
        });
    }
}

/// Key/value rows: fixed key column, value fills the rest and wraps. Click a value to copy.
pub fn kv_table<'a>(ui: &mut egui::Ui, rows: impl IntoIterator<Item = (&'a str, &'a str)>) {
    const KEY_W: f32 = 150.0;
    ui.horizontal(|ui| {
        ui.add_sized([KEY_W, 18.0], egui::Label::new(overline("KEY")));
        ui.label(overline("VALUE"));
    });
    for (i, (k, v)) in rows.into_iter().enumerate() {
        let fill = if i % 2 == 1 {
            pal().faint
        } else {
            egui::Color32::TRANSPARENT
        };
        egui::Frame::new()
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(4, 3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.allocate_ui(egui::vec2(KEY_W - 8.0, 0.0), |ui| {
                        ui.set_width(KEY_W - 8.0);
                        ui.add(egui::Label::new(RichText::new(k).strong().color(accent())).wrap());
                    });
                    if ui
                        .add(
                            egui::Label::new(RichText::new(v).monospace())
                                .sense(egui::Sense::click())
                                .wrap(),
                        )
                        .on_hover_text("click to copy")
                        .clicked()
                    {
                        ui.ctx().copy_text(v.to_string());
                    }
                });
            });
    }
}

pub fn headers_view(ui: &mut egui::Ui, id: &str, m: &Message) {
    ScrollArea::vertical()
        .id_salt(format!("{id}-hd"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            kv_table(ui, m.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        });
}

pub fn diff_view(ui: &mut egui::Ui, id: &str, lines: &[(char, String)], only_changes: bool) {
    ScrollArea::both()
        .id_salt(format!("{id}-diff"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, (tag, l)) in lines.iter().enumerate() {
                let near = |d: usize| {
                    lines[i.saturating_sub(d)..(i + d + 1).min(lines.len())]
                        .iter()
                        .any(|(t, _)| *t != ' ')
                };
                if only_changes && *tag == ' ' && !near(2) {
                    continue;
                }
                let (c, bg) = match tag {
                    '+' => (success(), tint(success(), Tint::Subtle)),
                    '-' => (danger(), tint(danger(), Tint::Subtle)),
                    _ => (fg_muted(), egui::Color32::TRANSPARENT),
                };
                egui::Frame::new().fill(bg).show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{tag} {l}"))
                                .monospace()
                                .size(FS_CAPTION)
                                .color(c),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
            }
        });
}

fn ph_copy() -> String {
    egui_phosphor::regular::COPY.to_string()
}
