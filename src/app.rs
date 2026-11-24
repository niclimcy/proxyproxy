use crate::domain::config::Shared;
use crate::domain::flow::Event;
use crate::domain::repaint::Repaint;
use crate::proxy;
use crate::services::{self, app_menu::MenuRequest, flow_store::FlowStore};
use crate::viewmodels::{
    about_vm::AboutVm,
    capture_vm::CaptureVm,
    detail_vm::DetailVm,
    devices_vm::DevicesVm,
    intent::{Effect, SessionIntent, ToolbarIntent},
    network_vm::NetworkVm,
    onboarding_vm::OnboardingVm,
    replay_vm::ReplayVm,
    rules_vm::{RulesTab, RulesVm},
    schemas_vm::SchemasVm,
    session_vm::SessionVm,
    settings_vm::SettingsVm,
    status::Status,
};
use crate::views::{self, theme::apply_theme};
use eframe::egui;
use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, channel},
};
use std::time::Duration;

struct EguiRepaint(egui::Context);

impl Repaint for EguiRepaint {
    fn request(&self) {
        self.0.request_repaint();
    }
}

#[cfg(target_os = "windows")]
fn win32_hwnd(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(target_os = "windows"))]
fn win32_hwnd(_cc: &eframe::CreationContext<'_>) -> Option<isize> {
    None
}

struct App {
    rx: Receiver<Event>,
    store: FlowStore,
    capture: CaptureVm,
    detail: DetailVm,
    rules: RulesVm,
    replay: ReplayVm,
    network: NetworkVm,
    schemas: SchemasVm,
    session: SessionVm,
    status: Status,
    port: u16,
    api_port: u16,
    cert: String,
    menu: services::app_menu::AppMenu,
    devices: DevicesVm,
    about: AboutVm,
    settings: SettingsVm,
    onboarding: OnboardingVm,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        port: u16,
        rx: Receiver<Event>,
        cfg: Shared,
        store: FlowStore,
        api_port: u16,
        cfg_note: Option<String>,
    ) -> Self {
        let ctx = &cc.egui_ctx;
        let icon = views::icon::data();
        let menu = services::app_menu::AppMenu::install(win32_hwnd(cc), icon.rgba, icon.width);
        let repaint: Arc<dyn Repaint> = Arc::new(EguiRepaint(ctx.clone()));
        let cert = proxy::tls::cert_path().display().to_string();
        let session = SessionVm::new(store.clone(), cfg.clone());
        let onboarding = OnboardingVm::new(cfg.clone());
        let restored = session.restore();
        let status_text = cfg_note.or(restored).unwrap_or_default();
        Self {
            rx,
            capture: CaptureVm::new(
                store.clone(),
                cfg.clone(),
                services::app_icons::IconStore::new(
                    services::app_icons::icons_dir(),
                    repaint.clone(),
                ),
            ),
            detail: DetailVm::new(store.clone()),
            rules: RulesVm::new(cfg.clone(), store.clone()),
            replay: ReplayVm::new(store.clone(), repaint.clone()),
            network: NetworkVm::new(cfg.clone()),
            schemas: SchemasVm::new(cfg.clone()),
            session,
            devices: DevicesVm::new(port, cert.clone(), cfg, repaint.clone()),
            status: Status::new(status_text, repaint),
            store,
            port,
            api_port,
            cert,
            menu,
            about: AboutVm::default(),
            settings: SettingsVm::default(),
            onboarding,
        }
    }

    fn drain(&mut self) {
        self.status.poll();
        for r in self.menu.take_requests() {
            match r {
                MenuRequest::SetupGuide => self.onboarding.open(),
                MenuRequest::Settings => self.settings.open(),
            }
        }
        let rules = &mut self.rules;
        self.store.pump(&self.rx, |ev| match ev {
            Event::Paused(p) => rules.pause(p),
            Event::Log(l) => rules.log(l),
            _ => {}
        });
    }

    fn apply(&mut self, effect: Option<Effect>) {
        match effect {
            Some(Effect::OpenReplay(id)) => self.replay.open_replay(id),
            Some(Effect::RuleFromFlow(id, kind)) => self.rules.open_from_flow(id, kind),
            Some(Effect::ShowOverview) => self.detail.show_overview(),
            Some(Effect::DiffBase(id)) => self.detail.use_as_base(id),
            Some(Effect::Device(action)) => self.devices.run(action, &self.status),
            Some(Effect::OpenSettings) => self.settings.open(),
            Some(Effect::CloseDetail) => {
                self.capture.selected = None;
                self.detail.on_select();
            }
            Some(Effect::ImportFlows(path)) => self.session.handle(
                SessionIntent::ImportFlows(path),
                &mut self.capture,
                &mut self.rules,
                &mut self.status,
            ),
            None => {}
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let active_rules = self.rules.active_rule_count();
        let props = views::toolbar::Props {
            port: self.port,
            cert: &self.cert,
            devices: &self.devices,
            show_rules: &mut self.rules.show,
            active_rules,
            status: &self.status.text,
            api_port: self.api_port,
            paused_breakpoints: self.rules.paused.len(),
            show_help: !self.menu.is_native(),
        };
        for i in views::toolbar::show(ui, ctx, &mut self.capture, props) {
            match i {
                ToolbarIntent::Session(s) => {
                    self.session
                        .handle(s, &mut self.capture, &mut self.rules, &mut self.status)
                }
                ToolbarIntent::Device(a) => self.devices.run(a, &self.status),
                ToolbarIntent::Compose => self.replay.open_compose(),
                ToolbarIntent::About => self.about.open(),
                ToolbarIntent::Settings => self.settings.open(),
                ToolbarIntent::ShowBreakpoint => self.rules.focus_breakpoint = true,
                ToolbarIntent::ShowScope => {
                    self.rules.show = true;
                    self.rules.tab = RulesTab::Scope;
                }
                ToolbarIntent::Onboarding => self.onboarding.open(),
                other => self.capture.handle_toolbar(other),
            }
        }
    }

    fn rules_panel(&mut self, ui: &mut egui::Ui) {
        for i in views::rules_panel::show(ui, &mut self.rules) {
            let e = self.rules.handle(i);
            self.apply(e);
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        if let Some(i) = views::breakpoint_window::show(ctx, &mut self.rules) {
            self.rules.handle(i);
        }
        for id in views::replay_window::show(ctx, &mut self.replay) {
            self.replay.send(id);
        }
        self.replay.prune_closed();
        views::about_window::show(ctx, &mut self.about);
        let (net, schemas) =
            views::settings_window::show(ctx, &mut self.settings, &self.network, &self.schemas);
        for i in net {
            self.network.handle(i, &mut self.status);
        }
        for i in schemas {
            self.schemas.handle(i);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.session.shutdown();
        self.devices.shutdown();
    }
}

impl App {
    fn onboarding(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let flow_count = self.store.lock().flows.len();
        let props = views::onboarding_window::Props {
            devices: &self.devices,
            port: self.port,
            cert: &self.cert,
            status: &self.status.text,
            flow_count,
        };
        for i in views::onboarding_window::show(ui, ctx, &mut self.onboarding, props) {
            let e = self.onboarding.handle(i);
            self.apply(e);
        }
    }

    fn workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.capture.refresh_visible();
        for i in views::shortcuts::read(ctx) {
            let before = self.capture.selected;
            let e = self.capture.handle_shortcut(i);
            if self.capture.selected != before {
                self.detail.on_select();
            }
            self.apply(e);
        }

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0, views::theme::backdrop());
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(full.shrink(views::theme::GAP as f32 / 2.0)));
        let ui = &mut ui;

        self.toolbar(ui, ctx);

        self.schemas.sync();
        self.capture.refresh_tree();
        for i in views::sidebar::show(ui, &mut self.capture) {
            self.capture.handle_sidebar(i);
        }
        self.rules_panel(ui);
        for i in views::detail::show(
            ui,
            ctx,
            &mut self.detail,
            &self.capture.icons,
            self.capture.selected,
            self.capture.has_flows(),
        ) {
            let e = self.detail.handle(i);
            self.apply(e);
        }
        // toolbar and session intents above can replace the flows this frame
        self.capture.refresh_visible();
        for i in views::table::show(ui, ctx, &self.capture, self.detail.diff_base, self.port) {
            let before = self.capture.selected;
            let e = self.capture.handle_row(i, &mut self.status);
            if self.capture.selected != before {
                self.detail.on_select();
            }
            self.apply(e);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        views::theme::sync(&ctx);
        self.drain();
        self.devices.tick(&self.status);
        self.replay.poll();
        if self.onboarding.show {
            self.onboarding(ui, &ctx);
        } else {
            self.workspace(ui, &ctx);
        }
        self.windows(&ctx);
        self.session.tick();
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

pub fn run(port: u16) -> eframe::Result {
    let api_port = services::api::port_for(port);
    let (tx, rx) = channel();
    let (cfg, cfg_note) = services::config_store::load(&services::config_store::path());
    let cfg: Shared = Arc::new(Mutex::new(cfg));
    let proxy_cfg = cfg.clone();
    let store = FlowStore::default();
    services::api::serve(api_port, store.clone(), cfg.clone());

    eframe::run_native(
        "proxyproxy",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1360.0, 840.0])
                .with_min_inner_size([560.0, 400.0])
                .with_icon(views::icon::data()),
            ..Default::default()
        },
        Box::new(move |cc| {
            apply_theme(&cc.egui_ctx);
            cc.egui_ctx.set_embed_viewports(false);
            #[cfg(target_os = "macos")]
            if let Some(mtm) = objc2::MainThreadMarker::new() {
                // macOS would otherwise merge child viewports into tabs
                objc2_app_kit::NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
            }
            let sink: Arc<dyn proxy::sink::FlowSink> = Arc::new(proxy::sink::ChannelSink::new(
                tx,
                Arc::new(EguiRepaint(cc.egui_ctx.clone())),
            ));
            let app = App::new(cc, port, rx, cfg, store, api_port, cfg_note);
            // the proxy only returns on failure (say, the port is taken), so that lands in the status line
            app.status.jobs.spawn(move || {
                let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                rt.block_on(proxy::run(port, sink, proxy_cfg))
                    .map_err(|e| format!("proxy on port {port} stopped: {e}"))?;
                Ok(String::new())
            });
            Ok(Box::new(app))
        }),
    )
}
