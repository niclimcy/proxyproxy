use super::intent::MapSwitchChoice;
use super::rules_vm::RulesVm;
use crate::domain::mime;
use crate::domain::rule::MapStorage;
use crate::services::mocks::{self, Source};
use std::path::PathBuf;
use std::time::SystemTime;

const EDIT_LIMIT: u64 = 1 << 20;

/// Bytes waiting to be written when the rule is saved, after a storage switch.
pub struct Staged {
    pub dest: Source,
    pub bytes: Vec<u8>,
    /// What the destination held when this was staged (`None`: nothing). Saving checks it again.
    pub seen: Option<Vec<u8>>,
}

/// A switch waiting for the user, because the destination already holds different content.
pub struct Prompt {
    pub to: MapStorage,
    pub dest: Source,
    pub carried: Vec<u8>,
    pub theirs: Vec<u8>,
    /// "Your file: 2.1 KB, modified 3 min ago · Stored copy: 1.8 KB"
    pub summary: String,
    pub replace: String,
    pub keep: String,
}

/// What the saved rule serves, for the "Serving now" line.
pub struct Live {
    pub storage: MapStorage,
    pub label: String,
}

#[derive(Default)]
pub struct MapLocalForm {
    /// The staged mode; the saved rule keeps serving `live` until Save rule.
    pub storage: MapStorage,
    pub file: String,
    pub managed: String,
    /// What the user calls the stored file; only its extension affects serving.
    pub name: String,
    pub content: String,
    pub dirty: bool,
    /// The disk path (disk mode) or managed name (stored mode) the editor was loaded from.
    pub loaded: String,
    pub staged: Option<Staged>,
    pub prompt: Option<Prompt>,
    pub live: Option<Live>,
    pub status: Option<String>,
    pub error: Option<String>,
    /// Why the editor is empty (binary or large content). Not an error, so never green or red.
    pub limit: Option<String>,
}

pub fn human(bytes: usize) -> String {
    let n = bytes as f64;
    if n < 1024.0 {
        format!("{bytes} B")
    } else if n < 1024.0 * 1024.0 {
        format!("{:.1} KB", n / 1024.0)
    } else {
        format!("{:.1} MB", n / 1024.0 / 1024.0)
    }
}

fn ago(t: SystemTime) -> String {
    let s = SystemTime::now()
        .duration_since(t)
        .map_or(0, |d| d.as_secs());
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86400),
    }
}

fn side(label: &str, len: usize, modified: Option<SystemTime>) -> String {
    match modified {
        Some(t) => format!("{label}: {}, modified {}", human(len), ago(t)),
        None => format!("{label}: {}", human(len)),
    }
}

impl MapLocalForm {
    pub fn source(&self) -> Option<Source> {
        match self.storage {
            MapStorage::Disk => {
                (!self.file.trim().is_empty()).then(|| Source::Disk(self.file.trim().into()))
            }
            MapStorage::AppData => {
                (!self.managed.is_empty()).then(|| Source::Stored(self.managed.clone()))
            }
        }
    }

    /// The editor shows the loaded content only while the source still is what it was loaded from.
    pub fn editable(&self) -> bool {
        !self.loaded.is_empty()
            && self.loaded
                == match self.storage {
                    MapStorage::Disk => self.file.trim(),
                    MapStorage::AppData => &self.managed,
                }
    }

    /// Lower-case extension of the mapped file; also the highlighter's language name.
    pub fn ext(&self) -> String {
        let name = match self.storage {
            MapStorage::Disk => self.file.trim(),
            MapStorage::AppData => self.name.trim(),
        };
        std::path::Path::new(name)
            .extension()
            .map_or_default(|e| e.to_string_lossy().to_lowercase())
    }

    /// Something the saved rule doesn't have yet.
    pub fn pending(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|l| l.storage != self.storage)
            || self.staged.is_some()
            || (self.storage == MapStorage::AppData && self.dirty)
    }

    pub fn served_as(&self) -> &'static str {
        mime::content_type(&mocks::clean_name(&self.name))
    }

    pub fn name_note(&self) -> Option<String> {
        let clean = mocks::clean_name(&self.name);
        if clean != self.name.trim() {
            Some(format!("Will be saved as {clean}"))
        } else if self.served_as() == "application/octet-stream" {
            Some("Unknown extension: served as application/octet-stream".into())
        } else {
            None
        }
    }

    pub fn stored_path(&self) -> Option<String> {
        mocks::path(&self.managed).map(|p| p.display().to_string())
    }

    pub fn note(&mut self, text: impl Into<String>) {
        self.status = Some(text.into());
        self.error = None;
    }

    pub fn fail(&mut self, text: impl Into<String>) {
        self.status = None;
        self.error = Some(text.into());
    }

    /// Puts content in the editor, or says why it can't be edited.
    fn show(&mut self, bytes: Vec<u8>, key: &str) {
        self.dirty = false;
        let size = bytes.len() as u64;
        match String::from_utf8(bytes) {
            Ok(text) if text.len() as u64 <= EDIT_LIMIT => {
                self.content = text;
                self.loaded = key.to_string();
                self.limit = None;
            }
            _ => {
                self.content.clear();
                self.loaded.clear();
                self.limit = Some(too_big(size));
            }
        }
    }

    /// Unsaved editor text, else what a pending switch holds, else the current source's bytes.
    fn current_bytes(&self) -> Result<Vec<u8>, String> {
        if self.dirty {
            return Ok(self.content.clone().into_bytes());
        }
        if let Some(s) = &self.staged {
            return Ok(s.bytes.clone());
        }
        let Some(src) = self.source() else {
            return Ok(vec![]);
        };
        src.read().map_err(|e| match self.storage {
            MapStorage::Disk => format!("Cannot read {}: {e}", src.label()),
            MapStorage::AppData => format!("Cannot read the stored copy: {e}"),
        })
    }

    pub fn make_prompt(
        &self,
        to: MapStorage,
        dest: Source,
        carried: Vec<u8>,
        theirs: Vec<u8>,
    ) -> Prompt {
        let theirs_side = dest.metadata().and_then(|m| m.modified().ok());
        let (theirs_label, mine) = match (to, self.dirty) {
            (MapStorage::Disk, false) => ("Your file", "the stored copy"),
            (MapStorage::Disk, true) => ("Your file", "my edits"),
            (MapStorage::AppData, false) => ("Stored copy", "your file"),
            (MapStorage::AppData, true) => ("Stored copy", "my edits"),
        };
        let mine_label = match (to, self.dirty) {
            (MapStorage::Disk, false) => "Stored copy",
            (MapStorage::AppData, false) => "Your file",
            (_, true) => "Your edits",
        };
        let name = dest.label();
        let name = std::path::Path::new(&name)
            .file_name()
            .map_or(name.clone(), |n| n.to_string_lossy().into_owned());
        let discard = if self.dirty {
            " (discard my edits)"
        } else {
            ""
        };
        let (replace, keep) = match to {
            MapStorage::Disk => (
                format!("Overwrite {name} with {mine}"),
                format!("Keep {name} as it is{discard}"),
            ),
            MapStorage::AppData => (
                format!("Replace the stored copy with {mine}"),
                format!("Keep the stored copy as it is{discard}"),
            ),
        };
        Prompt {
            to,
            summary: format!(
                "{} · {}",
                side(theirs_label, theirs.len(), theirs_side),
                side(mine_label, carried.len(), None)
            ),
            dest,
            carried,
            theirs,
            replace,
            keep,
        }
    }
}

fn too_big(size: u64) -> String {
    let size = usize::try_from(size).unwrap_or(usize::MAX);
    format!(
        "Binary or over 1 MB ({}). Served as it is; can't be edited here.",
        human(size)
    )
}

impl RulesVm {
    pub(super) fn load_map_local_file(&mut self) {
        let ml = &mut self.form.ml;
        let Some(src) = ml.source() else {
            ml.fail("Choose a file first");
            return;
        };
        let key = match &src {
            Source::Disk(p) => p.display().to_string(),
            Source::Stored(n) => n.clone(),
        };
        // reads run on the UI thread and the editor lays out every line each frame
        if let Some(meta) = src.metadata()
            && meta.len() > EDIT_LIMIT
        {
            ml.loaded.clear();
            ml.limit = Some(too_big(meta.len()));
            ml.status = None;
            ml.error = None;
            return;
        }
        match src.read() {
            Ok(bytes) => {
                ml.show(bytes, &key);
                ml.staged = None;
                ml.note(match ml.storage {
                    MapStorage::Disk => "File loaded from disk",
                    MapStorage::AppData => "Loaded from proxyproxy",
                });
            }
            Err(e) => ml.fail(match ml.storage {
                MapStorage::Disk => format!("Cannot read {}: {e}", src.label()),
                MapStorage::AppData => format!("Cannot read the stored copy: {e}"),
            }),
        }
    }

    /// Disk mode only: this writes the user's file straight away. Stored content is saved with the rule.
    pub(super) fn save_map_local_file(&mut self) {
        let ml = &mut self.form.ml;
        if ml.storage != MapStorage::Disk || !ml.editable() {
            ml.fail("Load the file first");
            return;
        }
        match Source::Disk(PathBuf::from(&ml.loaded)).write(ml.content.as_bytes()) {
            Ok(()) => {
                ml.dirty = false;
                ml.staged = None;
                ml.note("Saved directly to disk");
            }
            Err(e) => ml.fail(format!("Cannot save the file: {e}")),
        }
    }

    pub(super) fn reveal_map_file(&mut self) {
        if let Err(e) = mocks::reveal(&self.form.ml.managed) {
            self.form.ml.fail(e);
        }
    }

    /// Stages a switch: nothing is written until the rule is saved. Both sides are only read here.
    pub(super) fn set_map_storage(&mut self, to: MapStorage, target: Option<PathBuf>) {
        let ml = &mut self.form.ml;
        if ml.prompt.take().is_some() {
            ml.note("Switch cancelled");
            return;
        }
        if ml.storage == to {
            return;
        }
        let carried = match ml.current_bytes() {
            Ok(c) => c,
            Err(e) => return ml.fail(e),
        };
        let dest = match to {
            MapStorage::AppData if ml.managed.is_empty() => {
                Source::Stored(mocks::new_name(if ml.name.trim().is_empty() {
                    &ml.file
                } else {
                    &ml.name
                }))
            }
            MapStorage::AppData => Source::Stored(ml.managed.clone()),
            MapStorage::Disk => {
                let chosen = target.or_else(|| {
                    (!ml.file.trim().is_empty()).then(|| PathBuf::from(ml.file.trim()))
                });
                match chosen {
                    Some(p) => Source::Disk(p),
                    None => return ml.fail("Choose where to save the file first"),
                }
            }
        };
        let seen = match dest.read() {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return ml.fail(format!("Cannot read {}: {e}", dest.label())),
        };
        match seen {
            Some(old) if old != carried => {
                ml.prompt = Some(ml.make_prompt(to, dest, carried, old));
            }
            seen => self.stage(to, dest, carried, seen),
        }
    }

    pub(super) fn resolve_map_switch(&mut self, choice: MapSwitchChoice) {
        match (choice, self.form.ml.prompt.take()) {
            (MapSwitchChoice::Replace, Some(p)) => {
                self.stage(p.to, p.dest, p.carried, Some(p.theirs));
            }
            (MapSwitchChoice::Adopt, Some(p)) => {
                self.stage(p.to, p.dest, p.theirs.clone(), Some(p.theirs));
            }
            // a dismissed Save As dialog arrives here without a prompt
            _ => self.form.ml.note("Switch cancelled"),
        }
    }

    pub(super) fn stage(
        &mut self,
        to: MapStorage,
        dest: Source,
        bytes: Vec<u8>,
        seen: Option<Vec<u8>>,
    ) {
        let ml = &mut self.form.ml;
        ml.storage = to;
        let key = match &dest {
            Source::Disk(p) => {
                ml.file = p.display().to_string();
                ml.file.clone()
            }
            Source::Stored(n) => {
                ml.managed.clone_from(n);
                if ml.name.trim().is_empty() {
                    ml.name = mocks::display_name(n).to_string();
                }
                n.clone()
            }
        };
        ml.show(bytes.clone(), &key);
        ml.status = None;
        ml.error = None;
        ml.staged = (seen.as_ref() != Some(&bytes)).then_some(Staged { dest, bytes, seen });
    }

    /// Deletes stored files dropped by earlier edits, except any the open form still needs.
    /// Call it with what `SessionVm` hands back after the config was saved.
    pub fn sweep_mocks(&mut self, names: &[String]) {
        if names.is_empty() {
            return;
        }
        let ml = &self.form.ml;
        let mut keep = vec![ml.managed.as_str()];
        for dest in ml
            .prompt
            .iter()
            .map(|p| &p.dest)
            .chain(ml.staged.iter().map(|s| &s.dest))
        {
            if let Source::Stored(n) = dest {
                keep.push(n);
            }
        }
        mocks::sweep(&self.cfg.lock().unwrap(), &keep, names);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::domain::rule::Action;
    use crate::services::flow_store::FlowStore;
    use crate::viewmodels::intent::RuleIntent;
    use crate::viewmodels::rules_vm::{RuleForm, RuleKind};
    use crate::viewmodels::status::{NoRepaint, Status};
    use std::sync::{Arc, Mutex};

    fn vm() -> RulesVm {
        RulesVm::new(
            Arc::new(Mutex::new(Config::default())),
            FlowStore::default(),
        )
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pxp-store-{}-{name}", std::process::id()))
    }

    fn map_vm(file: Option<&PathBuf>) -> RulesVm {
        let mut vm = vm();
        vm.form.kind = RuleKind::MapLocal;
        vm.form.pattern = "a.com/x".into();
        if let Some(f) = file {
            vm.form.ml.file = f.display().to_string();
        }
        vm
    }

    fn to_stored(vm: &mut RulesVm) {
        vm.handle(RuleIntent::SetMapStorage(MapStorage::AppData, None));
    }

    fn to_disk(vm: &mut RulesVm, target: Option<&PathBuf>) {
        vm.handle(RuleIntent::SetMapStorage(MapStorage::Disk, target.cloned()));
    }

    fn choose(vm: &mut RulesVm, c: MapSwitchChoice) {
        vm.handle(RuleIntent::ResolveMapSwitch(c));
    }

    fn type_text(vm: &mut RulesVm, text: &str) {
        vm.form.ml.content = text.into();
        vm.form.ml.dirty = true;
    }

    fn stored(name: &str) -> std::io::Result<Vec<u8>> {
        Source::Stored(name.into()).read()
    }

    /// What the app does after the config was saved.
    fn settle(vm: &mut RulesVm) {
        let gone = std::mem::take(&mut vm.cfg.lock().unwrap().doomed_mocks);
        vm.sweep_mocks(&gone);
    }

    fn saved_action(vm: &RulesVm, id: u64) -> Action {
        let cfg = vm.cfg.lock().unwrap();
        cfg.rules
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .action
            .clone()
    }

    fn managed_of(vm: &RulesVm, id: u64) -> String {
        saved_action(vm, id).managed_file().unwrap().to_string()
    }

    /// A saved rule that serves a stored file with this content.
    fn add_stored_rule(vm: &mut RulesVm, name: &str, content: &str) -> u64 {
        vm.form = RuleForm {
            kind: RuleKind::MapLocal,
            pattern: "a.com/x".into(),
            ..RuleForm::default()
        };
        to_stored(vm);
        vm.form.ml.name = name.into();
        type_text(vm, content);
        vm.handle(RuleIntent::Submit);
        vm.cfg.lock().unwrap().rules.last().unwrap().id
    }

    #[test]
    fn rules_saved_before_storage_modes_load_as_disk() {
        let old = r#"{"rules":[{"id":1,"enabled":true,"pattern":"a.com","method":"",
            "action":{"MapLocal":{"path":"/a.json","status":201}}}]}"#;
        let cfg: Config = serde_json::from_str(old).unwrap();
        let action = cfg.rules[0].action.clone();
        assert_eq!(
            action,
            Action::MapLocal {
                path: "/a.json".into(),
                status: 201,
                storage: MapStorage::Disk,
                managed: String::new(),
                embedded: None
            }
        );
        let saved = serde_json::to_string(&action).unwrap();
        assert!(
            !saved.contains("managed") && !saved.contains("embedded"),
            "{saved}"
        );
    }

    #[test]
    fn switching_only_stages_and_saving_the_rule_writes() {
        let disk = tmp("stage.json");
        std::fs::write(&disk, "on disk").unwrap();
        let mut vm = map_vm(Some(&disk));
        type_text(&mut vm, "edited");
        to_stored(&mut vm);
        let name = vm.form.ml.managed.clone();
        assert_eq!(vm.form.ml.storage, MapStorage::AppData);
        assert!(vm.form.ml.pending() && stored(&name).is_err());
        assert!(vm.cfg.lock().unwrap().rules.is_empty());

        vm.handle(RuleIntent::Submit);
        assert_eq!(stored(&name).unwrap(), b"edited");
        assert_eq!(std::fs::read(&disk).unwrap(), b"on disk");
        assert!(name.ends_with("-stage.json"));
        let cfg = vm.cfg.lock().unwrap();
        assert!(matches!(
            &cfg.rules[0].action,
            Action::MapLocal { storage: MapStorage::AppData, path, .. } if *path == disk.display().to_string()
        ));
        drop(cfg);
        mocks::delete(&name);
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn binary_disk_bytes_are_carried_exactly() {
        let disk = tmp("img.bin");
        let bytes = vec![0u8, 159, 146, 150, 255, 10];
        std::fs::write(&disk, &bytes).unwrap();
        let mut vm = map_vm(Some(&disk));
        to_stored(&mut vm);
        assert!(!vm.form.ml.editable() && vm.form.ml.limit.is_some());
        assert!(vm.form.ml.error.is_none());
        vm.handle(RuleIntent::Submit);
        let name = vm.cfg.lock().unwrap().rules[0]
            .action
            .managed_file()
            .unwrap()
            .to_string();
        assert_eq!(stored(&name).unwrap(), bytes);
        mocks::delete(&name);
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn cancelling_after_a_switch_changes_nothing() {
        let disk = tmp("cancel.txt");
        std::fs::write(&disk, "D").unwrap();
        let mut vm = map_vm(Some(&disk));
        to_stored(&mut vm);
        let name = vm.form.ml.managed.clone();
        vm.handle(RuleIntent::CancelEdit);
        assert!(stored(&name).is_err() && vm.cfg.lock().unwrap().rules.is_empty());

        vm.handle(RuleIntent::Submit);
        vm.form.ml.file = disk.display().to_string();
        vm.form.kind = RuleKind::MapLocal;
        vm.form.pattern = "a.com".into();
        vm.handle(RuleIntent::Submit);
        let id = vm.cfg.lock().unwrap().rules[0].id;
        vm.handle(RuleIntent::Edit(id));
        to_stored(&mut vm);
        vm.handle(RuleIntent::CancelEdit);
        assert!(matches!(
            saved_action(&vm, id),
            Action::MapLocal {
                storage: MapStorage::Disk,
                ..
            }
        ));
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn stored_to_disk_waits_for_the_rule_to_be_saved() {
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "r.json", "{\"a\":1}");
        let old = managed_of(&vm, id);
        let target = tmp("free.json");
        std::fs::remove_file(&target).ok();

        vm.handle(RuleIntent::Edit(id));
        assert_eq!(vm.form.ml.content, "{\"a\":1}");
        to_disk(&mut vm, Some(&target));
        assert!(!target.exists() && vm.form.ml.pending());
        assert!(
            vm.form
                .ml
                .live
                .as_ref()
                .is_some_and(|l| l.storage == MapStorage::AppData)
        );

        vm.handle(RuleIntent::Submit);
        assert_eq!(std::fs::read(&target).unwrap(), b"{\"a\":1}");
        assert!(matches!(
            saved_action(&vm, id),
            Action::MapLocal { storage: MapStorage::Disk, ref managed, .. } if *managed == old
        ));
        assert!(stored(&old).is_ok());
        vm.handle(RuleIntent::Delete(id));
        settle(&mut vm);
        assert!(stored(&old).is_err());
        std::fs::remove_file(target).ok();
    }

    #[test]
    fn a_differing_file_is_not_touched_until_confirmed_and_saved() {
        let disk = tmp("conflict.txt");
        std::fs::write(&disk, "theirs").unwrap();
        let mut vm = map_vm(None);
        to_stored(&mut vm);
        type_text(&mut vm, "mine");

        to_disk(&mut vm, Some(&disk));
        assert_eq!(vm.form.ml.storage, MapStorage::AppData);
        let p = vm.form.ml.prompt.as_ref().unwrap();
        assert!(
            p.replace.contains("conflict.txt with my edits"),
            "{}",
            p.replace
        );
        assert!(p.keep.ends_with("(discard my edits)"), "{}", p.keep);
        assert!(p.summary.contains("Your file: 6 B"), "{}", p.summary);

        choose(&mut vm, MapSwitchChoice::Cancel);
        assert!(vm.form.ml.prompt.is_none() && vm.form.ml.dirty);
        assert_eq!(vm.form.ml.status.as_deref(), Some("Switch cancelled"));

        to_disk(&mut vm, Some(&disk));
        choose(&mut vm, MapSwitchChoice::Replace);
        assert_eq!(vm.form.ml.storage, MapStorage::Disk);
        assert_eq!(std::fs::read(&disk).unwrap(), b"theirs");
        vm.handle(RuleIntent::Submit);
        assert_eq!(std::fs::read(&disk).unwrap(), b"mine");
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn keeping_the_files_contents_discards_edits_only_when_the_label_says_so() {
        let disk = tmp("keep.txt");
        std::fs::write(&disk, "theirs").unwrap();
        let mut vm = map_vm(None);
        to_stored(&mut vm);
        type_text(&mut vm, "mine");
        to_disk(&mut vm, Some(&disk));
        choose(&mut vm, MapSwitchChoice::Adopt);
        assert_eq!(
            (vm.form.ml.storage, vm.form.ml.content.as_str()),
            (MapStorage::Disk, "theirs")
        );
        assert!(!vm.form.ml.dirty);
        vm.handle(RuleIntent::Submit);
        assert_eq!(std::fs::read(&disk).unwrap(), b"theirs");

        // without edits the label doesn't promise a discard
        let mut vm = map_vm(Some(&disk));
        to_stored(&mut vm);
        let name = vm.form.ml.managed.clone();
        vm.handle(RuleIntent::Submit);
        vm.handle(RuleIntent::Edit(1));
        std::fs::write(&disk, "changed").unwrap();
        to_disk(&mut vm, None);
        assert!(!vm.form.ml.prompt.as_ref().unwrap().keep.contains("discard"));
        mocks::delete(&name);
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn a_target_that_changed_after_the_choice_is_asked_about_again() {
        let disk = tmp("race.txt");
        std::fs::write(&disk, "theirs").unwrap();
        let mut vm = map_vm(None);
        to_stored(&mut vm);
        type_text(&mut vm, "mine");
        to_disk(&mut vm, Some(&disk));
        choose(&mut vm, MapSwitchChoice::Replace);

        std::fs::write(&disk, "someone else").unwrap();
        vm.handle(RuleIntent::Submit);
        assert!(vm.cfg.lock().unwrap().rules.is_empty());
        assert_eq!(std::fs::read(&disk).unwrap(), b"someone else");
        assert!(
            vm.form
                .ml
                .prompt
                .as_ref()
                .is_some_and(|p| p.theirs == b"someone else")
        );

        choose(&mut vm, MapSwitchChoice::Replace);
        vm.handle(RuleIntent::Submit);
        assert_eq!(std::fs::read(&disk).unwrap(), b"mine");
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn toggling_back_and_forth_keeps_both_copies() {
        let disk = tmp("both.txt");
        std::fs::write(&disk, "D").unwrap();
        let mut vm = map_vm(Some(&disk));
        to_stored(&mut vm);
        vm.handle(RuleIntent::Submit);
        let name = vm.cfg.lock().unwrap().rules[0]
            .action
            .managed_file()
            .unwrap()
            .to_string();
        mocks::write(&name, b"S").unwrap();

        vm.handle(RuleIntent::Edit(1));
        for _ in 0..2 {
            to_disk(&mut vm, None);
            choose(&mut vm, MapSwitchChoice::Adopt);
            assert_eq!(
                (vm.form.ml.storage, vm.form.ml.content.as_str()),
                (MapStorage::Disk, "D")
            );
            to_stored(&mut vm);
            choose(&mut vm, MapSwitchChoice::Adopt);
            assert_eq!(
                (vm.form.ml.storage, vm.form.ml.content.as_str()),
                (MapStorage::AppData, "S")
            );
        }
        assert_eq!(std::fs::read(&disk).unwrap(), b"D");
        assert_eq!(stored(&name).unwrap(), b"S");
        mocks::delete(&name);
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn a_failed_switch_or_save_leaves_the_rule_and_mode_alone() {
        let mut vm = map_vm(Some(&tmp("missing.txt")));
        to_stored(&mut vm);
        assert_eq!(vm.form.ml.storage, MapStorage::Disk);
        assert!(vm.form.ml.managed.is_empty() && vm.form.ml.error.is_some());

        let mut vm = map_vm(None);
        to_stored(&mut vm);
        to_disk(&mut vm, None);
        assert_eq!(vm.form.ml.storage, MapStorage::AppData);
        assert_eq!(
            vm.form.ml.error.as_deref(),
            Some("Choose where to save the file first")
        );

        // the write fails at save time: nothing changes and the form stays
        to_disk(&mut vm, Some(&tmp("no-such-dir").join("x.txt")));
        vm.handle(RuleIntent::Submit);
        assert!(vm.cfg.lock().unwrap().rules.is_empty());
        assert!(
            vm.form
                .ml
                .error
                .as_deref()
                .unwrap()
                .starts_with("Cannot save")
        );
    }

    #[test]
    fn renaming_keeps_the_content_and_the_old_file_goes_after_the_save() {
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "a.json", "{}");
        let first = managed_of(&vm, id);
        vm.handle(RuleIntent::Edit(id));
        assert_eq!(vm.form.ml.name, "a.json");
        vm.form.ml.name = "b.xml".into();
        vm.handle(RuleIntent::Submit);
        let second = managed_of(&vm, id);
        assert!(second.ends_with("-b.xml") && second != first);
        assert_eq!(stored(&second).unwrap(), b"{}");
        assert!(stored(&first).is_ok());
        settle(&mut vm);
        assert!(stored(&first).is_err() && stored(&second).is_ok());
        vm.handle(RuleIntent::Delete(id));
        settle(&mut vm);
    }

    #[test]
    fn deleting_a_rule_or_collection_removes_the_file_only_after_the_save() {
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "a.json", "1");
        let name = managed_of(&vm, id);
        vm.handle(RuleIntent::Delete(id));
        assert!(stored(&name).is_ok());
        settle(&mut vm);
        assert!(stored(&name).is_err());

        vm.handle(RuleIntent::AddCollection);
        let id = add_stored_rule(&mut vm, "b.json", "2");
        let c = vm.cfg.lock().unwrap().collections[0].id;
        vm.cfg.lock().unwrap().assign_rule(id, Some(c));
        let name = managed_of(&vm, id);
        vm.handle(RuleIntent::DeleteCollection(c, false));
        settle(&mut vm);
        assert!(stored(&name).is_ok());
        vm.handle(RuleIntent::DeleteCollection(c, true));
        vm.handle(RuleIntent::Delete(id));
        settle(&mut vm);
        assert!(stored(&name).is_err());
    }

    #[test]
    fn a_rule_on_disk_keeps_its_stored_copy_until_deleted() {
        let disk = tmp("kept.txt");
        std::fs::write(&disk, "D").unwrap();
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "a.txt", "D");
        let name = managed_of(&vm, id);
        vm.handle(RuleIntent::Edit(id));
        to_disk(&mut vm, Some(&disk));
        vm.handle(RuleIntent::Submit);
        settle(&mut vm);
        assert!(matches!(
            saved_action(&vm, id),
            Action::MapLocal { storage: MapStorage::Disk, ref managed, .. } if *managed == name
        ));
        assert!(stored(&name).is_ok());
        vm.handle(RuleIntent::Delete(id));
        settle(&mut vm);
        assert!(stored(&name).is_err());
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn deleting_the_rule_being_edited_then_saving_keeps_its_file() {
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "a.json", "{\"keep\":true}");
        let name = managed_of(&vm, id);
        vm.handle(RuleIntent::Edit(id));
        vm.handle(RuleIntent::Delete(id));
        settle(&mut vm);
        assert!(stored(&name).is_ok());

        vm.handle(RuleIntent::Submit);
        let rules = vm.cfg.lock().unwrap().rules.len();
        assert_eq!(rules, 1);
        let new_id = vm.cfg.lock().unwrap().rules[0].id;
        assert_eq!(
            stored(&managed_of(&vm, new_id)).unwrap(),
            b"{\"keep\":true}"
        );
        vm.handle(RuleIntent::Delete(new_id));
        settle(&mut vm);
    }

    #[test]
    fn a_rule_replaced_or_deleted_elsewhere_cannot_pull_the_file_from_the_open_form() {
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "a.json", "{}");
        let name = managed_of(&vm, id);
        vm.handle(RuleIntent::Edit(id));
        // what an MCP delete does: the rule goes and its file is queued
        {
            let mut cfg = vm.cfg.lock().unwrap();
            cfg.rules.clear();
            cfg.doomed_mocks.push(name.clone());
        }
        settle(&mut vm);
        assert!(stored(&name).is_ok());
        vm.handle(RuleIntent::Submit);
        settle(&mut vm);
        assert!(stored(&name).is_ok() && vm.cfg.lock().unwrap().rules.len() == 1);
        let last = vm.cfg.lock().unwrap().rules[0].id;
        vm.handle(RuleIntent::Delete(last));
        settle(&mut vm);
    }

    #[test]
    fn saving_with_the_file_gone_rewrites_it_from_the_editor_or_says_why_not() {
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "a.json", "{\"a\":1}");
        let name = managed_of(&vm, id);
        vm.handle(RuleIntent::Edit(id));
        mocks::delete(&name);
        vm.handle(RuleIntent::Submit);
        assert_eq!(stored(&name).unwrap(), b"{\"a\":1}");

        let mut vm2 = vm_with_missing_file();
        vm2.handle(RuleIntent::Submit);
        assert!(vm2.form.ml.error.is_some());
        assert!(vm2.cfg.lock().unwrap().rules.is_empty());
        mocks::delete(&name);
    }

    fn vm_with_missing_file() -> RulesVm {
        let mut vm = vm();
        vm.form = RuleForm {
            kind: RuleKind::MapLocal,
            pattern: "a.com".into(),
            ..RuleForm::default()
        };
        vm.form.ml.storage = MapStorage::AppData;
        vm.form.ml.managed = mocks::new_name("gone.json");
        vm.form.ml.name = "gone.json".into();
        vm
    }

    #[test]
    fn clicking_the_switch_while_asked_cancels_and_a_dismissed_dialog_says_so() {
        let disk = tmp("ask.txt");
        std::fs::write(&disk, "x").unwrap();
        let mut vm = map_vm(None);
        to_stored(&mut vm);
        type_text(&mut vm, "y");
        to_disk(&mut vm, Some(&disk));
        assert!(vm.form.ml.prompt.is_some());
        to_disk(&mut vm, Some(&disk));
        assert!(vm.form.ml.prompt.is_none());
        assert_eq!(vm.form.ml.status.as_deref(), Some("Switch cancelled"));
        vm.form.ml.status = None;
        choose(&mut vm, MapSwitchChoice::Cancel);
        assert_eq!(vm.form.ml.status.as_deref(), Some("Switch cancelled"));
        std::fs::remove_file(disk).ok();
    }

    fn status() -> Status {
        Status::new(String::new(), Arc::new(NoRepaint))
    }

    #[test]
    fn exported_rules_carry_stored_content_and_import_under_fresh_names() {
        let mut vm = vm();
        let a = add_stored_rule(&mut vm, "a.json", "{\"n\":1}");
        let b = add_stored_rule(&mut vm, "b.bin", "bytes");
        let json = vm.rules_json();
        assert!(json.contains("embedded"));

        let mut other = self::vm();
        let mut st = status();
        other.import_rules(&json, &mut st);
        other.import_rules(&json, &mut st);
        assert_eq!(st.text, "Rules imported");
        let cfg = other.cfg.lock().unwrap();
        let names = mocks::names(cfg.rules.iter());
        assert_eq!(names.len(), 4);
        assert_eq!(
            names.iter().collect::<std::collections::HashSet<_>>().len(),
            4
        );
        assert!(
            names
                .iter()
                .all(|n| *n != managed_of(&vm, a) && *n != managed_of(&vm, b))
        );
        assert_eq!(stored(&names[0]).unwrap(), b"{\"n\":1}");
        assert_eq!(stored(&names[1]).unwrap(), b"bytes");
        assert!(names[0].ends_with("-a.json"));
        assert!(
            !serde_json::to_string(&cfg.rules[0].action)
                .unwrap()
                .contains("embedded")
        );
        for n in names
            .iter()
            .chain([&managed_of(&vm, a), &managed_of(&vm, b)])
        {
            mocks::delete(n);
        }
    }

    #[test]
    fn a_disk_rules_retained_copy_is_not_exported() {
        let disk = tmp("exp.txt");
        std::fs::write(&disk, "D").unwrap();
        let mut vm = vm();
        let id = add_stored_rule(&mut vm, "a.txt", "S");
        let name = managed_of(&vm, id);
        vm.handle(RuleIntent::Edit(id));
        to_disk(&mut vm, Some(&disk));
        choose(&mut vm, MapSwitchChoice::Adopt);
        vm.handle(RuleIntent::Submit);
        assert!(!vm.rules_json().contains("embedded"));
        mocks::delete(&name);
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn a_failed_import_leaves_no_files_and_no_rules() {
        let good = mocks::new_name("g.json");
        mocks::write(&good, b"1").unwrap();
        let rule = |n: &str, emb: &str| {
            serde_json::json!({"id": 1, "enabled": true, "pattern": "a.com", "method": "",
            "action": {"MapLocal": {"path": "", "status": 200, "storage": "AppData",
            "managed": n, "embedded": emb}}})
        };
        let text = serde_json::json!({"rules": [rule("g.json", "MQ=="), rule("h.json", "***")]})
            .to_string();
        let mut vm = vm();
        let adopted = || {
            std::fs::read_dir(mocks::dir()).map_or(0, |d| {
                d.flatten()
                    .filter(|e| e.file_name().to_string_lossy().ends_with("-g.json"))
                    .count()
            })
        };
        let before = adopted();
        let mut st = status();
        vm.import_rules(&text, &mut st);
        assert!(st.text.starts_with("Error"), "{}", st.text);
        assert!(vm.cfg.lock().unwrap().rules.is_empty());
        assert_eq!(adopted(), before);
        mocks::delete(&good);
    }
}
