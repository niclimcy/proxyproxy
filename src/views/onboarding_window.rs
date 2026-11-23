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
    ui.label(RichText::new(title).size(FS_TITLE).strong().color(fg()));
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
        egui::Stroke::new(
            if response.hovered() { 1.5 } else { 1.0 },
            if response.hovered() {
                accent()
            } else {
                border()
            },
        ),
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
    system_proxy_enabled: bool,
    status: &str,
    flow_count: usize,
) -> Vec<OnboardingIntent> {
    if !vm.show {
        return vec![];
    }

    sync(ctx);
    ui.painter()
        .rect_filled(ui.max_rect(), egui::CornerRadius::ZERO, canvas());

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
                let gap = 16.0;
                let width = 340.0;
                let content_width = width * 2.0 + gap;
                ui.allocate_ui(egui::vec2(content_width, 132.0), |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.horizontal(|ui| {
                        if choice_card(
                            ui,
                            ph::LIGHTNING,
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
                            ph::HANDSHAKE,
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
                });
                ui.add_space(18.0);
                ui.allocate_ui(egui::vec2(content_width, HIT), |ui| {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if action_button_sized(ui, ph::ARROW_RIGHT, "Skip and open the app", accent(), 210.0, true)
                            .clicked()
                            {
                                out.push(OnboardingIntent::Skip);
                            }
                        });
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
                let width = 230.0;
                let gap = 8.0;
                let content_width = width * 3.0 + gap * 2.0;
                ui.allocate_ui(egui::vec2(content_width, 132.0), |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.horizontal(|ui| {
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
                        }
                    });
                });
                footer(ui, &mut out, content_width);
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
                ui.allocate_ui(egui::vec2(760.0, 180.0), |ui| {
                    card().show(ui, |ui| match target {
                        OnboardingTarget::Web => {
                            if action_button_sized(ui, ph::SHIELD_CHECK, "Trust CA on this computer", accent(), 240.0, true)
                            .clicked()
                            {
                                out.push(OnboardingIntent::Device(DeviceAction::TrustCa));
                            }
                            ui.add_space(8.0);
                            if system_proxy_enabled {
                                ui.label(RichText::new("System capture enabled").color(success()));
                                if action_button_sized(ui, ph::GEAR_SIX, "Disable system proxy", fg_muted(), 240.0, true)
                                .clicked()
                                {
                                    out.push(OnboardingIntent::Device(DeviceAction::ClearSystemProxy));
                                }
                            } else {
                                ui.label(RichText::new("System capture disabled").color(fg_muted()));
                                if action_button_sized(ui, ph::GEAR_SIX, "Enable system proxy", accent(), 240.0, true)
                                .clicked()
                                {
                                    out.push(OnboardingIntent::Device(DeviceAction::SetSystemProxy));
                                }
                            }
                            ui.add_space(8.0);
                            if action_button_sized(ui, ph::BROWSERS, "Launch clean Chrome", accent(), 240.0, true)
                            .clicked()
                            {
                                out.push(OnboardingIntent::Device(DeviceAction::LaunchChrome));
                            }
                        }
                        OnboardingTarget::Ios => {
                            let running: Vec<_> = ios_simulators
                                .iter()
                                .filter(|sim| {
                                    sim.state
                                        == crate::services::devices::IosSimulatorState::Booted
                                })
                                .collect();
                            if running.is_empty() {
                                ui.label(RichText::new("No running iOS simulator").strong());
                                ui.add_space(6.0);
                                ui.label(muted(
                                    "Boot a simulator in Xcode or the Simulator app, then return here.",
                                ));
                            } else {
                                let selected = running
                                    .iter()
                                    .find(|sim| {
                                        vm.selected_ios_udid.as_deref() == Some(sim.udid.as_str())
                                    })
                                    .copied()
                                    .unwrap_or(running[0]);
                                if vm.selected_ios_udid.as_deref() != Some(selected.udid.as_str()) {
                                    out.push(OnboardingIntent::SelectIosSimulator(
                                        selected.udid.clone(),
                                    ));
                                }

                                if running.len() > 1 {
                                    let mut selected_udid = selected.udid.clone();
                                    egui::ComboBox::from_id_salt("onboarding-ios-simulator")
                                        .width(360.0)
                                        .selected_text(format!(
                                            "{} · {}",
                                            selected.name, selected.runtime
                                        ))
                                        .show_ui(ui, |ui| {
                                            for sim in &running {
                                                let text = format!("{} · {}", sim.name, sim.runtime);
                                                if ui
                                                    .selectable_label(
                                                        selected_udid == sim.udid,
                                                        text,
                                                    )
                                                    .clicked()
                                                {
                                                    selected_udid = sim.udid.clone();
                                                }
                                            }
                                        });
                                    if selected_udid != selected.udid {
                                        out.push(OnboardingIntent::SelectIosSimulator(selected_udid));
                                    }
                                    ui.add_space(8.0);
                                }

                                egui::Frame::group(ui.style())
                                    .fill(tint(accent(), Tint::Subtle))
                                    .inner_margin(egui::Margin::symmetric(14, 12))
                                    .corner_radius(egui::CornerRadius::same(RADIUS))
                                    .stroke(egui::Stroke::new(1.0, border()))
                                    .show(ui, |ui| {
                                        ui.set_min_width(332.0);
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(ph::DEVICE_MOBILE).size(FS_TITLE).color(accent()));
                                            ui.vertical(|ui| {
                                                ui.label(RichText::new(&selected.name).strong());
                                                ui.label(muted(&selected.runtime));
                                            });
                                        });
                                        ui.add_space(8.0);
                                        match selected.ca_status {
                                            crate::services::devices::IosCaStatus::Installed => {
                                                ui.label(RichText::new("Certificate installed").color(success()));
                                                ui.add_space(8.0);
                                                ui.label(muted("Configure the simulator proxy in Settings if needed."));
                                                ui.add_space(6.0);
                                                if action_button_sized(ui, ph::ARROWS_CLOCKWISE, "Reboot simulator", accent(), 220.0, true)
                                                .clicked()
                                                {
                                                    out.push(OnboardingIntent::Device(
                                                        DeviceAction::IosSimReboot(selected.udid.clone()),
                                                    ));
                                                }
                                            }
                                            crate::services::devices::IosCaStatus::Missing => {
                                                ui.label(RichText::new("Certificate missing").color(warning()));
                                                ui.add_space(8.0);
                                                if action_button_sized(ui, ph::SHIELD_CHECK, "Install CA", accent(), 180.0, true)
                                                .clicked()
                                                {
                                                    out.push(OnboardingIntent::Device(
                                                        DeviceAction::IosSimInstallCa(selected.udid.clone()),
                                                    ));
                                                }
                                            }
                                            crate::services::devices::IosCaStatus::Unknown => {
                                                ui.label(RichText::new("Certificate status unknown").color(fg_muted()));
                                                ui.add_space(8.0);
                                                if action_button_sized(ui, ph::SHIELD_CHECK, "Install CA", accent(), 180.0, true)
                                                .clicked()
                                                {
                                                    out.push(OnboardingIntent::Device(
                                                        DeviceAction::IosSimInstallCa(selected.udid.clone()),
                                                    ));
                                                }
                                            }
                                        }
                                    });
                                ui.add_space(6.0);
                                ui.label(muted(format!(
                                    "Capture proxy: 127.0.0.1:{port}",
                                )));
                            }
                        }
                        OnboardingTarget::Android => {
                            if action_button_sized(ui, ph::GEAR_SIX, "Set proxy with adb", accent(), 220.0, true)
                            .clicked()
                            {
                                out.push(OnboardingIntent::Device(DeviceAction::AndroidSetProxy));
                            }
                            ui.add_space(8.0);
                            if action_button_sized(ui, ph::SHIELD_CHECK, "Push CA to Android", accent(), 220.0, true)
                            .clicked()
                            {
                                out.push(OnboardingIntent::Device(DeviceAction::AndroidInstallCa));
                            }
                            ui.label(muted(
                                "Android 7+ apps may ignore user CAs unless they opt in.",
                            ));
                        }
                    });
                });
                if flow_count > 0 {
                    ui.add_space(10.0);
                    pill(ui, &format!("{flow_count} captured request(s)"), success());
                }
                footer(ui, &mut out, 760.0);
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
                let content_width = 676.0;
                ui.allocate_ui(egui::vec2(content_width, 64.0), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if action_button_sized(ui, ph::FOLDER_OPEN, "Import HAR / Charles JSON", accent(), 220.0, true)
                        .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("HAR / Charles JSON", &["har", "json", "chlsj"])
                                .pick_file()
                            {
                                out.push(OnboardingIntent::ImportFlows(path));
                            }
                        if action_button_sized(ui, ph::SHIELD_CHECK, "Trust CA", accent(), 220.0, true)
                        .clicked()
                        {
                            out.push(OnboardingIntent::Device(DeviceAction::TrustCa));
                        }
                        if action_button_sized(ui, ph::GEAR_SIX, if system_proxy_enabled {
                                "Disable system proxy"
                            } else {
                                "Enable system proxy"
                            }, accent(), 220.0, true)
                        .clicked()
                        {
                            out.push(OnboardingIntent::Device(if system_proxy_enabled {
                                DeviceAction::ClearSystemProxy
                            } else {
                                DeviceAction::SetSystemProxy
                            }));
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if action_button_sized(ui, ph::BROWSERS, "Launch Chrome", accent(), 220.0, true)
                        .clicked()
                        {
                            out.push(OnboardingIntent::Device(DeviceAction::LaunchChrome));
                        }
                        let running_missing: Vec<_> = ios_simulators
                            .iter()
                            .filter(|sim| {
                                sim.state == crate::services::devices::IosSimulatorState::Booted
                                    && sim.ca_status
                                        != crate::services::devices::IosCaStatus::Installed
                            })
                            .collect();
                        let mut ios_clicked = false;
                        ui.scope(|ui| {
                            ios_clicked = action_button_sized(ui, ph::APPLE_LOGO, if running_missing.len() == 1 {
                                    "Install iOS Simulator CA"
                                } else {
                                    "Install iOS Simulator CAs"
                                }, accent(), 220.0, !running_missing.is_empty())
                            .clicked();
                        });
                        if ios_clicked {
                            for sim in running_missing {
                                out.push(OnboardingIntent::Device(
                                    DeviceAction::IosSimInstallCa(sim.udid.clone()),
                                ));
                            }
                        }
                        if action_button_sized(ui, ph::ANDROID_LOGO, "Android setup", accent(), 220.0, true)
                        .clicked()
                        {
                            out.push(OnboardingIntent::Device(DeviceAction::AndroidSetProxy));
                        }
                    });
                });
                let running_count = ios_simulators
                    .iter()
                    .filter(|sim| sim.state == crate::services::devices::IosSimulatorState::Booted)
                    .count();
                ui.label(muted(if running_count == 0 {
                    "iOS Simulator: no running simulator".to_string()
                } else if running_count == 1 {
                    "iOS Simulator: 1 running".to_string()
                } else {
                    format!("iOS Simulator: {running_count} running")
                }));
                ui.add_space(12.0);
                ui.allocate_ui(egui::vec2(content_width, 92.0), |ui| {
                    card().show(ui, |ui| {
                        ui.label(RichText::new(format!("Proxy: 127.0.0.1:{port}")).monospace());
                        ui.label(
                            RichText::new(format!("CA: {cert}"))
                                .size(FS_SMALL)
                                .color(fg_muted()),
                        );
                        ui.add_space(8.0);
                        if action_button_sized(ui, ph::TERMINAL, "Copy shell environment", accent(), 220.0, true)
                        .clicked()
                        {
                            ctx.copy_text(crate::services::devices::env_snippet(port, cert));
                        }
                    });
                });
                if !status.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(status).size(FS_SMALL).color(fg_muted()));
                }
                footer(ui, &mut out, content_width);
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
        pill(
            ui,
            if target == OnboardingTarget::Ios {
                "3. Configure simulator traffic"
            } else {
                "3. Enable proxy"
            },
            info(),
        );
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

fn footer(ui: &mut egui::Ui, out: &mut Vec<OnboardingIntent>, content_width: f32) {
    ui.add_space(18.0);
    ui.allocate_ui(egui::vec2(content_width, HIT), |ui| {
        ui.horizontal(|ui| {
            if action_button_sized(ui, ph::ARROW_LEFT, "Back", fg_muted(), 100.0, true).clicked() {
                out.push(OnboardingIntent::Back);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if action_button_sized(
                    ui,
                    ph::ARROW_RIGHT,
                    "Open proxyproxy",
                    accent(),
                    170.0,
                    true,
                )
                .clicked()
                {
                    out.push(OnboardingIntent::Complete);
                }
                ui.add_space(8.0);
                if action_button_sized(ui, ph::FAST_FORWARD, "Skip setup", fg_muted(), 150.0, true)
                    .clicked()
                {
                    out.push(OnboardingIntent::Skip);
                }
            });
        });
    });
}
