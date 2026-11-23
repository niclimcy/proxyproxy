use crate::domain::config::Shared;
use crate::domain::flow::Event;
use crate::domain::repaint::Repaint;
use crate::proxy;
use crate::services::{self, flow_store::FlowStore};
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
    ) -> Self {
        let ctx = &cc.egui_ctx;
        let icon = views::icon::data();
        let menu = services::app_menu::AppMenu::install(win32_hwnd(cc), icon.rgba, icon.width);
        let repaint: Arc<dyn Repaint> = Arc::new(EguiRepaint(ctx.clone()));
        let cert = proxy::tls::cert_path().display().to_string();
        let session = SessionVm::new(store.clone(), cfg.clone());
        let onboarding = OnboardingVm::new(cfg.clone());
        let status_text = session.restore().unwrap_or_default();
        Self {
            rx,
            capture: CaptureVm::new(store.clone(), cfg.clone()),
            detail: DetailVm::new(store.clone()),
            rules: RulesVm::new(cfg.clone(), store.clone()),
            replay: ReplayVm::new(store.clone(), repaint.clone()),
            network: NetworkVm::new(cfg.clone()),
            schemas: SchemasVm::new(cfg),
            session,
            devices: DevicesVm::new(port, cert.clone(), repaint.clone()),
            status: Status::new(status_text, repaint),
            store,
            port,
            api_port,
            cert,
            menu,
            about: AboutVm::default(),
            onboarding,
        }
    }

    fn drain(&mut self) {
        self.status.poll();
        if self.menu.take_setup_guide_request() {
            self.onboarding.open();
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
                ToolbarIntent::Onboarding => self.onboarding.open(),
                other => self.capture.handle_toolbar(other),
            }
        }
    }

    fn rules_panel(&mut self, ui: &mut egui::Ui) {
        let (network, schemas, status) = (&self.network, &self.schemas, &mut self.status);
        let mut net_intents = vec![];
        let mut schema_intents = vec![];
        let rule_intents = views::rules_panel::show(ui, &mut self.rules, |ui, tab| match tab {
            RulesTab::Network => net_intents = views::network_tab::show(ui, network),
            RulesTab::Schemas => schema_intents = views::schemas_tab::show(ui, schemas),
            _ => {}
        });
        for i in net_intents {
            self.network.handle(i, status);
        }
        for i in schema_intents {
            self.schemas.handle(i);
        }
        for i in rule_intents {
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
            self.capture.selected,
            self.capture.has_flows(),
        ) {
            let e = self.detail.handle(i);
            self.apply(e);
        }
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
        self.devices.tick();
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
    let cfg: Shared = Arc::new(Mutex::new(services::config_store::load()));
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
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                if let Err(e) = rt.block_on(proxy::run(port, sink, proxy_cfg)) {
                    eprintln!("proxy error: {e}");
                }
            });
            Ok(Box::new(App::new(cc, port, rx, cfg, store, api_port)))
        }),
    )
}
