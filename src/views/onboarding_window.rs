use crate::domain::config::OnboardingProfile;
use crate::viewmodels::intent::DeviceAction;
use crate::viewmodels::{
    intent::{OnboardingIntent, OnboardingTarget},
    onboarding_vm::{OnboardingStage, OnboardingVm},
};
use crate::views::theme::*;
use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

fn header(ui: &mut egui::Ui, ctx: &egui::Context, title: &str, subtitle: &str, icon_size: f32) {
    let icon = crate::views::icon::data();
    let texture = ctx.load_texture(
        "proxyproxy-onboarding-icon",
        egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        ),
        egui::TextureOptions::LINEAR,
    );
    ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(icon_size, icon_size)));
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(FS_TITLE).strong());
    if !subtitle.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(subtitle).size(FS_BODY).color(fg_muted()));
    }
}

fn choice_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    width: f32,
) -> egui::Response {
    let height = 132.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let fill = if response.hovered() {
        tint(accent(), Tint::Subtle)
    } else {
        surface()
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(RADIUS), fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(RADIUS),
        egui::Stroke::new(1.0, border()),
        egui::StrokeKind::Inside,
    );

    let left = rect.left() + 16.0;
    let mut top = rect.top() + 16.0;
    if !icon.is_empty() {
        let icon_galley = painter.layout_no_wrap(
            icon.to_string(),
            egui::FontId::proportional(FS_TITLE),
            accent(),
        );
        painter.galley(egui::pos2(left, top), icon_galley, accent());
        top += 24.0;
    }
    let title_galley =
        painter.layout_no_wrap(title.to_string(), egui::FontId::proportional(FS_BODY), fg());
    let title_height = title_galley.size().y;
    painter.galley(egui::pos2(left, top), title_galley, fg());
    top += title_height + 8.0;
    let description_galley = painter.layout(
        description.to_string(),
        egui::FontId::proportional(FS_SMALL),
        fg_muted(),
        rect.width() - 32.0,
    );
    painter.galley(egui::pos2(left, top), description_galley, fg_muted());
    response
}

pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &mut OnboardingVm,
    port: u16,
    cert: &str,
    ios_simulators: &[crate::services::devices::IosSimulator],
    status: &str,
    flow_count: usize,
) -> Vec<OnboardingIntent> {
    if !vm.show {
        return vec![];
    }

    let mut out = vec![];
    let block_height = match vm.stage {
        OnboardingStage::ChooseExperience => 319.0,
        OnboardingStage::ChooseTarget => 275.0,
        OnboardingStage::Beginner(_) => 430.0,
        OnboardingStage::Experienced => 430.0,
    };
    ui.add_space(((ui.available_height() - block_height) / 2.0).max(24.0));
    ui.vertical_centered(|ui| {
        ui.set_max_width(760.0);
        match vm.stage {
            OnboardingStage::ChooseExperience => {
                header(ui, ctx, "Welcome to proxyproxy", "", 96.0);
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    let gap = 16.0;
                    let width = ((ui.available_width() - gap) / 2.0).clamp(220.0, 340.0);
                    let start = ((ui.available_width() - (width * 2.0 + gap)) / 2.0).max(0.0);
                    ui.add_space(start);
                    if choice_card(
                        ui,
                        "",
                        "I'm new to HTTP debugging",
                        "Start with a platform and follow a short setup checklist.",
                        width,
                    )
                    .clicked()
                    {
                        out.push(OnboardingIntent::ChooseProfile(OnboardingProfile::Beginner));
                    }
                    ui.add_space(gap);
                    if choice_card(
                        ui,
                        "",
                        "I've used Charles or Proxyman",
                        "Import a session or jump directly to proxy setup.",
                        width,
                    )
                    .clicked()
                    {
                        out.push(OnboardingIntent::ChooseProfile(
                            OnboardingProfile::Experienced,
                        ));
                    }
                });
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(action_button("Skip and open the app", accent()))
                            .clicked()
                        {
                            out.push(OnboardingIntent::Skip);
                        }
                    });
                });
            }
            OnboardingStage::ChooseTarget => {
                header(
                    ui,
                    ctx,
                    "What do you want to inspect?",
                    "Choose a target. The next screen will show only the relevant setup.",
                    64.0,
                );
                ui.add_space(18.0);
                ui.horizontal_wrapped(|ui| {
                    let width = if ui.available_width() < 720.0 {
                        ui.available_width()
                    } else {
                        230.0
                    };
                    for (target, icon, title, description) in [
                        (
                            OnboardingTarget::Web,
                            ph::GLOBE,
                            "Web / desktop",
                            "Browser traffic or this computer.",
                        ),
                        (
                            OnboardingTarget::Ios,
                            ph::APPLE_LOGO,
                            "iOS",
                            "Simulator or a physical iOS device.",
                        ),
                        (
                            OnboardingTarget::Android,
                            ph::ANDROID_LOGO,
                            "Android",
                            "adb emulator or connected device.",
                        ),
                    ] {
                        if choice_card(ui, icon, title, description, width).clicked() {
                            out.push(OnboardingIntent::ChooseTarget(target));
                        }
                        ui.add_space(8.0);
                    }
                });
                footer(ui, &mut out);
            }
            OnboardingStage::Beginner(target) => {
                let (title, subtitle) = match target {
                    OnboardingTarget::Web => (
                        "Set up Web / desktop",
                        "Use a clean Chrome profile for the simplest isolated setup.",
                    ),
                    OnboardingTarget::Ios => (
                        "Set up iOS",
                        "Install the proxy CA and route simulator or device traffic.",
                    ),
                    OnboardingTarget::Android => (
                        "Set up Android",
                        "Use adb for an emulator or connected device.",
                    ),
                };
                header(ui, ctx, title, subtitle, 64.0);
                ui.add_space(14.0);
                steps(ui, target, flow_count, status);
                ui.add_space(12.0);
                card().show(ui, |ui| match target {
                    OnboardingTarget::Web => {
                        if ui.button("Trust CA on this computer").clicked() {
                            out.push(OnboardingIntent::Device(DeviceAction::TrustCa));
                        }
                        if ui.button("Set system proxy").clicked() {
                            out.push(OnboardingIntent::Device(DeviceAction::SetSystemProxy));
                        }
                        if ui.button("Launch clean Chrome").clicked() {
                            out.push(OnboardingIntent::Device(DeviceAction::LaunchChrome));
                        }
                    }
                    OnboardingTarget::Ios => {
                        ui.label(RichText::new("Running iOS simulators").strong());
                        if ios_simulators.is_empty() {
                            ui.label(muted("No running simulators"));
                        } else {
                            for sim in ios_simulators {
                                ui.label(muted(&format!("{} · {}", sim.name, sim.runtime)));
                            }
                            if action_button_sized(
                                ui,
                                "Install CA on running simulators",
                                accent(),
                                280.0,
                            )
                            .clicked()
                            {
                                out.push(OnboardingIntent::Device(
                                    DeviceAction::IosSimInstallCa,
                                ));
                            }
                        }
                        ui.label(muted(
                            "For a physical device, copy the CA path and install it in Settings.",
                        ));
                        if ui.button("Copy CA path").clicked() {
                            ctx.copy_text(cert.to_string());
                        }
                    }
                    OnboardingTarget::Android => {
                        if ui.button("Set proxy with adb").clicked() {
                            out.push(OnboardingIntent::Device(DeviceAction::AndroidSetProxy));
                        }
                        if ui.button("Push CA to Android").clicked() {
                            out.push(OnboardingIntent::Device(DeviceAction::AndroidInstallCa));
                        }
                        ui.label(muted(
                            "Android 7+ apps may ignore user CAs unless they opt in.",
                        ));
                    }
                });
                if flow_count > 0 {
                    ui.add_space(10.0);
                    pill(ui, &format!("{flow_count} captured request(s)"), success());
                }
                footer(ui, &mut out);
            }
            OnboardingStage::Experienced => {
                header(
                    ui,
                    ctx,
                    "Quick start",
                    "Use familiar setup actions or continue directly to the workspace.",
                    64.0,
                );
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| {
                    if action_button_sized(ui, "Import HAR / Charles JSON", accent(), 220.0)
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("HAR / Charles JSON", &["har", "json", "chlsj"])
                            .pick_file()
                        {
                            out.push(OnboardingIntent::ImportFlows(path));
                        }
                    }
                    if action_button_sized(ui, "Trust CA", accent(), 220.0).clicked() {
                        out.push(OnboardingIntent::Device(DeviceAction::TrustCa));
                    }
                    if action_button_sized(ui, "Set system proxy", accent(), 220.0).clicked() {
                        out.push(OnboardingIntent::Device(DeviceAction::SetSystemProxy));
                    }
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if action_button_sized(ui, "Launch Chrome", accent(), 220.0).clicked() {
                        out.push(OnboardingIntent::Device(DeviceAction::LaunchChrome));
                    }
                    let mut ios_clicked = false;
                    ui.scope(|ui| {
                        ios_clicked = ui
                            .add_enabled(
                                !ios_simulators.is_empty(),
                                action_button("iOS Simulator CA", accent()).min_size(egui::vec2(
                                    220.0,
                                    HIT,
                                )),
                            )
                            .clicked();
                    });
                    if ios_clicked {
                        out.push(OnboardingIntent::Device(DeviceAction::IosSimInstallCa));
                    }
                    if action_button_sized(ui, "Android setup", accent(), 220.0).clicked() {
                        out.push(OnboardingIntent::Device(DeviceAction::AndroidSetProxy));
                    }
                });
                if ios_simulators.is_empty() {
                    ui.label(muted("iOS Simulator: no running simulators"));
                } else {
                    let names = ios_simulators
                        .iter()
                        .map(|sim| format!("{} ({})", sim.name, sim.runtime))
                        .collect::<Vec<_>>()
                        .join(", ");
                    ui.label(muted(format!("iOS Simulator: {names}")));
                }
                ui.add_space(12.0);
                card().show(ui, |ui| {
                    ui.label(RichText::new(format!("Proxy: 127.0.0.1:{port}")).monospace());
                    ui.label(
                        RichText::new(format!("CA: {cert}"))
                            .size(FS_SMALL)
                            .color(fg_muted()),
                    );
                    if ui.button("Copy shell environment").clicked() {
                        ctx.copy_text(crate::services::devices::env_snippet(port, cert));
                    }
                });
                if !status.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(status).size(FS_SMALL).color(fg_muted()));
                }
                footer(ui, &mut out);
            }
        }
    });
    out
}

fn steps(ui: &mut egui::Ui, target: OnboardingTarget, flow_count: usize, status: &str) {
    let target_label = match target {
        OnboardingTarget::Web => "1. Choose Web",
        OnboardingTarget::Ios => "1. Choose iOS",
        OnboardingTarget::Android => "1. Choose Android",
    };
    ui.horizontal_wrapped(|ui| {
        pill(ui, target_label, success());
        pill(ui, "2. Trust CA", accent());
        pill(ui, "3. Enable proxy", info());
        if flow_count > 0 {
            pill(ui, "4. Traffic captured", success());
        } else {
            pill(ui, "4. Verify traffic", fg_muted());
        }
    });
    if !status.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(status).size(FS_SMALL).color(fg_muted()));
    }
}

fn footer(ui: &mut egui::Ui, out: &mut Vec<OnboardingIntent>) {
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        if ui.button("Back").clicked() {
            out.push(OnboardingIntent::Back);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(action_button("Open proxyproxy", accent())).clicked() {
                out.push(OnboardingIntent::Complete);
            }
            if ui.button("Skip setup").clicked() {
                out.push(OnboardingIntent::Skip);
            }
        });
    });
}
