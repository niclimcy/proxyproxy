use super::intent::DeviceAction;
use super::status::Status;
use crate::domain::config::Shared;
use crate::domain::pac::Route;
use crate::domain::repaint::Repaint;
use crate::services::devices::{self, DeviceSnapshot};
pub use crate::services::devices::{AndroidDevice, IosCaStatus, IosSimulator, IosSimulatorState};
use crate::services::sysproxy;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{Receiver, Sender, channel},
};
use std::time::{Duration, Instant};

const POLL_EVERY: Duration = Duration::from_secs(4);
const POLL_AFTER_ACTION: Duration = Duration::from_secs(1);

/// Simulator and system-proxy state, polled off the UI thread because each poll runs
/// several subprocesses.
pub struct DevicesVm {
    pub ios_simulators: Vec<IosSimulator>,
    pub android: Vec<AndroidDevice>,
    /// Global or PAC proxy active on any network service.
    pub system_proxy_enabled: bool,
    global_enabled: bool,
    pac_enabled: bool,
    // what we last applied, so list edits while enabled can rewrite the PAC
    applied: Option<Route>,
    cfg: Shared,
    // set when we turn the system proxy on, so quitting only undoes our own change
    proxy_ours: bool,
    port: u16,
    cert: String,
    next_poll: Instant,
    polling: bool,
    // system-proxy changes in flight, and a counter bumped at each start/finish; a poll
    // that overlapped either would read a half-applied setting and flip the checkbox back
    proxy_busy: Arc<AtomicUsize>,
    proxy_gen: Arc<AtomicUsize>,
    tx: Sender<DeviceSnapshot>,
    rx: Receiver<DeviceSnapshot>,
    repaint: Arc<dyn Repaint>,
}

impl DevicesVm {
    pub fn new(port: u16, cert: String, cfg: Shared, repaint: Arc<dyn Repaint>) -> Self {
        let (tx, rx) = channel();
        Self {
            ios_simulators: vec![],
            android: vec![],
            system_proxy_enabled: false,
            global_enabled: false,
            pac_enabled: false,
            applied: None,
            cfg,
            proxy_ours: false,
            port,
            cert,
            next_poll: Instant::now(),
            polling: false,
            proxy_busy: Arc::default(),
            proxy_gen: Arc::default(),
            tx,
            rx,
            repaint,
        }
    }

    pub fn tick(&mut self, status: &Status) {
        while let Ok(snapshot) = self.rx.try_recv() {
            self.apply(snapshot);
        }
        if let Some(route) = self.route_change() {
            let port = self.port;
            self.spawn_proxy_job(status, move || devices::apply_route(&route, port));
        }
        if self.polling || Instant::now() < self.next_poll {
            return;
        }
        self.polling = true;
        let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
        let (cert, port) = (self.cert.clone(), self.port);
        let (busy, generation) = (self.proxy_busy.clone(), self.proxy_gen.clone());
        std::thread::spawn(move || {
            let before = generation.load(Ordering::SeqCst);
            let mut snapshot = devices::snapshot(&cert, port);
            let settled =
                busy.load(Ordering::SeqCst) == 0 && generation.load(Ordering::SeqCst) == before;
            if !settled {
                snapshot.system_proxy_ours = None;
                snapshot.pac_ours = None;
            }
            let _ = tx.send(snapshot);
            repaint.request();
        });
    }

    fn spawn_proxy_job(
        &self,
        status: &Status,
        job: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) {
        let (busy, generation) = (self.proxy_busy.clone(), self.proxy_gen.clone());
        busy.fetch_add(1, Ordering::SeqCst);
        generation.fetch_add(1, Ordering::SeqCst);
        status.jobs.spawn(move || {
            let result = job();
            generation.fetch_add(1, Ordering::SeqCst);
            busy.fetch_sub(1, Ordering::SeqCst);
            result
        });
    }

    fn apply(&mut self, snapshot: DeviceSnapshot) {
        self.polling = false;
        self.next_poll = Instant::now() + POLL_EVERY;
        self.ios_simulators = snapshot.ios_simulators;
        self.android = snapshot.android;
        if let Some(ours) = snapshot.system_proxy_ours {
            self.global_enabled = ours;
        }
        if let Some(ours) = snapshot.pac_ours {
            self.pac_enabled = ours;
        }
        self.system_proxy_enabled = self.global_enabled || self.pac_enabled;
    }

    fn desired_route(&self) -> Route {
        self.cfg.lock().unwrap().proxy_route()
    }

    /// The route to apply now because the config changed while the proxy is on.
    fn route_change(&mut self) -> Option<Route> {
        let want = self.desired_route();
        match &self.applied {
            Some(have) if *have != want => {
                self.applied = Some(want.clone());
                Some(want)
            }
            _ => None,
        }
    }

    /// Host count when only some hosts go through the proxy.
    pub fn hosts_only(&self) -> Option<usize> {
        if !self.system_proxy_enabled {
            return None;
        }
        match &self.applied {
            Some(Route::Hosts(h)) => Some(h.len()),
            Some(Route::All) => None,
            None => self.pac_enabled.then_some(0),
        }
    }

    /// Routes feeding the proxy, for the toolbar chip. None when nothing is routed.
    pub fn sources_label(&self) -> Option<String> {
        let mut parts = vec![];
        if self.system_proxy_enabled {
            parts.push(match self.hosts_only() {
                Some(1) => "System · 1 host".to_string(),
                Some(n) => format!("System · {n} hosts"),
                None => "System".to_string(),
            });
        }
        match self.android.iter().filter(|d| d.proxied).count() {
            0 => {}
            n => parts.push(format!("{n} Android")),
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    pub fn proxy_only_setting(&self) -> (bool, usize) {
        let c = self.cfg.lock().unwrap();
        (c.proxy_only, c.proxy_only_hosts.len())
    }

    pub fn running(&self) -> impl Iterator<Item = &IosSimulator> {
        self.ios_simulators
            .iter()
            .filter(|sim| sim.state == IosSimulatorState::Booted)
    }

    pub fn missing_ca(&self) -> Vec<&IosSimulator> {
        self.running()
            .filter(|sim| sim.ca_status != IosCaStatus::Installed)
            .collect()
    }

    /// The chosen running simulator, or the first running one.
    pub fn selected_sim(&self, udid: Option<&str>) -> Option<&IosSimulator> {
        self.running()
            .find(|sim| Some(sim.udid.as_str()) == udid)
            .or_else(|| self.running().next())
    }

    pub fn run(&mut self, action: DeviceAction, status: &Status) {
        let jobs = &status.jobs;
        let (port, cert) = (self.port, self.cert.clone());
        match action {
            DeviceAction::TrustCa => jobs.spawn(move || devices::trust_ca_on_host(&cert)),
            DeviceAction::ToggleSystemProxy => {
                let enable = !self.system_proxy_enabled;
                // optimistic, so the label flips now; the next poll confirms it
                self.system_proxy_enabled = enable;
                self.proxy_ours = enable;
                if enable {
                    let route = self.desired_route();
                    self.applied = Some(route.clone());
                    self.spawn_proxy_job(status, move || devices::apply_route(&route, port));
                } else {
                    self.applied = None;
                    self.spawn_proxy_job(status, move || sysproxy::system_proxy(false, port));
                }
                self.next_poll = Instant::now() + POLL_EVERY;
                return;
            }
            DeviceAction::ToggleProxyOnly => {
                let mut c = self.cfg.lock().unwrap();
                c.proxy_only = !c.proxy_only;
                // `tick` re-applies when the system proxy is already on
                return;
            }
            DeviceAction::LaunchChrome => jobs.spawn(move || devices::launch_chrome(port)),
            DeviceAction::IosSimInstallCa(udid) => {
                jobs.spawn(move || devices::ios_sim_install_ca(&udid, &cert));
            }
            DeviceAction::InstallMissingIosCas => {
                let udids: Vec<String> = self.missing_ca().iter().map(|s| s.udid.clone()).collect();
                jobs.spawn(move || {
                    for udid in &udids {
                        devices::ios_sim_install_ca(udid, &cert)?;
                    }
                    Ok(format!("CA installed on {} simulator(s)", udids.len()))
                });
            }
            DeviceAction::IosSimReboot(udid) => jobs.spawn(move || devices::ios_sim_reboot(&udid)),
            DeviceAction::AndroidSetProxy => jobs.spawn(move || devices::android_set_proxy(port)),
            DeviceAction::AndroidToggleProxy(serial) => {
                let enable = !self.android.iter().any(|d| d.serial == serial && d.proxied);
                if let Some(d) = self.android.iter_mut().find(|d| d.serial == serial) {
                    // optimistic, so the checkbox flips now; the next poll confirms it
                    d.proxied = enable;
                }
                jobs.spawn(move || devices::android_toggle_proxy(&serial, port, enable));
            }
            DeviceAction::AndroidInstallCa => {
                jobs.spawn(move || devices::android_install_ca(&cert));
            }
        }
        self.next_poll = self.next_poll.min(Instant::now() + POLL_AFTER_ACTION);
    }

    /// Turns the system proxy off only if it is ours, never a proxy the user set up.
    pub fn shutdown(&self) {
        let ours = self.proxy_ours
            || sysproxy::system_proxy_status(self.port).unwrap_or(false)
            || sysproxy::pac_status().unwrap_or(false);
        if ours && let Err(e) = sysproxy::system_proxy(false, self.port) {
            eprintln!("failed to clear system proxy: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::viewmodels::status::NoRepaint;
    use std::sync::Mutex;

    fn sim(udid: &str, state: IosSimulatorState, ca: IosCaStatus) -> IosSimulator {
        IosSimulator {
            udid: udid.into(),
            name: udid.into(),
            runtime: "iOS-27-0".into(),
            state,
            ca_status: ca,
            data_path: String::new(),
        }
    }

    fn vm_with(sims: Vec<IosSimulator>) -> DevicesVm {
        let mut vm = DevicesVm::new(
            9090,
            String::new(),
            Arc::new(Mutex::new(Config::default())),
            Arc::new(NoRepaint),
        );
        vm.apply(DeviceSnapshot {
            ios_simulators: sims,
            android: vec![],
            system_proxy_ours: Some(false),
            pac_ours: Some(false),
        });
        vm
    }

    #[test]
    fn selection_falls_back_to_first_running_sim() {
        use IosSimulatorState::*;
        let vm = vm_with(vec![
            sim("off", Shutdown, IosCaStatus::Unknown),
            sim("a", Booted, IosCaStatus::Installed),
            sim("b", Booted, IosCaStatus::Missing),
        ]);
        assert_eq!(vm.selected_sim(None).unwrap().udid, "a");
        assert_eq!(vm.selected_sim(Some("b")).unwrap().udid, "b");
        assert_eq!(vm.selected_sim(Some("off")).unwrap().udid, "a");
        let missing: Vec<_> = vm.missing_ca().iter().map(|s| s.udid.as_str()).collect();
        assert_eq!(missing, ["b"]);
    }

    #[test]
    fn unknown_proxy_state_keeps_the_last_value() {
        let mut vm = vm_with(vec![]);
        vm.global_enabled = true;
        vm.apply(DeviceSnapshot {
            ios_simulators: vec![],
            android: vec![],
            system_proxy_ours: None,
            pac_ours: None,
        });
        assert!(vm.system_proxy_enabled);
    }

    fn set_hosts(vm: &DevicesVm, on: bool, hosts: &[&str]) {
        let mut c = vm.cfg.lock().unwrap();
        c.proxy_only = on;
        c.proxy_only_hosts = hosts.iter().map(ToString::to_string).collect();
    }

    #[test]
    fn list_edit_while_enabled_triggers_one_reapply() {
        let mut vm = vm_with(vec![]);
        set_hosts(&vm, true, &["a.com"]);
        assert_eq!(vm.route_change(), None, "nothing applied yet");
        vm.applied = Some(vm.desired_route());
        assert_eq!(vm.route_change(), None);
        set_hosts(&vm, true, &["a.com", "b.com"]);
        assert_eq!(
            vm.route_change(),
            Some(Route::Hosts(vec!["a.com".into(), "b.com".into()]))
        );
        assert_eq!(vm.route_change(), None);
        set_hosts(&vm, false, &["a.com", "b.com"]);
        assert_eq!(vm.route_change(), Some(Route::All));
    }

    #[test]
    fn hosts_only_reports_the_applied_route_while_enabled() {
        let mut vm = vm_with(vec![]);
        vm.applied = Some(Route::Hosts(vec!["a.com".into(), "b.com".into()]));
        assert_eq!(vm.hosts_only(), None, "proxy is off");
        vm.system_proxy_enabled = true;
        assert_eq!(vm.hosts_only(), Some(2));
        vm.applied = Some(Route::All);
        assert_eq!(vm.hosts_only(), None);
    }

    #[test]
    fn sources_label_names_active_routes() {
        let mut vm = vm_with(vec![]);
        assert_eq!(vm.sources_label(), None);
        vm.system_proxy_enabled = true;
        vm.applied = Some(Route::All);
        assert_eq!(vm.sources_label().as_deref(), Some("System"));
        vm.applied = Some(Route::Hosts(vec!["a.com".into()]));
        assert_eq!(vm.sources_label().as_deref(), Some("System · 1 host"));
        vm.applied = Some(Route::Hosts(vec!["a.com".into(), "b.com".into()]));
        assert_eq!(vm.sources_label().as_deref(), Some("System · 2 hosts"));
        vm.android = vec![AndroidDevice {
            serial: "emulator-5554".into(),
            name: "Pixel".into(),
            proxied: true,
        }];
        assert_eq!(
            vm.sources_label().as_deref(),
            Some("System · 2 hosts · 1 Android")
        );
        vm.system_proxy_enabled = false;
        assert_eq!(vm.sources_label().as_deref(), Some("1 Android"));
    }

    #[test]
    fn pac_poll_counts_as_system_proxy_on() {
        let mut vm = vm_with(vec![]);
        vm.apply(DeviceSnapshot {
            ios_simulators: vec![],
            android: vec![],
            system_proxy_ours: Some(false),
            pac_ours: Some(true),
        });
        assert!(vm.system_proxy_enabled);
    }
}
