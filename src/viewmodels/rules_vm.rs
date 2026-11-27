use super::intent::{Effect, ListKind, RuleIntent};
use super::status::Status;
use crate::domain::config::{Config, Shared};
use crate::domain::flow::{BpEdit, Decision, Flow, Paused, Stage};
use crate::domain::pac;
use crate::domain::policy::{rule_active, url_matches};
use crate::domain::rule::{Action, Rule, RuleCollection};
use crate::services::flow_store::FlowStore;
use bytes::Bytes;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

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
    pub collection: Option<u64>,
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
            collection: None,
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
            collection: r.collection,
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
                s.file.clone_from(path);
                s.status = status.to_string();
            }
            Action::MapRemote { target } => {
                s.kind = RuleKind::Redirect;
                s.target.clone_from(target);
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
                s.hname.clone_from(name);
                s.hvalue.clone_from(value);
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
                s.find.clone_from(find);
                s.replace.clone_from(replace);
                s.regex = *regex;
            }
            Action::Mock {
                status,
                content_type,
                body,
            } => {
                s.kind = RuleKind::Mock;
                s.status = status.to_string();
                s.ctype.clone_from(content_type);
                s.body.clone_from(body);
            }
            Action::Script { code } => {
                s.kind = RuleKind::Script;
                s.code.clone_from(code);
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
            .map_or_default(|e| e.to_string_lossy().to_lowercase())
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
    orig_body: Bytes,
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
                    Bytes::from(self.body.clone().into_bytes())
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
    /// where new rules go: the collection last created or last given a "+"
    pub working: Option<u64>,
    pub new_collection: String,
    pub adding_collection: bool,
    /// status-line text for the app to show after a handled intent
    pub message: Option<String>,
    pub renaming: Option<u64>,
    pub rename_input: String,
    pub rename_focus: bool,
    pub confirm_delete: Option<u64>,
    pub collapsed: HashSet<Option<u64>>,
    pub list_inputs: [String; 5],
    pub console: Vec<String>,
    pub paused: Vec<BpDraft>,
    /// raise the breakpoint window on the next frame
    pub focus_breakpoint: bool,
    // the panel asks every frame; counting 20k flows each time stalls the UI
    live_cache: Cell<Option<(u64, usize)>>,
    scope_cache: Cell<Option<(u64, usize)>>,
}

fn cached_count(
    cache: &Cell<Option<(u64, usize)>>,
    key: impl Hash,
    count: impl FnOnce() -> usize,
) -> usize {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    let key = h.finish();
    match cache.get() {
        Some((k, n)) if k == key => n,
        _ => {
            let n = count();
            cache.set(Some((key, n)));
            n
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Warning,
    Danger,
    Violet,
    Info,
    Success,
}

/// What the list draws for one rule; the action itself stays in the config.
pub struct RuleRow {
    pub id: u64,
    pub label: &'static str,
    pub tone: Tone,
    pub method: String,
    pub pattern: String,
    pub name: String,
    pub description: String,
    pub hits: u64,
    pub enabled: bool,
}

pub struct RuleGroup {
    /// `None` is Ungrouped
    pub id: Option<u64>,
    pub name: String,
    pub enabled: bool,
    /// rules with their own switch on, whatever the collection says
    pub on: usize,
    pub rows: Vec<RuleRow>,
}

pub struct RuleList {
    pub groups: Vec<RuleGroup>,
    /// id and name of every collection, for pickers
    pub collections: Vec<(u64, String)>,
    /// one line about which collections are on; `None` while there are none
    pub summary: Option<String>,
    pub all_on: bool,
}

pub struct ScopeView {
    pub scope_only: bool,
    pub scope: Vec<String>,
    pub exclude: Vec<String>,
    pub muted: Vec<String>,
    pub bypass_tls: Vec<String>,
    pub proxy_only: bool,
    pub proxy_only_hosts: Vec<String>,
    /// scope entries carry paths that seeding the proxy-only list drops
    pub scope_has_paths: bool,
    /// recorded flows that pass scope, exclude and mute, out of all recorded flows
    pub matching: usize,
    pub total: usize,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum RulesTab {
    Rules,
    Scope,
    Console,
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
            working: None,
            new_collection: String::new(),
            adding_collection: false,
            message: None,
            renaming: None,
            rename_input: String::new(),
            rename_focus: false,
            confirm_delete: None,
            collapsed: HashSet::new(),
            list_inputs: Default::default(),
            console: vec![],
            paused: vec![],
            focus_breakpoint: false,
            live_cache: Cell::default(),
            scope_cache: Cell::default(),
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
            self.form = RuleForm {
                collection: self.working,
                ..form
            };
            self.editing = None;
            self.tab = RulesTab::Rules;
            self.show = true;
        }
    }

    pub fn live_matches(&self) -> usize {
        let f = &self.form;
        let st = self.store.lock();
        cached_count(&self.live_cache, (st.rev(), &f.pattern, &f.method), || {
            st.flows
                .iter()
                .filter(|fl| {
                    url_matches(&f.pattern, &fl.url())
                        && (f.method.trim().is_empty()
                            || f.method.trim().eq_ignore_ascii_case(&fl.method))
                })
                .count()
        })
    }

    /// Ungrouped first, then collections in creation order. Ungrouped is left out when it is empty
    /// and collections exist, so it only shows up when it has something to say.
    pub fn rule_list(&self) -> RuleList {
        let cfg = self.cfg.lock().unwrap();
        let counts = cfg.collection_counts();
        let mut groups: Vec<RuleGroup> = std::iter::once((None, "No collection", true))
            .chain(
                cfg.collections
                    .iter()
                    .map(|c| (Some(c.id), c.name.as_str(), c.enabled)),
            )
            .map(|(id, name, enabled)| RuleGroup {
                id,
                name: name.into(),
                enabled,
                on: counts.get(&id).map_or(0, |c| c.1),
                rows: vec![],
            })
            .collect();
        let slot: HashMap<Option<u64>, usize> =
            groups.iter().enumerate().map(|(i, g)| (g.id, i)).collect();
        for r in &cfg.rules {
            // a dangling id shows with Ungrouped, matching how it is treated
            let g = slot.get(&r.collection).copied().unwrap_or(0);
            groups[g].rows.push(RuleRow {
                id: r.id,
                label: r.action.label(),
                tone: match r.action {
                    Action::Breakpoint { .. } => Tone::Warning,
                    Action::Block { .. } => Tone::Danger,
                    Action::Script { .. } => Tone::Violet,
                    Action::Delay { .. } => Tone::Info,
                    _ => Tone::Success,
                },
                method: r.method.clone(),
                pattern: r.pattern.clone(),
                name: r.name.clone(),
                description: r.action.describe(),
                hits: r.hits,
                enabled: r.enabled,
            });
        }
        if !cfg.collections.is_empty() && groups[0].rows.is_empty() {
            groups.remove(0);
        }
        let named: Vec<&RuleCollection> = cfg.collections.iter().collect();
        let on: Vec<&&RuleCollection> = named.iter().filter(|c| c.enabled).collect();
        let summary = match (named.len(), on.len()) {
            (0, _) => None,
            (_, 0) => Some("All collections off".to_string()),
            (n, k) if k == n && n > 1 => Some("All collections on".to_string()),
            (_, 1) => Some(format!("Active: {}", on[0].name)),
            (n, k) => Some(format!("{k} of {n} collections on")),
        };
        RuleList {
            collections: named.iter().map(|c| (c.id, c.name.clone())).collect(),
            all_on: on.len() == named.len(),
            summary,
            groups,
        }
    }

    fn blank_form(&self) -> RuleForm {
        RuleForm {
            collection: self.working,
            ..RuleForm::default()
        }
    }

    pub fn scope_view(&self) -> ScopeView {
        let c = {
            let cfg = self.cfg.lock().unwrap();
            Config {
                scope_only: cfg.scope_only,
                scope: cfg.scope.clone(),
                exclude: cfg.exclude.clone(),
                muted: cfg.muted.clone(),
                bypass_tls: cfg.bypass_tls.clone(),
                proxy_only: cfg.proxy_only,
                proxy_only_hosts: cfg.proxy_only_hosts.clone(),
                ..Config::default()
            }
        };
        let st = self.store.lock();
        let key = (st.rev(), c.scope_only, &c.scope, &c.exclude, &c.muted);
        let matching = cached_count(&self.scope_cache, key, || {
            st.flows
                .iter()
                .filter(|f| c.in_record_scope(&f.url()))
                .count()
        });
        ScopeView {
            scope_only: c.scope_only,
            scope: c.scope.clone(),
            exclude: c.exclude.clone(),
            muted: c.muted.clone(),
            bypass_tls: c.bypass_tls.clone(),
            proxy_only: c.proxy_only,
            proxy_only_hosts: c.proxy_only_hosts.clone(),
            scope_has_paths: pac::has_path_entries(&c.scope),
            matching,
            total: st.flows.len(),
        }
    }

    pub fn active_rule_count(&self) -> usize {
        let cfg = self.cfg.lock().unwrap();
        let off = cfg.disabled_collections();
        cfg.rules.iter().filter(|r| rule_active(&off, r)).count()
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
            // priority is the flat order, but the list shows groups, so a rule trades places with its neighbour in the same group
            RuleIntent::Move(id, d) => {
                let mut cfg = self.cfg.lock().unwrap();
                let i = cfg.rules.iter().position(|r| r.id == id)?;
                let group = cfg.rules[i].collection;
                let neighbour = if d > 0 {
                    (i + 1..cfg.rules.len()).find(|&j| cfg.rules[j].collection == group)
                } else {
                    cfg.rules[..i].iter().rposition(|r| r.collection == group)
                };
                if let Some(j) = neighbour {
                    cfg.rules.swap(i, j);
                }
            }
            RuleIntent::SetRuleCollection(id, collection) => {
                self.cfg.lock().unwrap().assign_rule(id, collection);
            }
            RuleIntent::SetCollectionEnabled(id, on) => {
                let mut cfg = self.cfg.lock().unwrap();
                if cfg.set_collection_enabled(id, on) {
                    let name = cfg
                        .collections
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| c.name.clone());
                    self.message = name.map(|n| format!("{n} {}", if on { "on" } else { "off" }));
                }
            }
            RuleIntent::SoloCollection(id) => {
                let mut cfg = self.cfg.lock().unwrap();
                if cfg.solo_collection(id) {
                    let others = cfg.collections.len() - 1;
                    let name = cfg
                        .collections
                        .iter()
                        .find(|c| c.id == id)
                        .map_or("", |c| c.name.as_str());
                    self.message = Some(match others {
                        0 => format!("{name} on"),
                        n => format!("{name} on, {n} other{} off", if n == 1 { "" } else { "s" }),
                    });
                }
            }
            RuleIntent::EnableAllCollections => {
                for c in &mut self.cfg.lock().unwrap().collections {
                    c.enabled = true;
                }
                self.message = Some("All collections on".into());
            }
            RuleIntent::AddCollection => {
                let name = self.new_collection.trim().to_string();
                if !name.is_empty() {
                    let id = self.cfg.lock().unwrap().add_collection(&name);
                    self.new_collection.clear();
                    self.adding_collection = false;
                    self.working = Some(id);
                    if self.editing.is_none() {
                        self.form.collection = Some(id);
                    }
                }
            }
            RuleIntent::RenameCollection(id, name) => {
                let name = name.trim();
                if !name.is_empty()
                    && let Some(c) = self
                        .cfg
                        .lock()
                        .unwrap()
                        .collections
                        .iter_mut()
                        .find(|c| c.id == id)
                {
                    c.name = name.to_string();
                }
            }
            RuleIntent::DeleteCollection(id, delete_rules) => {
                let editing_gone = {
                    let mut cfg = self.cfg.lock().unwrap();
                    cfg.delete_collection(id, delete_rules);
                    self.editing
                        .is_some_and(|e| !cfg.rules.iter().any(|r| r.id == e))
                };
                self.confirm_delete = None;
                if self.working == Some(id) {
                    self.working = None;
                }
                if self.form.collection == Some(id) {
                    self.form.collection = None;
                }
                self.collapsed.remove(&Some(id));
                if editing_gone {
                    self.editing = None;
                    self.form = self.blank_form();
                }
            }
            RuleIntent::NewRuleIn(collection) => {
                self.working =
                    collection.filter(|c| self.cfg.lock().unwrap().collection_exists(*c));
                self.editing = None;
                self.collapsed.remove(&collection);
                self.form = self.blank_form();
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
                self.form = self.blank_form();
            }
            RuleIntent::Submit => self.submit(),
            RuleIntent::SetScopeOnly(on) => self.cfg.lock().unwrap().scope_only = on,
            RuleIntent::SetProxyOnly(on) => self.cfg.lock().unwrap().proxy_only = on,
            RuleIntent::SeedProxyOnlyFromScope => {
                let mut cfg = self.cfg.lock().unwrap();
                let merged = [cfg.proxy_only_hosts.clone(), cfg.scope.clone()].concat();
                cfg.proxy_only_hosts = pac::normalize_hosts(&merged);
            }
            RuleIntent::AddHost(kind) => {
                let slot = match kind {
                    ListKind::Scope => 0,
                    ListKind::Exclude => 1,
                    ListKind::Muted => 2,
                    ListKind::BypassTls => 3,
                    ListKind::ProxyOnly => 4,
                };
                let v = self.list_inputs[slot].trim().to_string();
                if !v.is_empty() {
                    let mut cfg = self.cfg.lock().unwrap();
                    match kind {
                        ListKind::Scope => cfg.scope.push(v),
                        ListKind::Exclude => cfg.exclude.push(v),
                        ListKind::Muted => cfg.muted.push(v),
                        ListKind::BypassTls => cfg.bypass_tls.push(v),
                        ListKind::ProxyOnly => cfg.proxy_only_hosts.push(v),
                    }
                    self.list_inputs[slot].clear();
                }
            }
            RuleIntent::RemoveHost(kind, i) => {
                let mut cfg = self.cfg.lock().unwrap();
                let list = match kind {
                    ListKind::Scope => &mut cfg.scope,
                    ListKind::Exclude => &mut cfg.exclude,
                    ListKind::Muted => &mut cfg.muted,
                    ListKind::BypassTls => &mut cfg.bypass_tls,
                    ListKind::ProxyOnly => &mut cfg.proxy_only_hosts,
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
            Ok(data) => {
                if let Ok(text) = String::from_utf8(data) {
                    self.form.map_file_content = text;
                    self.form.map_file_loaded_path = path;
                    self.form.map_file_dirty = false;
                    self.form.map_file_status = Some("File loaded from disk".into());
                    self.form.map_file_error = None;
                } else {
                    self.form.map_file_status = None;
                    self.form.map_file_error = Some("This file is not valid UTF-8 text".into());
                }
            }
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
            Ok(()) => {
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
        // the collection may have been deleted since the form was filled in
        let collection = f.collection.filter(|c| cfg.collection_exists(*c));
        if let Some(r) = self
            .editing
            .take()
            .and_then(|id| cfg.rules.iter_mut().find(|r| r.id == id))
        {
            r.name = name;
            r.pattern = pattern;
            r.method = method;
            r.action = action;
            r.collection = collection;
        } else {
            let id = cfg.next_rule_id();
            cfg.rules.push(Rule {
                id,
                enabled: true,
                name,
                pattern,
                method,
                action,
                collection,
                hits: 0,
            });
        }
        drop(cfg);
        self.form = self.blank_form();
    }

    pub fn import_rules(&mut self, text: &str, status: &mut Status) {
        // files from before collections were a bare array, or a config-shaped object without them
        let parsed = serde_json::from_str::<Config>(text)
            .map(|c| (c.collections, c.rules))
            .or_else(|e| {
                serde_json::from_str::<Vec<Rule>>(text)
                    .map(|rules| (vec![], rules))
                    .map_err(|_| e)
            });
        match parsed {
            Ok((collections, rules)) => {
                self.cfg.lock().unwrap().merge_rules(collections, rules);
                status.set("Rules imported");
            }
            Err(e) => status.set(format!("Error: {e}")),
        }
    }

    // config-shaped so older builds can import it, but only the rules: the rest
    // (upstream credentials, keylog path, scope) shouldn't travel with a shared file
    pub fn rules_json(&self) -> String {
        let cfg = self.cfg.lock().unwrap();
        let only_rules = Config {
            rules: cfg.rules.clone(),
            collections: cfg.collections.clone(),
            ..Config::default()
        };
        serde_json::to_string_pretty(&only_rules).unwrap_or_default()
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
                .matching("GET", "https://a.com/x", |r| r.id)
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
        vm.list_inputs[2] = "  tracker.com ".into();
        vm.handle(RuleIntent::AddHost(ListKind::Muted));
        assert_eq!(
            vm.cfg.lock().unwrap().muted,
            vec!["tracker.com".to_string()]
        );
        assert!(vm.list_inputs[2].is_empty());
        vm.handle(RuleIntent::AddHost(ListKind::Muted));
        assert_eq!(vm.cfg.lock().unwrap().muted.len(), 1);
        vm.handle(RuleIntent::RemoveHost(ListKind::Muted, 0));
        assert!(vm.cfg.lock().unwrap().muted.is_empty());
        vm.list_inputs[3] = "pinned.com".into();
        vm.handle(RuleIntent::AddHost(ListKind::BypassTls));
        assert!(!vm.cfg.lock().unwrap().should_decrypt("pinned.com"));
    }

    #[test]
    fn proxy_only_list_seeds_from_scope_hosts_without_touching_scope() {
        let mut vm = vm();
        vm.list_inputs[4] = "keep.com".into();
        vm.handle(RuleIntent::AddHost(ListKind::ProxyOnly));
        {
            let mut c = vm.cfg.lock().unwrap();
            c.scope = vec![
                "a.com/v2/*".into(),
                "https://b.com".into(),
                "keep.com".into(),
            ];
        }
        assert!(vm.scope_view().scope_has_paths);
        vm.handle(RuleIntent::SeedProxyOnlyFromScope);
        vm.handle(RuleIntent::SetProxyOnly(true));
        let sv = vm.scope_view();
        assert_eq!(sv.proxy_only_hosts, ["keep.com", "a.com", "b.com"]);
        assert!(sv.proxy_only);
        assert_eq!(sv.scope.len(), 3);
        vm.handle(RuleIntent::RemoveHost(ListKind::ProxyOnly, 0));
        assert_eq!(vm.scope_view().proxy_only_hosts, ["a.com", "b.com"]);
    }

    #[test]
    fn scope_view_counts_recorded_flows_in_scope() {
        let mut vm = vm();
        for (id, host, path) in [
            (1, "a.com", "/v2/x"),
            (2, "a.com", "/v1/x"),
            (3, "b.com", "/v2/x"),
        ] {
            let mut f = Flow::sample(id);
            f.host = host.into();
            f.path = path.into();
            vm.store.lock().push(f);
        }
        assert_eq!(vm.scope_view().matching, 3);
        vm.list_inputs[0] = "a.com/v2/*".into();
        vm.handle(RuleIntent::AddHost(ListKind::Scope));
        vm.handle(RuleIntent::SetScopeOnly(true));
        let sv = vm.scope_view();
        assert_eq!((sv.matching, sv.total), (1, 3));
        vm.list_inputs[1] = "a.com/v2/x".into();
        vm.handle(RuleIntent::AddHost(ListKind::Exclude));
        assert_eq!(vm.scope_view().matching, 0);
        vm.handle(RuleIntent::RemoveHost(ListKind::Exclude, 0));
        assert!(vm.cfg.lock().unwrap().exclude.is_empty());
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
                        body: Bytes::copy_from_slice(body),
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
                    Bytes::from_static(b"changed"),
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
                body: Bytes::from_static(&[0xff, 0xfe]),
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
                body: Bytes::new(),
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
    fn exported_rules_leave_out_other_settings_and_reimport() {
        let mut vm = vm();
        vm.form.pattern = "a.com".into();
        vm.handle(RuleIntent::Submit);
        vm.cfg.lock().unwrap().upstream.url = "http://user:secret@proxy:3128".into();
        let json = vm.rules_json();
        assert!(!json.contains("secret"));
        let mut st = Status::new(String::new(), Arc::new(NoRepaint));
        vm.import_rules(&json, &mut st);
        let ids: Vec<u64> = vm.cfg.lock().unwrap().rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "imported rules get fresh ids");
        assert_eq!(st.text, "Rules imported");
        vm.import_rules("not json", &mut st);
        assert!(st.text.starts_with("Error:"));
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
        let mut f = Flow::sample(3);
        f.method = "POST".into();
        vm.store.lock().push(f);
        assert_eq!(vm.live_matches(), 1);
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

    fn matches(vm: &RulesVm, url: &str) -> Vec<u64> {
        vm.cfg.lock().unwrap().matching("GET", url, |r| r.id)
    }

    fn add_collection(vm: &mut RulesVm, name: &str) -> u64 {
        vm.new_collection = name.into();
        vm.handle(RuleIntent::AddCollection);
        vm.cfg.lock().unwrap().collections.last().unwrap().id
    }

    #[test]
    fn collection_toggle_applies_to_the_next_request_and_says_so() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "Errors");
        add_block(&mut vm, "a.com");
        assert_eq!(matches(&vm, "https://a.com/").len(), 1);
        vm.handle(RuleIntent::SetCollectionEnabled(a, false));
        assert!(matches(&vm, "https://a.com/").is_empty());
        assert_eq!(vm.active_rule_count(), 0);
        assert_eq!(vm.message.take().as_deref(), Some("Errors off"));
        vm.handle(RuleIntent::EnableAllCollections);
        assert_eq!(matches(&vm, "https://a.com/").len(), 1);
    }

    #[test]
    fn solo_reports_how_many_others_went_off() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "Checkout");
        add_collection(&mut vm, "B");
        add_collection(&mut vm, "C");
        vm.handle(RuleIntent::SoloCollection(a));
        assert_eq!(
            vm.message.take().as_deref(),
            Some("Checkout on, 2 others off")
        );
        vm.handle(RuleIntent::SoloCollection(99));
        assert!(vm.message.is_none());
    }

    #[test]
    fn new_rules_go_to_the_working_collection() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "A");
        assert_eq!(vm.form.collection, Some(a));
        add_block(&mut vm, "a.com");
        assert_eq!(vm.form.collection, Some(a), "stays for the next rule");
        vm.handle(RuleIntent::NewRuleIn(None));
        add_block(&mut vm, "b.com");
        vm.handle(RuleIntent::NewRuleIn(Some(99)));
        assert_eq!(vm.working, None, "an unknown collection isn't remembered");
        let cfg = vm.cfg.lock().unwrap();
        assert_eq!(cfg.rules[0].collection, Some(a));
        assert_eq!(cfg.rules[1].collection, None);
    }

    #[test]
    fn submit_drops_a_collection_that_was_deleted_meanwhile() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "A");
        vm.form.collection = Some(a);
        vm.cfg.lock().unwrap().delete_collection(a, false);
        add_block(&mut vm, "a.com");
        assert_eq!(vm.cfg.lock().unwrap().rules[0].collection, None);
    }

    #[test]
    fn editing_a_rule_can_move_it_to_another_collection() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "A");
        let b = add_collection(&mut vm, "B");
        add_block(&mut vm, "a.com");
        let id = vm.cfg.lock().unwrap().rules[0].id;
        vm.handle(RuleIntent::Edit(id));
        assert_eq!(vm.form.collection, Some(b));
        vm.form.collection = Some(a);
        vm.handle(RuleIntent::Submit);
        assert_eq!(vm.cfg.lock().unwrap().rules[0].collection, Some(a));
    }

    #[test]
    fn rename_ignores_blank_names() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "  A  ");
        assert_eq!(vm.cfg.lock().unwrap().collections[0].name, "A");
        vm.handle(RuleIntent::RenameCollection(a, "  ".into()));
        assert_eq!(vm.cfg.lock().unwrap().collections[0].name, "A");
        vm.handle(RuleIntent::RenameCollection(a, " Slow ".into()));
        assert_eq!(vm.cfg.lock().unwrap().collections[0].name, "Slow");
        vm.new_collection = " ".into();
        vm.handle(RuleIntent::AddCollection);
        assert_eq!(vm.cfg.lock().unwrap().collections.len(), 1);
    }

    #[test]
    fn deleting_a_collection_resets_what_pointed_at_it() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "A");
        add_block(&mut vm, "a.com");
        let id = vm.cfg.lock().unwrap().rules[0].id;
        vm.handle(RuleIntent::Edit(id));
        vm.confirm_delete = Some(a);
        vm.handle(RuleIntent::DeleteCollection(a, true));
        assert!(vm.cfg.lock().unwrap().rules.is_empty());
        assert!(vm.editing.is_none() && vm.confirm_delete.is_none());
        assert_eq!((vm.working, vm.form.collection), (None, None));
    }

    #[test]
    fn moving_a_rule_swaps_within_its_group_only() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "A");
        add_block(&mut vm, "a1.com");
        vm.handle(RuleIntent::NewRuleIn(None));
        add_block(&mut vm, "u.com");
        vm.handle(RuleIntent::NewRuleIn(Some(a)));
        add_block(&mut vm, "a2.com");
        let id = |vm: &RulesVm, p: &str| {
            vm.cfg
                .lock()
                .unwrap()
                .rules
                .iter()
                .find(|r| r.pattern == p)
                .unwrap()
                .id
        };
        let order = |vm: &RulesVm| -> Vec<String> {
            vm.cfg
                .lock()
                .unwrap()
                .rules
                .iter()
                .map(|r| r.pattern.clone())
                .collect()
        };
        let a1 = id(&vm, "a1.com");
        vm.handle(RuleIntent::Move(a1, 1));
        assert_eq!(order(&vm), ["a2.com", "u.com", "a1.com"]);
        vm.handle(RuleIntent::Move(a1, 1));
        assert_eq!(
            order(&vm),
            ["a2.com", "u.com", "a1.com"],
            "already last in its group"
        );
        vm.handle(RuleIntent::Move(a1, -1));
        assert_eq!(order(&vm), ["a1.com", "u.com", "a2.com"]);
        vm.handle(RuleIntent::Move(12345, 1));
    }

    #[test]
    fn rule_list_groups_counts_and_summarises() {
        let mut vm = vm();
        let list = vm.rule_list();
        assert_eq!((list.groups.len(), list.summary), (1, None));
        let a = add_collection(&mut vm, "A");
        let b = add_collection(&mut vm, "B");
        let list = vm.rule_list();
        assert_eq!(list.groups.len(), 2, "empty Ungrouped is hidden");
        assert_eq!(list.summary.as_deref(), Some("All collections on"));
        vm.handle(RuleIntent::NewRuleIn(Some(a)));
        add_block(&mut vm, "a.com");
        add_block(&mut vm, "b.com");
        let first = vm.cfg.lock().unwrap().rules[0].id;
        vm.handle(RuleIntent::SetEnabled(first, false));
        vm.handle(RuleIntent::SetCollectionEnabled(a, false));
        vm.handle(RuleIntent::NewRuleIn(None));
        add_block(&mut vm, "c.com");
        let list = vm.rule_list();
        assert_eq!(list.groups[0].id, None);
        assert_eq!(list.groups[1].rows.len(), 2);
        assert_eq!((list.groups[1].on, list.groups[1].enabled), (1, false));
        assert_eq!(list.summary.as_deref(), Some("Active: B"));
        assert!(!list.all_on);
        vm.handle(RuleIntent::SetCollectionEnabled(b, false));
        assert_eq!(
            vm.rule_list().summary.as_deref(),
            Some("All collections off")
        );
        vm.handle(RuleIntent::SetCollectionEnabled(b, true));
        add_collection(&mut vm, "C");
        assert_eq!(
            vm.rule_list().summary.as_deref(),
            Some("2 of 3 collections on")
        );
    }

    #[test]
    fn exported_collections_reimport_under_new_ids() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "A");
        add_block(&mut vm, "a.com");
        vm.handle(RuleIntent::SetCollectionEnabled(a, false));
        let json = vm.rules_json();
        let mut st = Status::new(String::new(), Arc::new(NoRepaint));
        vm.import_rules(&json, &mut st);
        let cfg = vm.cfg.lock().unwrap();
        assert_eq!(cfg.collections.len(), 2);
        let copy = &cfg.collections[1];
        assert_ne!(copy.id, a);
        assert!(!copy.enabled);
        assert_eq!(cfg.rules[1].collection, Some(copy.id));
        assert_eq!(cfg.rules[0].collection, Some(a));
    }

    #[test]
    fn imports_a_plain_array_of_rules_from_before_collections() {
        let mut vm = vm();
        let mut st = Status::new(String::new(), Arc::new(NoRepaint));
        vm.import_rules(
            r#"[{"id":7,"enabled":true,"pattern":"a.com","method":"","action":{"Block":{"status":503}}}]"#,
            &mut st,
        );
        assert_eq!(st.text, "Rules imported");
        let cfg = vm.cfg.lock().unwrap();
        assert_eq!((cfg.rules.len(), cfg.rules[0].collection), (1, None));
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
                collection: None,
                hits: 0,
            };
            assert_eq!(RuleForm::from_rule(&r).action(), a);
        }
    }
}
