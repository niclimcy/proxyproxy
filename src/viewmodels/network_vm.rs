use super::intent::NetworkIntent;
use super::status::Status;
use crate::domain::config::Shared;
use crate::proxy::tls;

pub struct NetworkVm {
    pub cfg: Shared,
}

impl NetworkVm {
    pub fn new(cfg: Shared) -> Self {
        Self { cfg }
    }

    // text fields bind straight to the config
    pub fn config(&self) -> std::sync::MutexGuard<'_, crate::domain::config::Config> {
        self.cfg.lock().unwrap()
    }

    pub fn handle(&mut self, intent: NetworkIntent, status: &mut Status) {
        match intent {
            NetworkIntent::AddDns => self
                .cfg
                .lock()
                .unwrap()
                .dns_overrides
                .push((String::new(), String::new())),
            NetworkIntent::RemoveDns(i) => {
                let mut c = self.cfg.lock().unwrap();
                if i < c.dns_overrides.len() {
                    c.dns_overrides.remove(i);
                }
            }
            NetworkIntent::SetKeylog(p) => self.cfg.lock().unwrap().keylog_path = p,
            NetworkIntent::SetPac(p) => self.cfg.lock().unwrap().upstream.pac_path = p,
            NetworkIntent::ImportCa(cert, key) => {
                status.set(match tls::import_custom_files(&tls::dir(), &cert, &key) {
                    Ok(()) => "Custom CA imported. Restart proxyproxy to apply.".to_string(),
                    Err(e) => format!("CA import failed: {e}"),
                });
            }
        }
    }
}

pub fn valid_ip(s: &str) -> bool {
    s.trim().parse::<std::net::IpAddr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::viewmodels::status::NoRepaint;
    use std::sync::{Arc, Mutex};

    fn setup() -> (NetworkVm, Status) {
        (
            NetworkVm::new(Arc::new(Mutex::new(Config::default()))),
            Status::new(String::new(), Arc::new(NoRepaint)),
        )
    }

    #[test]
    fn dns_rows_add_and_remove() {
        let (mut vm, mut st) = setup();
        vm.handle(NetworkIntent::AddDns, &mut st);
        vm.handle(NetworkIntent::AddDns, &mut st);
        vm.cfg.lock().unwrap().dns_overrides[0] = ("a.com".into(), "127.0.0.1".into());
        vm.handle(NetworkIntent::RemoveDns(1), &mut st);
        vm.handle(NetworkIntent::RemoveDns(9), &mut st);
        assert_eq!(vm.cfg.lock().unwrap().dns_overrides.len(), 1);
    }

    #[test]
    fn paths_are_stored() {
        let (mut vm, mut st) = setup();
        vm.handle(NetworkIntent::SetKeylog("/tmp/k".into()), &mut st);
        vm.handle(NetworkIntent::SetPac("/tmp/p.pac".into()), &mut st);
        let c = vm.cfg.lock().unwrap();
        assert_eq!(
            (c.keylog_path.as_str(), c.upstream.pac_path.as_str()),
            ("/tmp/k", "/tmp/p.pac")
        );
    }

    #[test]
    fn bad_ca_import_reports_failure_in_status() {
        let (mut vm, mut st) = setup();
        vm.handle(
            NetworkIntent::ImportCa("/nonexistent/c.pem".into(), "/nonexistent/k.pem".into()),
            &mut st,
        );
        assert!(st.text.starts_with("CA import failed"), "{}", st.text);
    }

    #[test]
    fn ip_validation() {
        assert!(valid_ip(" 127.0.0.1 ") && valid_ip("::1"));
        assert!(!valid_ip("localhost"));
    }
}
