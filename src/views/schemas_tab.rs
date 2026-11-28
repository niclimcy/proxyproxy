use crate::viewmodels::intent::SchemasIntent;
use crate::viewmodels::schemas_vm::SchemasVm;
use crate::views::theme::{danger, fg_muted, icon_button, icon_only, muted};
use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

pub fn show(ui: &mut egui::Ui, vm: &SchemasVm) -> Vec<SchemasIntent> {
    let mut out = vec![];
    ui.label(muted("Protobuf descriptor sets (protoc --descriptor_set_out=x.desc --include_imports ...). Bodies that decode cleanly get a Schema view; gRPC resolves by method path."));
    ui.add_space(6.0);
    if icon_button(ui, ph::PLUS, "Add .desc file", true).clicked()
        && let Some(files) = rfd::FileDialog::new()
            .add_filter("descriptor set", &["desc", "pb", "bin", "protoset"])
            .pick_files()
    {
        out.push(SchemasIntent::AddFiles(
            files.iter().map(|f| f.display().to_string()).collect(),
        ));
    }
    ui.add_space(6.0);
    let v = vm.view();
    if v.paths.is_empty() {
        ui.label(RichText::new("No schemas loaded").color(fg_muted()));
    }
    for (i, p) in v.paths.iter().enumerate() {
        ui.horizontal(|ui| {
            if icon_only(ui, ph::X, "Remove descriptor").clicked() {
                out.push(SchemasIntent::Remove(i));
            }
            ui.add(egui::Label::new(RichText::new(p).monospace()).truncate())
                .on_hover_text(p);
        });
        if let Some((_, e)) = v.errors.iter().find(|(ep, _)| ep == p) {
            ui.label(RichText::new(e).color(danger()));
        }
    }
    if !v.paths.is_empty() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(muted(format!(
                "{} messages · {} services",
                v.stats.0, v.stats.1
            )));
            if icon_button(ui, ph::ARROWS_CLOCKWISE, "Reload", true).clicked() {
                out.push(SchemasIntent::Reload);
            }
        });
    }
    out
}
