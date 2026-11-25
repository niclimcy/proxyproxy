use super::intent::KeymapIntent;
use crate::domain::config::Shared;
use crate::domain::keymap::{BindError, Binding, CUSTOMIZABLE, RowAction};

/// The Shortcuts tab of Settings.
pub struct KeymapVm {
    pub cfg: Shared,
    /// The action waiting for its new keys.
    pub recording: Option<RowAction>,
    /// The last rejected change, shown next to its action.
    pub error: Option<(RowAction, BindError)>,
}

pub struct KeymapRow {
    pub action: RowAction,
    pub binding: Option<Binding>,
    pub custom: bool,
}

impl KeymapVm {
    pub fn new(cfg: Shared) -> Self {
        Self { cfg, recording: None, error: None }
    }

    pub fn rows(&self) -> Vec<KeymapRow> {
        let c = self.cfg.lock().unwrap();
        CUSTOMIZABLE
            .iter()
            .map(|&action| KeymapRow { action, binding: c.binding(action), custom: c.is_custom(action) })
            .collect()
    }

    pub fn handle(&mut self, intent: KeymapIntent) {
        let (action, result) = match intent {
            KeymapIntent::Record(a) => {
                self.recording = Some(a);
                self.error = None;
                return;
            }
            KeymapIntent::Cancel => {
                self.recording = None;
                self.error = None;
                return;
            }
            KeymapIntent::ResetAll => {
                self.cfg.lock().unwrap().shortcuts.clear();
                (None, Ok(()))
            }
            KeymapIntent::Set(a, b) => (Some(a), self.cfg.lock().unwrap().set_binding(a, Some(b))),
            KeymapIntent::Clear(a) => (Some(a), self.cfg.lock().unwrap().set_binding(a, None)),
            KeymapIntent::Reset(a) => (Some(a), self.cfg.lock().unwrap().reset_binding(a)),
        };
        match (action, result) {
            // recording continues, so another combo can be tried straight away
            (Some(a), Err(e)) => self.error = Some((a, e)),
            _ => {
                self.recording = None;
                self.error = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use std::sync::{Arc, Mutex};

    fn vm() -> KeymapVm {
        KeymapVm::new(Arc::new(Mutex::new(Config::default())))
    }

    fn combo(key: &str, shift: bool) -> Binding {
        Binding { key: key.into(), shift }
    }

    #[test]
    fn recording_a_free_combo_saves_it_and_stops() {
        let mut vm = vm();
        vm.handle(KeymapIntent::Record(RowAction::Pin));
        assert_eq!(vm.recording, Some(RowAction::Pin));
        vm.handle(KeymapIntent::Set(RowAction::Pin, combo("K", false)));
        assert_eq!(vm.recording, None);
        assert_eq!(vm.cfg.lock().unwrap().binding(RowAction::Pin), Some(combo("K", false)));
        let pin = vm.rows().into_iter().find(|r| r.action == RowAction::Pin).unwrap();
        assert!(pin.custom);
    }

    #[test]
    fn a_taken_combo_is_rejected_and_recording_continues() {
        let mut vm = vm();
        vm.handle(KeymapIntent::Record(RowAction::Pin));
        vm.handle(KeymapIntent::Set(RowAction::Pin, combo("D", false)));
        assert_eq!(vm.error, Some((RowAction::Pin, BindError::InUse(RowAction::DiffBase))));
        assert_eq!(vm.recording, Some(RowAction::Pin));
        assert_eq!(vm.cfg.lock().unwrap().binding(RowAction::Pin), Some(combo("P", false)));
        vm.handle(KeymapIntent::Cancel);
        assert_eq!((vm.recording, vm.error), (None, None));
    }

    #[test]
    fn clear_reset_and_reset_all() {
        let mut vm = vm();
        vm.handle(KeymapIntent::Clear(RowAction::Mute));
        assert_eq!(vm.cfg.lock().unwrap().binding(RowAction::Mute), None);
        vm.handle(KeymapIntent::Reset(RowAction::Mute));
        assert_eq!(vm.cfg.lock().unwrap().binding(RowAction::Mute), Some(combo("M", true)));
        vm.handle(KeymapIntent::Set(RowAction::Pin, combo("K", false)));
        vm.handle(KeymapIntent::Clear(RowAction::Replay));
        vm.handle(KeymapIntent::ResetAll);
        assert!(vm.cfg.lock().unwrap().shortcuts.is_empty());
        assert!(vm.rows().iter().all(|r| !r.custom && r.binding.is_some()));
    }
}
