use crate::domain::config::Shared;
use crate::domain::flow::Event;
use crate::domain::repaint::Repaint;
use crate::proxy;
use crate::services::{self, flow_store::FlowStore};
use crate::viewmodels::{
    capture_vm::CaptureVm,
    detail_vm::DetailVm,
    intent::{Effect, ToolbarIntent},
    network_vm::NetworkVm,
    replay_vm::ReplayVm,
    rules_vm::{RulesTab, RulesVm},
    schemas_vm::SchemasVm,
    session_vm::SessionVm,
    status::Status,
};
use crate::views::{self, theme::apply_theme};
use eframe::egui;
use std::sync::{Arc, Mutex, mpsc::{Receiver, channel}};
use std::time::Duration;

struct EguiRepaint(egui::Context);

impl Repaint for EguiRepaint {
    fn request(&self) {
        self.0.request_repaint();
    }
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
    logo: egui::TextureHandle,
}

impl App {
    fn new(ctx: &egui::Context, port: u16, rx: Receiver<Event>, cfg: Shared, store: FlowStore, api_port: u16) -> Self {
        let logo = ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied([256, 256], &views::icon::rgba()), egui::TextureOptions::LINEAR);
        let repaint: Arc<dyn Repaint> = Arc::new(EguiRepaint(ctx.clone()));
        let cert = proxy::tls::cert_path().display().to_string();
        let session = SessionVm::new(store.clone(), cfg.clone());
        let status_text = session.restore().unwrap_or_else(|| format!("CA: {cert}"));
        Self {
            rx,
            capture: CaptureVm::new(store.clone(), cfg.clone()),
            detail: DetailVm::new(store.clone()),
            rules: RulesVm::new(cfg.clone(), store.clone()),
            replay: ReplayVm::new(store.clone(), repaint.clone()),
            network: NetworkVm::new(cfg.clone()),
            schemas: SchemasVm::new(cfg),
            session,
            status: Status::new(status_text, repaint),
            store,
            port,
            api_port,
            cert,
            logo,
        }
    }

    fn drain(&mut self) {
        self.status.poll();
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
            None => {}
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let active_rules = self.rules.active_rule_count();
        let props = views::toolbar::Props { port: self.port, cert: &self.cert, logo: &self.logo, show_rules: &mut self.rules.show, active_rules };
        for i in views::toolbar::show(ui, ctx, &mut self.capture, props) {
            match i {
                ToolbarIntent::Session(s) => self.session.handle(s, &mut self.capture, &mut self.rules, &mut self.status),
                ToolbarIntent::Device(a) => self.session.run_device(a, self.port, self.cert.clone(), &self.status),
                ToolbarIntent::Compose => self.replay.open_compose(),
                other => self.capture.handle_toolbar(other),
            }
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        let (network, schemas, status) = (&self.network, &self.schemas, &mut self.status);
        let mut net_intents = vec![];
        let mut schema_intents = vec![];
        let rule_intents = views::rules_window::show(ctx, &mut self.rules, |ui, tab| match tab {
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
        if let Some(i) = views::breakpoint_window::show(ctx, &mut self.rules) {
            self.rules.handle(i);
        }
        for id in views::replay_window::show(ctx, &mut self.replay) {
            self.replay.send(id);
        }
        self.replay.prune_closed();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.session.shutdown();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain();
        self.capture.refresh_visible();
        for i in views::shortcuts::read(&ctx) {
            let before = self.capture.selected;
            let e = self.capture.handle_shortcut(i);
            if self.capture.selected != before {
                self.detail.on_select();
            }
            self.apply(e);
        }
        self.replay.poll();

        self.toolbar(ui, &ctx);
        views::status_bar::show(ui, &self.status.text, self.api_port, self.rules.paused.len());

        self.schemas.sync();
        self.capture.refresh_tree();
        for i in views::sidebar::show(ui, &mut self.capture) {
            self.capture.handle_sidebar(i);
        }
        for i in views::detail::show(ui, &ctx, &mut self.detail, self.capture.selected) {
            let e = self.detail.handle(i);
            self.apply(e);
        }
        for i in views::table::show(ui, &ctx, &self.capture, self.detail.diff_base, self.port) {
            let before = self.capture.selected;
            let e = self.capture.handle_row(i, &mut self.status);
            if self.capture.selected != before {
                self.detail.on_select();
            }
            self.apply(e);
        }
        self.windows(&ctx);
        self.session.tick();
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

pub fn run(port: u16) -> eframe::Result {
    let api_port = port.wrapping_add(1);
    let (tx, rx) = channel();
    let cfg: Shared = Arc::new(Mutex::new(services::config_store::load()));
    let proxy_cfg = cfg.clone();
    let store = FlowStore::default();
    services::api::serve(api_port, store.clone());

    eframe::run_native(
        "proxyproxy",
        eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 840.0]).with_icon(views::icon::data()), ..Default::default() },
        Box::new(move |cc| {
            apply_theme(&cc.egui_ctx);
            let sink: Arc<dyn proxy::sink::FlowSink> = Arc::new(proxy::sink::ChannelSink::new(tx, Arc::new(EguiRepaint(cc.egui_ctx.clone()))));
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                if let Err(e) = rt.block_on(proxy::run(port, sink, proxy_cfg)) {
                    eprintln!("proxy error: {e}");
                }
            });
            Ok(Box::new(App::new(&cc.egui_ctx, port, rx, cfg, store, api_port)))
        }),
    )
}
