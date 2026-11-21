use super::capture_vm::CaptureVm;
use super::intent::{DeviceAction, SessionIntent};
use super::rules_vm::RulesVm;
use super::status::Status;
use crate::domain::config::Shared;
use crate::domain::flow::Flow;
use crate::services::{config_store, devices, exporter, flow_store::FlowStore, importer, session};
use std::time::{Duration, Instant};

pub struct SessionVm {
    pub store: FlowStore,
    pub cfg: Shared,
    cfg_saved: String,
    cfg_checked: Instant,
    saved_rev: u64,
    last_save: Instant,
}

fn cfg_json(cfg: &Shared) -> String {
    serde_json::to_string(&*cfg.lock().unwrap()).unwrap_or_default()
}

impl SessionVm {
    pub fn new(store: FlowStore, cfg: Shared) -> Self {
        let cfg_saved = cfg_json(&cfg);
        Self {
            store,
            cfg,
            cfg_saved,
            cfg_checked: Instant::now(),
            saved_rev: 0,
            last_save: Instant::now(),
        }
    }

    // restores the last autosave; returns a status line when something was restored
    pub fn restore(&self) -> Option<String> {
        match session::load(&session::autosave_path()) {
            Ok(f) if !f.is_empty() => {
                let n = f.len();
                self.store.lock().replace(f);
                Some(format!("Restored {n} requests from last session"))
            }
            _ => None,
        }
    }

    pub fn tick(&mut self) {
        if self.cfg_checked.elapsed() > Duration::from_secs(2) {
            self.cfg_checked = Instant::now();
            self.save_config_if_changed();
        }
        if self.last_save.elapsed() > Duration::from_secs(30)
            && self.saved_rev != self.store.lock().rev()
        {
            self.save_now();
        }
    }

    pub fn save_config_if_changed(&mut self) {
        let cur = cfg_json(&self.cfg);
        if cur != self.cfg_saved {
            config_store::save(&self.cfg.lock().unwrap());
            self.cfg_saved = cur;
        }
    }

    pub fn save_now(&mut self) {
        let st = self.store.lock();
        let _ = session::save(&session::autosave_path(), &st.flows);
        self.saved_rev = st.rev();
        self.last_save = Instant::now();
    }

    pub fn shutdown(&mut self) {
        self.save_now();
        config_store::save(&self.cfg.lock().unwrap());
    }

    pub fn handle(
        &mut self,
        intent: SessionIntent,
        capture: &mut CaptureVm,
        rules: &mut RulesVm,
        status: &mut Status,
    ) {
        match intent {
            SessionIntent::New => {
                capture.remove_flows(|f| f.pinned);
                status.set("New session (pinned requests kept)");
            }
            SessionIntent::Open(p) => {
                status.set(match session::load(&p) {
                    Ok(f) => {
                        let n = f.len();
                        self.store.lock().replace(f);
                        capture.selected = None;
                        format!("Opened {n} requests from {}", p.display())
                    }
                    Err(e) => format!("Error: {e}"),
                });
            }
            SessionIntent::SaveAs(p) => {
                let st = self.store.lock();
                status.set(match session::save(&p, &st.flows) {
                    Ok(()) => format!("Saved {} requests to {}", st.flows.len(), p.display()),
                    Err(e) => format!("Error: {e}"),
                });
            }
            SessionIntent::ImportFlows(p) => {
                let res = std::fs::read(&p)
                    .map_err(|e| e.to_string())
                    .and_then(|d| importer::import_bytes(&d));
                status.set(match res {
                    Ok(fl) => {
                        let n = fl.len();
                        self.store.lock().extend(fl);
                        format!("Imported {n} requests")
                    }
                    Err(e) => format!("Import failed: {e}"),
                });
            }
            SessionIntent::ExportHar(p) => self.export(&p, capture, status, exporter::har),
            SessionIntent::ExportCsv(p) => self.export(&p, capture, status, exporter::csv),
            SessionIntent::ExportRules(p) => {
                status.set(match std::fs::write(&p, rules.rules_json()) {
                    Ok(()) => format!("Rules saved to {}", p.display()),
                    Err(e) => format!("Error: {e}"),
                });
            }
            SessionIntent::ImportRules(p) => match std::fs::read_to_string(&p) {
                Ok(t) => rules.import_rules(&t, status),
                Err(e) => status.set(format!("Error: {e}")),
            },
        }
    }

    fn export(
        &self,
        p: &std::path::Path,
        capture: &CaptureVm,
        status: &mut Status,
        render: fn(&[&Flow]) -> String,
    ) {
        let (data, n) = {
            let st = self.store.lock();
            let visible: Vec<&Flow> = capture
                .visible
                .iter()
                .filter_map(|&i| st.flows.get(i))
                .collect();
            (render(&visible), visible.len())
        };
        status.set(match std::fs::write(p, data) {
            Ok(()) => format!("Exported {n} requests to {}", p.display()),
            Err(e) => format!("Error: {e}"),
        });
    }

    pub fn run_device(&self, action: DeviceAction, port: u16, cert: String, status: &Status) {
        let jobs = status.jobs.clone();
        match action {
            DeviceAction::TrustCa => jobs.spawn(move || devices::trust_ca_on_host(&cert)),
            DeviceAction::SetSystemProxy => jobs.spawn(move || devices::system_proxy(true, port)),
            DeviceAction::ClearSystemProxy => {
                jobs.spawn(move || devices::system_proxy(false, port))
            }
            DeviceAction::LaunchChrome => jobs.spawn(move || devices::launch_chrome(port)),
            DeviceAction::IosSimInstallCa => jobs.spawn(move || devices::ios_sim_install_ca(&cert)),
            DeviceAction::AndroidSetProxy => jobs.spawn(move || devices::android_set_proxy(port)),
            DeviceAction::AndroidClearProxy => {
                jobs.spawn(move || devices::android_clear_proxy(port))
            }
            DeviceAction::AndroidInstallCa => {
                jobs.spawn(move || devices::android_install_ca(&cert))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::viewmodels::status::NoRepaint;
    use std::sync::{Arc, Mutex};

    struct Rig {
        s: SessionVm,
        c: CaptureVm,
        r: RulesVm,
        st: Status,
        dir: std::path::PathBuf,
    }

    fn rig(tag: &str) -> Rig {
        let store = FlowStore::default();
        let cfg: Shared = Arc::new(Mutex::new(Config::default()));
        let dir = std::env::temp_dir().join(format!("pxp-svm-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Rig {
            s: SessionVm::new(store.clone(), cfg.clone()),
            c: CaptureVm::new(store.clone(), cfg.clone()),
            r: RulesVm::new(cfg, store),
            st: Status::new(String::new(), Arc::new(NoRepaint)),
            dir,
        }
    }

    fn add(r: &Rig, id: u64, pinned: bool) {
        let mut f = Flow::sample(id);
        f.pinned = pinned;
        r.s.store.lock().push(f);
    }

    #[test]
    fn new_session_keeps_pinned_only() {
        let mut r = rig("new");
        add(&r, 1, true);
        add(&r, 2, false);
        r.s.handle(SessionIntent::New, &mut r.c, &mut r.r, &mut r.st);
        assert_eq!(r.s.store.lock().flows.len(), 1);
        assert!(r.st.text.contains("pinned requests kept"));
    }

    #[test]
    fn save_as_then_open_roundtrips_and_clears_selection() {
        let mut r = rig("open");
        add(&r, 10, false);
        add(&r, 11, false);
        let p = r.dir.join("s.pxp");
        r.s.handle(
            SessionIntent::SaveAs(p.clone()),
            &mut r.c,
            &mut r.r,
            &mut r.st,
        );
        assert!(r.st.text.starts_with("Saved 2 requests"));
        r.s.store.lock().replace(vec![]);
        r.c.selected = Some(10);
        r.s.handle(SessionIntent::Open(p), &mut r.c, &mut r.r, &mut r.st);
        assert_eq!(r.s.store.lock().flows.len(), 2);
        assert_eq!(r.c.selected, None);
        r.s.handle(
            SessionIntent::Open(r.dir.join("missing.pxp")),
            &mut r.c,
            &mut r.r,
            &mut r.st,
        );
        assert!(r.st.text.starts_with("Error:"));
        assert_eq!(r.s.store.lock().flows.len(), 2);
    }

    #[test]
    fn har_export_covers_only_visible_rows_and_reimports() {
        let mut r = rig("har");
        for id in 1..=3 {
            let mut f = Flow::sample(id);
            f.status = Some(if id == 2 { 500 } else { 200 });
            r.s.store.lock().push(f);
        }
        r.c.filter = "status:>=500".into();
        r.c.refresh_visible();
        let p = r.dir.join("out.har");
        r.s.handle(
            SessionIntent::ExportHar(p.clone()),
            &mut r.c,
            &mut r.r,
            &mut r.st,
        );
        assert!(r.st.text.starts_with("Exported 1 requests"));
        r.s.handle(SessionIntent::ImportFlows(p), &mut r.c, &mut r.r, &mut r.st);
        assert_eq!(r.st.text, "Imported 1 requests");
        assert_eq!(r.s.store.lock().flows.len(), 4);
    }

    #[test]
    fn csv_export_writes_header_plus_rows() {
        let mut r = rig("csv");
        add(&r, 1, false);
        r.c.refresh_visible();
        let p = r.dir.join("o.csv");
        r.s.handle(
            SessionIntent::ExportCsv(p.clone()),
            &mut r.c,
            &mut r.r,
            &mut r.st,
        );
        assert_eq!(std::fs::read_to_string(p).unwrap().lines().count(), 2);
    }

    #[test]
    fn import_failure_leaves_store_untouched() {
        let mut r = rig("bad");
        add(&r, 1, false);
        let p = r.dir.join("bad.json");
        std::fs::write(&p, "garbage").unwrap();
        r.s.handle(SessionIntent::ImportFlows(p), &mut r.c, &mut r.r, &mut r.st);
        assert!(r.st.text.starts_with("Import failed"));
        assert_eq!(r.s.store.lock().flows.len(), 1);
    }

    #[test]
    fn rules_export_import_via_files() {
        let mut r = rig("rules");
        r.r.form.pattern = "a.com".into();
        r.r.form.kind = crate::viewmodels::rules_vm::RuleKind::Block;
        r.r.handle(crate::viewmodels::intent::RuleIntent::Submit);
        let p = r.dir.join("rules.json");
        r.s.handle(
            SessionIntent::ExportRules(p.clone()),
            &mut r.c,
            &mut r.r,
            &mut r.st,
        );
        assert!(r.st.text.starts_with("Rules saved"));
        r.s.handle(SessionIntent::ImportRules(p), &mut r.c, &mut r.r, &mut r.st);
        assert_eq!(r.s.cfg.lock().unwrap().rules.len(), 2);
        r.s.handle(
            SessionIntent::ImportRules(r.dir.join("nope.json")),
            &mut r.c,
            &mut r.r,
            &mut r.st,
        );
        assert!(r.st.text.starts_with("Error:"));
    }
}
