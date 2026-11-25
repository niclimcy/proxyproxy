use crate::domain::config::OnboardingProfile;
use crate::viewmodels::devices_vm::{DevicesVm, IosCaStatus};
use crate::viewmodels::intent::DeviceAction;
use crate::viewmodels::{
    intent::{OnboardingIntent, OnboardingTarget},
    onboarding_vm::{OnboardingStage, OnboardingVm},
};
use crate::views::theme::{
    FS_BODY, FS_SMALL, FS_TITLE, HIT, RADIUS, Tint, accent, action_button_sized, border, canvas,
    card, fg, fg_muted, focus_ring, info, muted, pill, success, surface, tint, warning,
};
use eframe::egui::{self, RichText};
use egui_phosphor::regular as ph;

#[derive(Clone, Copy)]
pub struct Props<'a> {
    pub devices: &'a DevicesVm,
    pub port: u16,
    pub cert: &'a str,
    pub status: &'a str,
    pub flow_count: usize,
}

fn header(ui: &mut egui::Ui, ctx: &egui::Context, title: &str, subtitle: &str, icon_size: f32) {
    // uploading the icon is not free, so keep one texture for the app's lifetime
    let id = egui::Id::unique("proxyproxy-onboarding-icon");
    let texture = ctx
        .data(|d| d.get_temp::<egui::TextureHandle>(id))
        .unwrap_or_else(|| {
            let icon = crate::views::icon::data();
            let texture = ctx.load_texture(
                "proxyproxy-onboarding-icon",
                egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                ),
                egui::TextureOptions::LINEAR,
            );
            ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
            texture
        });
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
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 132.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let hovered = response.hovered();
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(RADIUS),
        if hovered {
            tint(accent(), Tint::Subtle)
        } else {
            surface()
        },
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(RADIUS),
        if hovered {
            egui::Stroke::new(1.5, accent())
        } else {
            egui::Stroke::new(1.0, border())
        },
        egui::StrokeKind::Inside,
    );

    let left = rect.left() + 16.0;
    let mut top = rect.top() + 16.0;
    let icon_galley = painter.layout_no_wrap(
        icon.to_string(),
        egui::FontId::proportional(FS_TITLE),
        accent(),
    );
    painter.galley(egui::pos2(left, top), icon_galley, accent());
    top += 24.0;
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
    focus_ring(ui, &response, RADIUS);
    response
}

pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &mut OnboardingVm,
    props: Props,
) -> Vec<OnboardingIntent> {
    let mut out = vec![];
    if !vm.show {
        return out;
    }
    ui.painter()
        .rect_filled(ui.max_rect(), egui::CornerRadius::ZERO, canvas());

    // centre on last frame's measured height; content height varies by stage
    let height_id = egui::Id::unique("onboarding-content-height");
    let height = ctx.data(|d| d.get_temp::<f32>(height_id)).unwrap_or(400.0);
    ui.add_space(((ui.available_height() - height) / 2.0).max(24.0));
    let drawn = ui.vertical_centered(|ui| {
        ui.set_max_width(760.0);
        match vm.stage {
            OnboardingStage::ChooseExperience => choose_experience(ui, ctx, &mut out),
            OnboardingStage::ChooseTarget => choose_target(ui, ctx, &mut out),
            OnboardingStage::Beginner(target) => beginner(ui, ctx, vm, target, &props, &mut out),
            OnboardingStage::Experienced => experienced(ui, ctx, vm, &props, &mut out),
        }
    });
    let measured = drawn.response.rect.height();
    if (measured - height).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(height_id, measured));
        ctx.request_repaint();
    }
    out
}

fn choose_experience(ui: &mut egui::Ui, ctx: &egui::Context, out: &mut Vec<OnboardingIntent>) {
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action_button_sized(
                ui,
                ph::ARROW_RIGHT,
                "Skip and open the app",
                accent(),
                210.0,
                true,
            )
            .clicked()
            {
                out.push(OnboardingIntent::Finish);
            }
        });
    });
}

fn choose_target(ui: &mut egui::Ui, ctx: &egui::Context, out: &mut Vec<OnboardingIntent>) {
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
    footer(ui, out, content_width);
}

fn beginner(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &OnboardingVm,
    target: OnboardingTarget,
    props: &Props,
    out: &mut Vec<OnboardingIntent>,
) {
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
    steps(ui, target, props.flow_count, props.status);
    ui.add_space(12.0);
    ui.allocate_ui(egui::vec2(760.0, 180.0), |ui| {
        card().show(ui, |ui| match target {
            OnboardingTarget::Web => web_setup(ui, props.devices, out),
            OnboardingTarget::Ios => ios_setup(ui, vm, props, out),
            OnboardingTarget::Android => {
                if action_button_sized(
                    ui,
                    ph::GEAR_SIX,
                    "Set proxy with adb",
                    accent(),
                    220.0,
                    true,
                )
                .clicked()
                {
                    out.push(OnboardingIntent::Device(DeviceAction::AndroidSetProxy));
                }
                ui.add_space(8.0);
                if action_button_sized(
                    ui,
                    ph::SHIELD_CHECK,
                    "Push CA to Android",
                    accent(),
                    220.0,
                    true,
                )
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
    if props.flow_count > 0 {
        ui.add_space(10.0);
        pill(
            ui,
            &format!("{} captured request(s)", props.flow_count),
            success(),
        );
    }
    footer(ui, out, 760.0);
}

fn web_setup(ui: &mut egui::Ui, devices: &DevicesVm, out: &mut Vec<OnboardingIntent>) {
    if action_button_sized(
        ui,
        ph::SHIELD_CHECK,
        "Trust CA on this computer",
        accent(),
        240.0,
        true,
    )
    .clicked()
    {
        out.push(OnboardingIntent::Device(DeviceAction::TrustCa));
    }
    ui.add_space(8.0);
    let (state, color, action) = if devices.system_proxy_enabled {
        ("System proxy on", success(), "Disable system proxy")
    } else {
        ("System proxy off", fg_muted(), "Enable system proxy")
    };
    ui.label(RichText::new(state).color(color));
    if action_button_sized(ui, ph::GEAR_SIX, action, accent(), 240.0, true).clicked() {
        out.push(OnboardingIntent::Device(DeviceAction::ToggleSystemProxy));
    }
    ui.add_space(8.0);
    if action_button_sized(
        ui,
        ph::BROWSERS,
        "Launch clean Chrome",
        accent(),
        240.0,
        true,
    )
    .clicked()
    {
        out.push(OnboardingIntent::Device(DeviceAction::LaunchChrome));
    }
}

fn ios_setup(ui: &mut egui::Ui, vm: &OnboardingVm, props: &Props, out: &mut Vec<OnboardingIntent>) {
    let devices = props.devices;
    let Some(selected) = devices.selected_sim(vm.selected_ios_udid.as_deref()) else {
        ui.label(RichText::new("No running iOS simulator").strong());
        ui.add_space(6.0);
        ui.label(muted(
            "Boot a simulator in Xcode or the Simulator app, then return here.",
        ));
        return;
    };
    if devices.running().nth(1).is_some() {
        egui::ComboBox::from_id_salt("onboarding-ios-simulator")
            .width(360.0)
            .selected_text(format!("{} · {}", selected.name, selected.runtime))
            .show_ui(ui, |ui| {
                for sim in devices.running() {
                    let text = format!("{} · {}", sim.name, sim.runtime);
                    if ui
                        .selectable_label(sim.udid == selected.udid, text)
                        .clicked()
                    {
                        out.push(OnboardingIntent::SelectIosSimulator(sim.udid.clone()));
                    }
                }
            });
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
                ui.label(
                    RichText::new(ph::DEVICE_MOBILE)
                        .size(FS_TITLE)
                        .color(accent()),
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new(&selected.name).strong());
                    ui.label(muted(&selected.runtime));
                });
            });
            ui.add_space(8.0);
            if selected.ca_status == IosCaStatus::Installed {
                ui.label(RichText::new("Certificate installed").color(success()));
                ui.add_space(8.0);
                ui.label(muted(
                    "Configure the simulator proxy in Settings if needed.",
                ));
                ui.add_space(6.0);
                if action_button_sized(
                    ui,
                    ph::ARROWS_CLOCKWISE,
                    "Reboot simulator",
                    accent(),
                    220.0,
                    true,
                )
                .clicked()
                {
                    out.push(OnboardingIntent::Device(DeviceAction::IosSimReboot(
                        selected.udid.clone(),
                    )));
                }
            } else {
                let (text, color) = if selected.ca_status == IosCaStatus::Missing {
                    ("Certificate missing", warning())
                } else {
                    ("Certificate status unknown", fg_muted())
                };
                ui.label(RichText::new(text).color(color));
                ui.add_space(8.0);
                if action_button_sized(ui, ph::SHIELD_CHECK, "Install CA", accent(), 180.0, true)
                    .clicked()
                {
                    out.push(OnboardingIntent::Device(DeviceAction::IosSimInstallCa(
                        selected.udid.clone(),
                    )));
                }
            }
        });
    ui.add_space(6.0);
    ui.label(muted(format!("Capture proxy: 127.0.0.1:{}", props.port)));
}

fn experienced(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &OnboardingVm,
    props: &Props,
    out: &mut Vec<OnboardingIntent>,
) {
    let devices = props.devices;
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
            if action_button_sized(
                ui,
                ph::FOLDER_OPEN,
                "Import HAR / Charles JSON",
                accent(),
                220.0,
                true,
            )
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
            let proxy_label = if devices.system_proxy_enabled {
                "Disable system proxy"
            } else {
                "Enable system proxy"
            };
            if action_button_sized(ui, ph::GEAR_SIX, proxy_label, accent(), 220.0, true).clicked() {
                out.push(OnboardingIntent::Device(DeviceAction::ToggleSystemProxy));
            }
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if action_button_sized(ui, ph::BROWSERS, "Launch Chrome", accent(), 220.0, true)
                .clicked()
            {
                out.push(OnboardingIntent::Device(DeviceAction::LaunchChrome));
            }
            let missing = devices.missing_ca().len();
            let label = if missing == 1 {
                "Install iOS Simulator CA"
            } else {
                "Install iOS Simulator CAs"
            };
            if action_button_sized(ui, ph::APPLE_LOGO, label, accent(), 220.0, missing > 0)
                .clicked()
            {
                out.push(OnboardingIntent::Device(DeviceAction::InstallMissingIosCas));
            }
            if action_button_sized(ui, ph::ANDROID_LOGO, "Android setup", accent(), 220.0, true)
                .clicked()
            {
                out.push(OnboardingIntent::Device(DeviceAction::AndroidSetProxy));
            }
        });
    });
    ui.label(muted(match devices.running().count() {
        0 => "iOS Simulator: no running simulator".to_string(),
        n => format!("iOS Simulator: {n} running"),
    }));
    ui.add_space(12.0);
    ui.allocate_ui(egui::vec2(content_width, 92.0), |ui| {
        card().show(ui, |ui| {
            ui.label(RichText::new(format!("Proxy: 127.0.0.1:{}", props.port)).monospace());
            ui.label(
                RichText::new(format!("CA: {}", props.cert))
                    .size(FS_SMALL)
                    .color(fg_muted()),
            );
            ui.add_space(8.0);
            let (icon, text) = if vm.env_copied.on() {
                (ph::CHECK, "Copied")
            } else {
                (ph::TERMINAL, "Copy shell environment")
            };
            if action_button_sized(ui, icon, text, accent(), 220.0, true).clicked() {
                ctx.copy_text(crate::services::devices::env_snippet(
                    props.port, props.cert,
                ));
                out.push(OnboardingIntent::EnvCopied);
            }
        });
    });
    if !props.status.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(props.status).size(FS_SMALL).color(fg_muted()));
    }
    footer(ui, out, content_width);
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
                    out.push(OnboardingIntent::Finish);
                }
            });
        });
    });
}
