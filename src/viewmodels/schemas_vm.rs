use super::intent::SchemasIntent;
use crate::domain::config::Shared;
use crate::services::schemas;

pub struct SchemasVm {
    pub cfg: Shared,
}

pub struct SchemasView {
    pub paths: Vec<String>,
    pub errors: Vec<(String, String)>,
    pub stats: (usize, usize),
}

impl SchemasVm {
    pub fn new(cfg: Shared) -> Self {
        Self { cfg }
    }

    pub fn sync(&self) {
        let paths = self.cfg.lock().unwrap().proto_descs.clone();
        schemas::sync(&paths);
    }

    pub fn view(&self) -> SchemasView {
        SchemasView { paths: self.cfg.lock().unwrap().proto_descs.clone(), errors: schemas::errors(), stats: schemas::stats() }
    }

    pub fn handle(&mut self, intent: SchemasIntent) {
        match intent {
            SchemasIntent::AddFiles(files) => {
                let mut cfg = self.cfg.lock().unwrap();
                for p in files {
                    if !cfg.proto_descs.contains(&p) {
                        cfg.proto_descs.push(p);
                    }
                }
            }
            SchemasIntent::Remove(i) => {
                let mut cfg = self.cfg.lock().unwrap();
                if i < cfg.proto_descs.len() {
                    cfg.proto_descs.remove(i);
                }
            }
            SchemasIntent::Reload => schemas::reload(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use std::sync::{Arc, Mutex};

    #[test]
    fn add_dedupes_and_remove_is_bounds_checked() {
        let mut vm = SchemasVm::new(Arc::new(Mutex::new(Config::default())));
        vm.handle(SchemasIntent::AddFiles(vec!["/a.desc".into(), "/b.desc".into(), "/a.desc".into()]));
        vm.handle(SchemasIntent::AddFiles(vec!["/b.desc".into()]));
        assert_eq!(vm.view().paths, vec!["/a.desc".to_string(), "/b.desc".to_string()]);
        vm.handle(SchemasIntent::Remove(5));
        vm.handle(SchemasIntent::Remove(0));
        assert_eq!(vm.cfg.lock().unwrap().proto_descs, vec!["/b.desc".to_string()]);
    }
}
