use super::intent::{Effect, ListKind, RuleIntent};
use super::status::Status;
use crate::domain::config::Shared;
use crate::domain::flow::{BpEdit, Decision, Flow, Paused, Stage};
use crate::domain::policy::url_matches;
use crate::domain::rule::{Action, Rule};
use crate::services::flow_store::FlowStore;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum RuleKind {
    Breakpoint,
    Mock,
    MapLocal,
    Redirect,
    Block,
    Throttle,
    Header,
    Body,
    Script,
}

pub const KINDS: [(RuleKind, &str); 9] = [
    (RuleKind::Breakpoint, "Breakpoint"),
    (RuleKind::Mock, "Mock"),
    (RuleKind::MapLocal, "Map Local"),
    (RuleKind::Redirect, "Redirect"),
    (RuleKind::Block, "Block"),
    (RuleKind::Throttle, "Throttle"),
    (RuleKind::Header, "Header"),
    (RuleKind::Body, "Body"),
    (RuleKind::Script, "Script"),
];

const SCRIPT_TEMPLATE: &str = "function onRequest(ctx) {\n  // ctx: { method, url, headers, body }\n  ctx.headers['x-debug'] = '1';\n  return ctx;\n}\n\nfunction onResponse(ctx) {\n  // ctx: { status, headers, body }\n  console.log('status', ctx.status);\n  return ctx;\n}\n";

pub struct RuleForm {
    pub kind: RuleKind,
    pub name: String,
    pub pattern: String,
    pub method: String,
    pub bp_request: bool,
    pub bp_response: bool,
    pub file: String,
    pub status: String,
    pub target: String,
    pub ms: String,
    pub kbps: String,
    pub resp: bool,
    pub hname: String,
    pub hvalue: String,
    pub hremove: bool,
    pub find: String,
    pub replace: String,
    pub regex: bool,
    pub ctype: String,
    pub body: String,
    pub code: String,
    pub map_file_content: String,
    pub map_file_loaded_path: String,
    pub map_file_dirty: bool,
    pub map_file_status: Option<String>,
    pub map_file_error: Option<String>,
}

impl Default for RuleForm {
    fn default() -> Self {
        Self {
            kind: RuleKind::Breakpoint,
            name: String::new(),
            pattern: String::new(),
            method: String::new(),
            bp_request: true,
            bp_response: false,
            file: String::new(),
            status: "200".into(),
            target: String::new(),
            ms: "1000".into(),
            kbps: "0".into(),
            resp: true,
            hname: String::new(),
            hvalue: String::new(),
            hremove: false,
            find: String::new(),
            replace: String::new(),
            regex: false,
            ctype: "application/json".into(),
            body: String::new(),
            code: SCRIPT_TEMPLATE.into(),
            map_file_content: String::new(),
            map_file_loaded_path: String::new(),
            map_file_dirty: false,
            map_file_status: None,
            map_file_error: None,
        }
    }
}

impl RuleForm {
    pub fn from_flow(f: &Flow, kind: RuleKind) -> Self {
        let mut s = Self {
            kind,
            pattern: format!("{}{}", f.host, f.path_only()),
            method: f.method.clone(),
            ..Default::default()
        };
        if let Some(r) = &f.response {
            s.status = f.status.unwrap_or(200).to_string();
            s.body = crate::services::decode::pretty_text(r);
            s.ctype = r
                .header("content-type")
                .unwrap_or("application/json")
                .to_string();
        }
        if kind == RuleKind::Breakpoint {
            s.bp_request = true;
            s.bp_response = true;
        }
        if kind == RuleKind::Block {
            s.status = "503".into();
        }
        s
    }

    pub fn from_rule(r: &Rule) -> Self {
        let mut s = Self {
            name: r.name.clone(),
            pattern: r.pattern.clone(),
            method: r.method.clone(),
            ..Default::default()
        };
        match &r.action {
            Action::Breakpoint { request, response } => {
                s.kind = RuleKind::Breakpoint;
                s.bp_request = *request;
                s.bp_response = *response;
            }
            Action::MapLocal { path, status } => {
                s.kind = RuleKind::MapLocal;
                s.file = path.clone();
                s.status = status.to_string();
            }
            Action::MapRemote { target } => {
                s.kind = RuleKind::Redirect;
                s.target = target.clone();
            }
            Action::Block { status } => {
                s.kind = RuleKind::Block;
                s.status = status.to_string();
            }
            Action::Delay { ms, kbps } => {
                s.kind = RuleKind::Throttle;
                s.ms = ms.to_string();
                s.kbps = kbps.to_string();
            }
            Action::SetHeader {
                response,
                name,
                value,
                remove,
            } => {
                s.kind = RuleKind::Header;
                s.resp = *response;
                s.hname = name.clone();
                s.hvalue = value.clone();
                s.hremove = *remove;
            }
            Action::ReplaceBody {
                response,
                find,
                replace,
                regex,
            } => {
                s.kind = RuleKind::Body;
                s.resp = *response;
                s.find = find.clone();
                s.replace = replace.clone();
                s.regex = *regex;
            }
            Action::Mock {
                status,
                content_type,
                body,
            } => {
                s.kind = RuleKind::Mock;
                s.status = status.to_string();
                s.ctype = content_type.clone();
                s.body = body.clone();
            }
            Action::Script { code } => {
                s.kind = RuleKind::Script;
                s.code = code.clone();
            }
        }
        s
    }

    /// The editor shows the loaded file only while the path still points at it.
    pub fn map_file_editable(&self) -> bool {
        !self.map_file_loaded_path.is_empty() && self.map_file_loaded_path == self.file.trim()
    }

    /// Lower-case extension of the mapped file; also the highlighter's language name.
    pub fn map_file_ext(&self) -> String {
        std::path::Path::new(self.file.trim())
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    pub fn valid(&self) -> bool {
        !self.pattern.trim().is_empty()
            && match self.kind {
                RuleKind::Breakpoint => self.bp_request || self.bp_response,
                RuleKind::MapLocal => !self.file.trim().is_empty(),
                RuleKind::Redirect => !self.target.trim().is_empty(),
                RuleKind::Header => !self.hname.trim().is_empty(),
                RuleKind::Body => !self.find.is_empty(),
                RuleKind::Script => {
                    self.code.contains("onRequest") || self.code.contains("onResponse")
                }
                _ => true,
            }
    }

    pub fn action(&self) -> Action {
        let status = self.status.trim().parse().unwrap_or(200);
        match self.kind {
            RuleKind::Breakpoint => Action::Breakpoint {
                request: self.bp_request,
                response: self.bp_response,
            },
            RuleKind::MapLocal => Action::MapLocal {
                path: self.file.trim().into(),
                status,
            },
            RuleKind::Redirect => Action::MapRemote {
                target: self.target.trim().into(),
            },
            RuleKind::Block => Action::Block { status },
            RuleKind::Throttle => Action::Delay {
                ms: self.ms.trim().parse().unwrap_or(0),
                kbps: self.kbps.trim().parse().unwrap_or(0),
            },
            RuleKind::Header => Action::SetHeader {
                response: self.resp,
                name: self.hname.trim().into(),
                value: self.hvalue.clone(),
                remove: self.hremove,
            },
            RuleKind::Body => Action::ReplaceBody {
                response: self.resp,
                find: self.find.clone(),
                replace: self.replace.clone(),
                regex: self.regex,
            },
            RuleKind::Mock => Action::Mock {
                status,
                content_type: self.ctype.trim().into(),
                body: self.body.clone(),
            },
            RuleKind::Script => Action::Script {
                code: self.code.clone(),
            },
        }
    }
}

pub struct BpDraft {
    pub id: u64,
    pub stage: Stage,
    pub method: String,
    pub url: String,
    pub status: String,
    pub headers: String,
    pub body: String,
    orig_body: Vec<u8>,
    orig_body_text: String,
    reply: Option<tokio::sync::oneshot::Sender<Decision>>,
}

impl BpDraft {
    pub fn new(p: Paused) -> Self {
        let body = String::from_utf8_lossy(&p.edit.body).into_owned();
        Self {
            id: p.id,
            stage: p.stage,
            method: p.edit.method,
            url: p.edit.url,
            status: p.edit.status.to_string(),
            headers: p
                .edit
                .headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join("\n"),
            orig_body: p.edit.body,
            orig_body_text: body.clone(),
            body,
            reply: Some(p.reply),
        }
    }

    pub fn finish(&mut self, resume: bool) {
        let Some(reply) = self.reply.take() else {
            return;
        };
        let _ = reply.send(if resume {
            Decision::Resume(BpEdit {
                method: self.method.clone(),
                url: self.url.clone(),
                status: self.status.trim().parse().unwrap_or(200),
                headers: self
                    .headers
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect(),
                body: if self.body == self.orig_body_text {
                    self.orig_body.clone()
                } else {
                    self.body.clone().into_bytes()
                },
            })
        } else {
            Decision::Abort
        });
    }
}

impl Drop for BpDraft {
    fn drop(&mut self) {
        self.finish(true);
    }
}

pub struct RulesVm {
    pub cfg: Shared,
    pub store: FlowStore,
    pub show: bool,
    pub tab: RulesTab,
    pub form: RuleForm,
    pub editing: Option<u64>,
    pub list_inputs: [String; 3],
    pub console: Vec<String>,
    pub paused: Vec<BpDraft>,
    /// raise the breakpoint window on the next frame
    pub focus_breakpoint: bool,
}

pub struct ScopeView {
    pub scope_only: bool,
    pub scope: Vec<String>,
    pub muted: Vec<String>,
    pub bypass_tls: Vec<String>,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum RulesTab {
    Rules,
    Scope,
    Console,
    Schemas,
    Network,
}

const MAP_FILE_EDIT_LIMIT: u64 = 1 << 20;
const CONSOLE_MAX: usize = 2000;
const CONSOLE_DROP: usize = 500;

impl RulesVm {
    pub fn new(cfg: Shared, store: FlowStore) -> Self {
        Self {
            cfg,
            store,
            show: false,
            tab: RulesTab::Rules,
            form: RuleForm::default(),
            editing: None,
            list_inputs: Default::default(),
            console: vec![],
            paused: vec![],
            focus_breakpoint: false,
        }
    }

    pub fn log(&mut self, l: String) {
        self.console.push(l);
        if self.console.len() > CONSOLE_MAX {
            self.console.drain(..CONSOLE_DROP);
        }
    }

    pub fn pause(&mut self, p: Paused) {
        self.paused.push(BpDraft::new(p));
        self.focus_breakpoint = true;
    }

    pub fn open_from_flow(&mut self, id: u64, kind: RuleKind) {
        let form = self
            .store
            .lock()
            .find(id)
            .map(|f| RuleForm::from_flow(f, kind));
        if let Some(form) = form {
            self.form = form;
            self.editing = None;
            self.tab = RulesTab::Rules;
            self.show = true;
        }
    }

    pub fn live_matches(&self) -> usize {
        let f = &self.form;
        self.store
            .lock()
            .flows
            .iter()
            .filter(|fl| {
                url_matches(&f.pattern, &fl.url())
                    && (f.method.trim().is_empty()
                        || f.method.trim().eq_ignore_ascii_case(&fl.method))
            })
            .count()
    }

    pub fn rules_snapshot(&self) -> Vec<Rule> {
        self.cfg.lock().unwrap().rules.clone()
    }

    pub fn scope_view(&self) -> ScopeView {
        let c = self.cfg.lock().unwrap();
        ScopeView {
            scope_only: c.scope_only,
            scope: c.scope.clone(),
            muted: c.muted.clone(),
            bypass_tls: c.bypass_tls.clone(),
        }
    }

    pub fn active_rule_count(&self) -> usize {
        self.cfg
            .lock()
            .unwrap()
            .rules
            .iter()
            .filter(|r| r.enabled)
            .count()
    }

    pub fn handle(&mut self, intent: RuleIntent) -> Option<Effect> {
        match intent {
            RuleIntent::SetEnabled(id, on) => {
                if let Some(r) = self
                    .cfg
                    .lock()
                    .unwrap()
                    .rules
                    .iter_mut()
                    .find(|r| r.id == id)
                {
                    r.enabled = on;
                }
            }
            RuleIntent::Delete(id) => self.cfg.lock().unwrap().rules.retain(|r| r.id != id),
            RuleIntent::Move(i, d) => {
                let mut cfg = self.cfg.lock().unwrap();
                let j = i as i32 + d;
                if j >= 0 && (j as usize) < cfg.rules.len() && i < cfg.rules.len() {
                    cfg.rules.swap(i, j as usize);
                }
            }
            RuleIntent::Edit(id) => {
                let r = self
                    .cfg
                    .lock()
                    .unwrap()
                    .rules
                    .iter()
                    .find(|r| r.id == id)
                    .cloned();
                if let Some(r) = r {
                    self.form = RuleForm::from_rule(&r);
                    self.editing = Some(id);
                    if matches!(&r.action, Action::MapLocal { .. }) {
                        self.load_map_local_file();
                    }
                }
            }
            RuleIntent::CancelEdit => {
                self.editing = None;
                self.form = RuleForm::default();
            }
            RuleIntent::Submit => self.submit(),
            RuleIntent::SetScopeOnly(on) => self.cfg.lock().unwrap().scope_only = on,
            RuleIntent::AddHost(kind) => {
                let slot = match kind {
                    ListKind::Scope => 0,
                    ListKind::Muted => 1,
                    ListKind::BypassTls => 2,
                };
                let v = self.list_inputs[slot].trim().to_string();
                if !v.is_empty() {
                    let mut cfg = self.cfg.lock().unwrap();
                    match kind {
                        ListKind::Scope => cfg.scope.push(v),
                        ListKind::Muted => cfg.muted.push(v),
                        ListKind::BypassTls => cfg.bypass_tls.push(v),
                    }
                    self.list_inputs[slot].clear();
                }
            }
            RuleIntent::RemoveHost(kind, i) => {
                let mut cfg = self.cfg.lock().unwrap();
                let list = match kind {
                    ListKind::Scope => &mut cfg.scope,
                    ListKind::Muted => &mut cfg.muted,
                    ListKind::BypassTls => &mut cfg.bypass_tls,
                };
                if i < list.len() {
                    list.remove(i);
                }
            }
            RuleIntent::ClearConsole => self.console.clear(),
            RuleIntent::Resolve(resume) => {
                if !self.paused.is_empty() {
                    self.paused.remove(0).finish(resume);
                }
            }
            RuleIntent::LoadMapLocalFile => self.load_map_local_file(),
            RuleIntent::SaveMapLocalFile => self.save_map_local_file(),
        }
        None
    }

    fn load_map_local_file(&mut self) {
        let path = self.form.file.trim().to_string();
        if path.is_empty() {
            self.form.map_file_status = None;
            self.form.map_file_error = Some("Choose a local file first".into());
            return;
        }
        // reads run on the UI thread and the editor lays out every line each frame
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.len() > MAP_FILE_EDIT_LIMIT
        {
            self.form.map_file_loaded_path.clear();
            self.form.map_file_status = None;
            self.form.map_file_error = Some(format!(
                "File is too large to edit here ({} KB; limit {} KB)",
                meta.len() / 1024,
                MAP_FILE_EDIT_LIMIT / 1024
            ));
            return;
        }
        match std::fs::read(&path) {
            Ok(data) => match String::from_utf8(data) {
                Ok(text) => {
                    self.form.map_file_content = text;
                    self.form.map_file_loaded_path = path;
                    self.form.map_file_dirty = false;
                    self.form.map_file_status = Some("File loaded from disk".into());
                    self.form.map_file_error = None;
                }
                Err(_) => {
                    self.form.map_file_status = None;
                    self.form.map_file_error = Some("This file is not valid UTF-8 text".into());
                }
            },
            Err(e) => {
                self.form.map_file_status = None;
                self.form.map_file_error = Some(format!("Cannot read file: {e}"));
            }
        }
    }

    fn save_map_local_file(&mut self) {
        if !self.form.map_file_editable() {
            self.form.map_file_status = None;
            self.form.map_file_error = Some("The file path changed — load the file first".into());
            return;
        }
        let path = self.form.map_file_loaded_path.clone();
        match std::fs::write(&path, &self.form.map_file_content) {
            Ok(_) => {
                self.form.map_file_loaded_path = path;
                self.form.map_file_dirty = false;
                self.form.map_file_status = Some("Saved directly to disk".into());
                self.form.map_file_error = None;
            }
            Err(e) => {
                self.form.map_file_status = None;
                self.form.map_file_error = Some(format!("Cannot save file: {e}"));
            }
        }
    }

    fn submit(&mut self) {
        if !self.form.valid() {
            return;
        }
        let f = &self.form;
        let action = f.action();
        let (name, pattern, method) = (
            f.name.trim().to_string(),
            f.pattern.trim().to_string(),
            f.method.trim().to_string(),
        );
        let mut cfg = self.cfg.lock().unwrap();
        match self
            .editing
            .take()
            .and_then(|id| cfg.rules.iter_mut().find(|r| r.id == id))
        {
            Some(r) => {
                r.name = name;
                r.pattern = pattern;
                r.method = method;
                r.action = action;
            }
            None => {
                let id = cfg.next_rule_id();
                cfg.rules.push(Rule {
                    id,
                    enabled: true,
                    name,
                    pattern,
                    method,
                    action,
                    hits: 0,
                });
            }
        }
        drop(cfg);
        self.form = RuleForm::default();
    }

    pub fn import_rules(&mut self, text: &str, status: &mut Status) {
        match serde_json::from_str::<crate::domain::config::Config>(text) {
            Ok(mut c) => {
                let mut cfg = self.cfg.lock().unwrap();
                let first = cfg.next_rule_id();
                for (id, r) in (first..).zip(c.rules.drain(..)) {
                    cfg.rules.push(Rule { id, ..r });
                }
                status.set("Rules imported");
            }
            Err(e) => status.set(format!("Error: {e}")),
        }
    }

    pub fn rules_json(&self) -> String {
        serde_json::to_string_pretty(&*self.cfg.lock().unwrap()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use crate::domain::flow::Flow;
    use crate::viewmodels::status::NoRepaint;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    fn vm() -> RulesVm {
        RulesVm::new(
            Arc::new(Mutex::new(Config::default())),
            FlowStore::default(),
        )
    }

    fn add_block(vm: &mut RulesVm, pattern: &str) {
        vm.form.pattern = pattern.into();
        vm.form.kind = RuleKind::Block;
        vm.handle(RuleIntent::Submit);
    }

    #[test]
    fn submit_adds_rule_resets_form_and_applies_immediately() {
        let mut vm = vm();
        add_block(&mut vm, "a.com");
        assert_eq!(
            vm.cfg
                .lock()
                .unwrap()
                .matching("GET", "https://a.com/x")
                .len(),
            1
        );
        assert!(vm.form.pattern.is_empty());
    }

    #[test]
    fn invalid_form_is_not_submitted() {
        let mut vm = vm();
        vm.handle(RuleIntent::Submit);
        vm.form.kind = RuleKind::Header;
        vm.form.pattern = "a.com".into();
        vm.handle(RuleIntent::Submit);
        assert!(vm.cfg.lock().unwrap().rules.is_empty());
    }

    #[test]
    fn map_local_save_refuses_when_path_changed_since_load() {
        let mut vm = vm();
        let dir = std::env::temp_dir();
        let a = dir.join(format!("pxp-map-a-{}.json", std::process::id()));
        let b = dir.join(format!("pxp-map-b-{}.json", std::process::id()));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        vm.form.file = a.display().to_string();
        vm.handle(RuleIntent::LoadMapLocalFile);
        assert_eq!(vm.form.map_file_ext(), "json");
        vm.form.file = b.display().to_string();
        vm.handle(RuleIntent::SaveMapLocalFile);
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
        assert!(vm.form.map_file_error.is_some());
        std::fs::remove_file(a).ok();
        std::fs::remove_file(b).ok();
    }

    #[test]
    fn map_local_refuses_files_too_large_to_edit() {
        let mut vm = vm();
        let p = std::env::temp_dir().join(format!("pxp-map-big-{}.bin", std::process::id()));
        std::fs::write(&p, vec![b'x'; MAP_FILE_EDIT_LIMIT as usize + 1]).unwrap();
        vm.form.file = p.display().to_string();
        vm.handle(RuleIntent::LoadMapLocalFile);
        assert!(!vm.form.map_file_editable());
        assert!(
            vm.form
                .map_file_error
                .as_deref()
                .unwrap()
                .contains("too large")
        );
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn map_local_file_load_edit_and_save_are_explicit() {
        let mut vm = vm();
        let path = std::env::temp_dir().join(format!("pxp-map-edit-{}.txt", std::process::id()));
        std::fs::write(&path, "before").unwrap();
        vm.form.file = path.display().to_string();
        vm.handle(RuleIntent::LoadMapLocalFile);
        assert_eq!(vm.form.map_file_content, "before");
        assert!(!vm.form.map_file_dirty);

        vm.form.map_file_content = "after".into();
        vm.form.map_file_dirty = true;
        vm.handle(RuleIntent::SaveMapLocalFile);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
        assert_eq!(
            vm.form.map_file_status.as_deref(),
            Some("Saved directly to disk")
        );
        assert!(!vm.form.map_file_dirty);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn edit_then_submit_updates_in_place() {
        let mut vm = vm();
        add_block(&mut vm, "a.com");
        let id = vm.cfg.lock().unwrap().rules[0].id;
        vm.handle(RuleIntent::Edit(id));
        assert_eq!(vm.editing, Some(id));
        vm.form.pattern = "b.com".into();
        vm.form.status = "418".into();
        vm.handle(RuleIntent::Submit);
        let cfg = vm.cfg.lock().unwrap();
        assert_eq!(cfg.rules.len(), 1);
        assert_eq!(
            (cfg.rules[0].pattern.as_str(), cfg.rules[0].id),
            ("b.com", id)
        );
        assert_eq!(cfg.rules[0].action, Action::Block { status: 418 });
    }

    #[test]
    fn cancel_edit_resets() {
        let mut vm = vm();
        add_block(&mut vm, "a.com");
        let id = vm.cfg.lock().unwrap().rules[0].id;
        vm.handle(RuleIntent::Edit(id));
        vm.handle(RuleIntent::CancelEdit);
        assert!(vm.editing.is_none() && vm.form.pattern.is_empty());
    }

    #[test]
    fn move_delete_and_toggle() {
        let mut vm = vm();
        add_block(&mut vm, "a.com");
        add_block(&mut vm, "b.com");
        let (a, b) = {
            let c = vm.cfg.lock().unwrap();
            (c.rules[0].id, c.rules[1].id)
        };
        vm.handle(RuleIntent::Move(0, 1));
        vm.handle(RuleIntent::Move(1, 1));
        vm.handle(RuleIntent::Move(0, -1));
        assert_eq!(
            vm.cfg
                .lock()
                .unwrap()
                .rules
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            vec![b, a]
        );
        vm.handle(RuleIntent::SetEnabled(a, false));
        assert_eq!(vm.active_rule_count(), 1);
        vm.handle(RuleIntent::Delete(b));
        assert_eq!(vm.cfg.lock().unwrap().rules.len(), 1);
    }

    #[test]
    fn host_lists_add_trim_and_remove() {
        let mut vm = vm();
        vm.list_inputs[1] = "  tracker.com ".into();
        vm.handle(RuleIntent::AddHost(ListKind::Muted));
        assert_eq!(
            vm.cfg.lock().unwrap().muted,
            vec!["tracker.com".to_string()]
        );
        assert!(vm.list_inputs[1].is_empty());
        vm.handle(RuleIntent::AddHost(ListKind::Muted));
        assert_eq!(vm.cfg.lock().unwrap().muted.len(), 1);
        vm.handle(RuleIntent::RemoveHost(ListKind::Muted, 0));
        assert!(vm.cfg.lock().unwrap().muted.is_empty());
        vm.list_inputs[2] = "pinned.com".into();
        vm.handle(RuleIntent::AddHost(ListKind::BypassTls));
        assert!(!vm.cfg.lock().unwrap().should_decrypt("pinned.com"));
    }

    #[test]
    fn console_is_capped() {
        let mut vm = vm();
        for i in 0..(CONSOLE_MAX + 10) {
            vm.log(i.to_string());
        }
        assert!(vm.console.len() <= CONSOLE_MAX);
        assert_eq!(vm.console.last().unwrap(), &(CONSOLE_MAX + 9).to_string());
        vm.handle(RuleIntent::ClearConsole);
        assert!(vm.console.is_empty());
    }

    #[test]
    fn breakpoint_resume_sends_edit_and_abort_sends_abort() {
        let mut vm = vm();
        let paused = |body: &[u8]| {
            let (tx, rx) = oneshot::channel();
            (
                Paused {
                    id: 1,
                    stage: Stage::Request,
                    edit: BpEdit {
                        method: "GET".into(),
                        url: "http://a/".into(),
                        status: 0,
                        headers: vec![("a".into(), "1".into())],
                        body: body.to_vec(),
                    },
                    reply: tx,
                },
                rx,
            )
        };
        let (p, mut rx) = paused(b"orig");
        vm.pause(p);
        assert!(vm.focus_breakpoint);
        vm.paused[0].body = "changed".into();
        vm.handle(RuleIntent::Resolve(true));
        match rx.try_recv().unwrap() {
            Decision::Resume(e) => assert_eq!(
                (e.body, e.headers),
                (
                    b"changed".to_vec(),
                    vec![("a".to_string(), "1".to_string())]
                )
            ),
            Decision::Abort => panic!("expected resume"),
        }
        let (p, mut rx) = paused(b"orig");
        vm.pause(p);
        vm.handle(RuleIntent::Resolve(false));
        assert!(matches!(rx.try_recv().unwrap(), Decision::Abort));
        assert!(vm.paused.is_empty());
    }

    #[test]
    fn untouched_breakpoint_body_is_sent_byte_exact() {
        let mut vm = vm();
        let (tx, mut rx) = oneshot::channel();
        vm.pause(Paused {
            id: 1,
            stage: Stage::Response,
            edit: BpEdit {
                method: String::new(),
                url: String::new(),
                status: 200,
                headers: vec![],
                body: vec![0xff, 0xfe],
            },
            reply: tx,
        });
        vm.handle(RuleIntent::Resolve(true));
        match rx.try_recv().unwrap() {
            Decision::Resume(e) => assert_eq!(e.body, vec![0xff, 0xfe]),
            Decision::Abort => panic!(),
        }
    }

    #[test]
    fn dropping_a_pending_breakpoint_resumes_it() {
        let mut vm = vm();
        let (tx, mut rx) = oneshot::channel();
        vm.pause(Paused {
            id: 1,
            stage: Stage::Request,
            edit: BpEdit {
                method: "GET".into(),
                url: "u".into(),
                status: 0,
                headers: vec![],
                body: vec![],
            },
            reply: tx,
        });
        drop(vm);
        assert!(matches!(rx.try_recv().unwrap(), Decision::Resume(_)));
    }

    #[test]
    fn open_from_flow_prefills_form_and_shows_window() {
        let mut vm = vm();
        let mut f = Flow::sample(1);
        f.host = "api.x.com".into();
        f.path = "/v1/u?q=1".into();
        f.method = "POST".into();
        vm.store.lock().push(f);
        vm.open_from_flow(1, RuleKind::Block);
        assert!(vm.show);
        assert_eq!(
            (
                vm.form.pattern.as_str(),
                vm.form.method.as_str(),
                vm.form.status.as_str()
            ),
            ("api.x.com/v1/u", "POST", "503")
        );
        vm.open_from_flow(99, RuleKind::Block);
        assert_eq!(vm.form.pattern, "api.x.com/v1/u");
    }

    #[test]
    fn live_match_count_follows_pattern_and_method() {
        let mut vm = vm();
        for (id, path) in [(1, "/a"), (2, "/b")] {
            let mut f = Flow::sample(id);
            f.path = path.into();
            vm.store.lock().push(f);
        }
        vm.form.pattern = "a.com/a".into();
        assert_eq!(vm.live_matches(), 1);
        vm.form.pattern = "a.com".into();
        vm.form.method = "post".into();
        assert_eq!(vm.live_matches(), 0);
    }

    #[test]
    fn rules_export_import_roundtrip_reassigns_ids() {
        let mut vm = vm();
        add_block(&mut vm, "a.com");
        let json = vm.rules_json();
        let mut st = Status::new(String::new(), Arc::new(NoRepaint));
        vm.import_rules(&json, &mut st);
        let ids: Vec<u64> = vm.cfg.lock().unwrap().rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(st.text, "Rules imported");
        vm.import_rules("not json", &mut st);
        assert!(st.text.starts_with("Error:"));
    }

    #[test]
    fn script_rule_needs_a_hook() {
        let mut f = RuleForm {
            kind: RuleKind::Script,
            pattern: "a".into(),
            ..Default::default()
        };
        assert!(f.valid());
        f.code = "console.log(1)".into();
        assert!(!f.valid());
    }

    #[test]
    fn form_roundtrips_every_action_kind() {
        let actions = vec![
            Action::Breakpoint {
                request: true,
                response: true,
            },
            Action::MapLocal {
                path: "/x".into(),
                status: 201,
            },
            Action::MapRemote {
                target: "http://l:3000".into(),
            },
            Action::Block { status: 503 },
            Action::Delay { ms: 5, kbps: 2 },
            Action::SetHeader {
                response: true,
                name: "x".into(),
                value: "y".into(),
                remove: false,
            },
            Action::ReplaceBody {
                response: false,
                find: "a".into(),
                replace: "b".into(),
                regex: true,
            },
            Action::Mock {
                status: 200,
                content_type: "text/plain".into(),
                body: "hi".into(),
            },
            Action::Script {
                code: "function onRequest(c){return c}".into(),
            },
        ];
        for a in actions {
            let r = Rule {
                id: 1,
                enabled: true,
                name: String::new(),
                pattern: "p".into(),
                method: String::new(),
                action: a.clone(),
                hits: 0,
            };
            assert_eq!(RuleForm::from_rule(&r).action(), a);
        }
    }
}
