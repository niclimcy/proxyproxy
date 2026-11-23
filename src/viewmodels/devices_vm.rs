use super::intent::DeviceAction;
use super::status::Status;
use crate::domain::repaint::Repaint;
use crate::services::devices::{self, DeviceSnapshot};
pub use crate::services::devices::{IosCaStatus, IosSimulator, IosSimulatorState};
use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};
use std::time::{Duration, Instant};

const POLL_EVERY: Duration = Duration::from_secs(4);
const POLL_AFTER_ACTION: Duration = Duration::from_secs(1);

/// Simulator and system-proxy state, polled off the UI thread because each poll runs
/// several subprocesses.
pub struct DevicesVm {
    pub ios_simulators: Vec<IosSimulator>,
    pub system_proxy_enabled: bool,
    port: u16,
    cert: String,
    next_poll: Instant,
    polling: bool,
    tx: Sender<DeviceSnapshot>,
    rx: Receiver<DeviceSnapshot>,
    repaint: Arc<dyn Repaint>,
}

impl DevicesVm {
    pub fn new(port: u16, cert: String, repaint: Arc<dyn Repaint>) -> Self {
        let (tx, rx) = channel();
        Self {
            ios_simulators: vec![],
            system_proxy_enabled: false,
            port,
            cert,
            next_poll: Instant::now(),
            polling: false,
            tx,
            rx,
            repaint,
        }
    }

    pub fn tick(&mut self) {
        while let Ok(snapshot) = self.rx.try_recv() {
            self.apply(snapshot);
        }
        if self.polling || Instant::now() < self.next_poll {
            return;
        }
        self.polling = true;
        let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
        let cert = self.cert.clone();
        std::thread::spawn(move || {
            let _ = tx.send(devices::snapshot(&cert));
            repaint.request();
        });
    }

    fn apply(&mut self, snapshot: DeviceSnapshot) {
        self.polling = false;
        self.next_poll = Instant::now() + POLL_EVERY;
        self.ios_simulators = snapshot.ios_simulators;
        if let Some(enabled) = snapshot.system_proxy {
            self.system_proxy_enabled = enabled;
        }
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
                jobs.spawn(move || devices::system_proxy(enable, port));
                self.next_poll = Instant::now() + POLL_EVERY;
                return;
            }
            DeviceAction::LaunchChrome => jobs.spawn(move || devices::launch_chrome(port)),
            DeviceAction::IosSimInstallCa(udid) => {
                jobs.spawn(move || devices::ios_sim_install_ca(&udid, &cert))
            }
            DeviceAction::InstallMissingIosCas => {
                let udids: Vec<String> = self.missing_ca().iter().map(|s| s.udid.clone()).collect();
                jobs.spawn(move || {
                    for udid in &udids {
                        devices::ios_sim_install_ca(udid, &cert)?;
                    }
                    Ok(format!("CA installed on {} simulator(s)", udids.len()))
                })
            }
            DeviceAction::IosSimReboot(udid) => jobs.spawn(move || devices::ios_sim_reboot(&udid)),
            DeviceAction::AndroidSetProxy => jobs.spawn(move || devices::android_set_proxy(port)),
            DeviceAction::AndroidClearProxy => {
                jobs.spawn(move || devices::android_clear_proxy(port))
            }
            DeviceAction::AndroidInstallCa => {
                jobs.spawn(move || devices::android_install_ca(&cert))
            }
        }
        self.next_poll = self.next_poll.min(Instant::now() + POLL_AFTER_ACTION);
    }

    pub fn shutdown(&self) {
        if let Err(e) = devices::system_proxy(false, self.port) {
            eprintln!("failed to clear system proxy: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewmodels::status::NoRepaint;

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
        let mut vm = DevicesVm::new(9090, String::new(), Arc::new(NoRepaint));
        vm.apply(DeviceSnapshot {
            ios_simulators: sims,
            system_proxy: Some(false),
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
        vm.system_proxy_enabled = true;
        vm.apply(DeviceSnapshot {
            ios_simulators: vec![],
            system_proxy: None,
        });
        assert!(vm.system_proxy_enabled);
    }
}
