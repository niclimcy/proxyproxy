use super::intent::{Effect, ListKind, MockTarget, RuleIntent};
use super::map_local::{Live, MapLocalForm};
use super::status::Status;
use crate::domain::config::{Config, Shared};
use crate::domain::flow::{BpEdit, Decision, Flow, Paused, Stage};
use crate::domain::pac;
use crate::domain::policy::{rule_active, url_matches};
use crate::domain::rule::{Action, MapStorage, Rule};
use crate::services::flow_store::FlowStore;
use crate::services::mocks::{self, Source};
use bytes::Bytes;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

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
    pub ml: MapLocalForm,
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
            ml: MapLocalForm::default(),
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
        }
    }
}

impl RuleForm {
    /// `raw_body` keeps the response bytes as they are; otherwise the body is pretty-printed for editing.
    pub fn from_flow(f: &Flow, kind: RuleKind, raw_body: bool) -> Self {
        let mut s = Self {
            kind,
            pattern: f.rule_pattern(),
            method: f.method.clone(),
            ..Default::default()
        };
        if let Some(r) = &f.response {
            s.status = f.status.unwrap_or(200).to_string();
            s.body = if raw_body {
                String::from_utf8_lossy(&r.body).into_owned()
            } else {
                crate::services::decode::pretty_text(r)
            };
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
            Action::MapLocal {
                path,
                status,
                storage,
                managed,
                ..
            } => {
                s.kind = RuleKind::MapLocal;
                s.status = status.to_string();
                s.ml.file.clone_from(path);
                s.ml.storage = *storage;
                s.ml.managed.clone_from(managed);
                s.ml.name = if managed.is_empty() {
                    mocks::clean_name(path)
                } else {
                    mocks::display_name(managed).to_string()
                };
                s.ml.live = Some(Live {
                    storage: *storage,
                    label: match storage {
                        MapStorage::Disk => path.clone(),
                        MapStorage::AppData => format!("stored copy ({})", s.ml.name),
                    },
                });
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

    pub fn valid(&self) -> bool {
        !self.pattern.trim().is_empty()
            && match self.kind {
                RuleKind::Breakpoint => self.bp_request || self.bp_response,
                RuleKind::MapLocal => match self.ml.storage {
                    MapStorage::Disk => !self.ml.file.trim().is_empty(),
                    MapStorage::AppData => !self.ml.managed.is_empty(),
                },
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
                path: self.ml.file.trim().into(),
                status,
                storage: self.ml.storage,
                managed: self.ml.managed.clone(),
                embedded: None,
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
    /// status-line text for the app to show after a handled intent
    pub message: Option<String>,
    pub renaming: Option<u64>,
    pub rename_input: String,
    pub rename_focus: bool,
    pub confirm_delete: Option<u64>,
    pub collapsed: HashSet<Option<u64>>,
    /// rules just created by a mock, and when; they glow for `HIGHLIGHT`
    pub highlight: Option<(HashSet<u64>, Instant)>,
    /// the list scrolls to this rule once
    pub scroll_to: Option<u64>,
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
    /// `None` when the pill already says it all
    pub description: Option<String>,
    pub hits: u64,
    pub enabled: bool,
}

pub struct RuleGroup {
    /// `None` is Ungrouped
    pub id: Option<u64>,
    pub name: String,
    pub enabled: bool,
    pub rows: Vec<RuleRow>,
}

pub struct RuleList {
    pub groups: Vec<RuleGroup>,
    /// id and name of every collection, for pickers
    pub collections: Vec<(u64, String)>,
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

pub const HIGHLIGHT: Duration = Duration::from_millis(1500);
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
            message: None,
            renaming: None,
            rename_input: String::new(),
            rename_focus: false,
            confirm_delete: None,
            collapsed: HashSet::new(),
            highlight: None,
            scroll_to: None,
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
            .map(|f| RuleForm::from_flow(f, kind, false));
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
                description: (!matches!(r.action, Action::Breakpoint { .. }) || !r.name.is_empty())
                    .then(|| r.action.describe()),
                hits: r.hits,
                enabled: r.enabled,
            });
        }
        if !cfg.collections.is_empty() && groups[0].rows.is_empty() {
            groups.remove(0);
        }
        RuleList {
            collections: cfg.collection_names(),
            groups,
        }
    }

    /// Adds a collection and opens it for renaming, so its first name is typed over.
    fn new_collection_for_rename(&mut self, name: &str) -> u64 {
        let id = self.cfg.lock().unwrap().add_collection(name);
        self.working = Some(id);
        self.renaming = Some(id);
        self.rename_input = name.to_string();
        self.rename_focus = true;
        self.collapsed.remove(&Some(id));
        self.show = true;
        self.tab = RulesTab::Rules;
        if self.editing.is_none() {
            self.form.collection = Some(id);
        }
        id
    }

    /// Turns captured responses into Mock rules in the target collection, with the bodies byte for byte.
    /// A rule with the same method and pattern already there gets the new response instead of a twin;
    /// several flows for one endpoint keep the latest. Then the panel opens on what changed.
    pub fn mock_flows(&mut self, ids: &[u64], target: MockTarget) {
        if let MockTarget::Collection(c) = target
            && !self.cfg.lock().unwrap().collection_exists(c)
        {
            self.message = Some("That collection no longer exists".into());
            return;
        }
        let wanted: HashSet<u64> = ids.iter().copied().collect();
        let mut forms: Vec<(u64, RuleForm)> = vec![];
        let mut slot: HashMap<(String, String), usize> = HashMap::new();
        let mut hosts: Vec<String> = vec![];
        let mut skipped = 0;
        {
            let st = self.store.lock();
            for f in st.flows.iter().filter(|f| wanted.contains(&f.id)) {
                if f.mock_problem().is_some() {
                    skipped += 1;
                    continue;
                }
                if !hosts.iter().any(|h| h == f.hostname()) {
                    hosts.push(f.hostname().to_string());
                }
                let at = slot.get(&(f.method.clone(), f.rule_pattern())).copied();
                match at {
                    Some(i) if forms[i].0 > f.id => {}
                    Some(i) => forms[i] = (f.id, RuleForm::from_flow(f, RuleKind::Mock, true)),
                    None => {
                        slot.insert((f.method.clone(), f.rule_pattern()), forms.len());
                        forms.push((f.id, RuleForm::from_flow(f, RuleKind::Mock, true)));
                    }
                }
            }
        }
        if forms.is_empty() {
            self.message = Some("Nothing to mock: no text response".into());
            return;
        }
        let collection = match target {
            MockTarget::NewCollection => {
                let name = match hosts.as_slice() {
                    [one] => one.clone(),
                    _ => "Mocked responses".into(),
                };
                Some(self.new_collection_for_rename(&name))
            }
            MockTarget::Collection(id) => Some(id),
            MockTarget::NoCollection => None,
        };
        let touched: Vec<u64> = {
            let mut cfg = self.cfg.lock().unwrap();
            forms
                .iter()
                .map(|(_, f)| cfg.upsert_mock(collection, &f.method, &f.pattern, f.action()))
                .collect()
        };
        let dest = collection
            .and_then(|c| {
                let cfg = self.cfg.lock().unwrap();
                cfg.collections
                    .iter()
                    .find(|x| x.id == c)
                    .map(|x| x.name.clone())
            })
            .unwrap_or_else(|| "no collection".into());
        let skipped = if skipped > 0 {
            format!(" ({skipped} skipped)")
        } else {
            String::new()
        };
        self.message = Some(match forms.as_slice() {
            [(_, f)] if skipped.is_empty() => {
                format!("Mocked {} {} into {dest}", f.method, f.pattern)
            }
            _ => format!("Mocked {} responses into {dest}{skipped}", forms.len()),
        });
        self.show = true;
        self.tab = RulesTab::Rules;
        self.collapsed.remove(&collection);
        self.scroll_to = touched.first().copied();
        self.highlight = Some((touched.into_iter().collect(), Instant::now()));
    }

    /// True while a rule that was just mocked should still glow.
    pub fn highlighted(&self, rule: u64) -> bool {
        self.highlight
            .as_ref()
            .is_some_and(|(ids, at)| at.elapsed() < HIGHLIGHT && ids.contains(&rule))
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
            RuleIntent::Delete(id) => {
                let mut cfg = self.cfg.lock().unwrap();
                let gone = mocks::names(cfg.rules.iter().filter(|r| r.id == id));
                cfg.rules.retain(|r| r.id != id);
                cfg.doomed_mocks.extend(gone);
            }
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
                self.cfg.lock().unwrap().set_collection_enabled(id, on);
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
            RuleIntent::AddCollection => {
                self.new_collection_for_rename("New collection");
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
                    if delete_rules {
                        let gone =
                            mocks::names(cfg.rules.iter().filter(|r| r.collection == Some(id)));
                        cfg.doomed_mocks.extend(gone);
                    }
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
            RuleIntent::RevealMapFile => self.reveal_map_file(),
            RuleIntent::SetMapStorage(to, target) => self.set_map_storage(to, target),
            RuleIntent::ResolveMapSwitch(choice) => self.resolve_map_switch(choice),
        }
        None
    }

    fn submit(&mut self) {
        if !self.form.valid() {
            return;
        }
        let mut doomed = vec![];
        if self.form.kind == RuleKind::MapLocal {
            // every write happens here, before the rule changes: a failure leaves the rule as it was
            let ml = &mut self.form.ml;
            let editor = ml.editable().then(|| ml.content.clone().into_bytes());
            let (write, stored_as) = match ml.storage {
                MapStorage::AppData => {
                    let wanted = mocks::clean_name(&ml.name);
                    let renamed =
                        ml.managed.is_empty() || wanted != mocks::display_name(&ml.managed);
                    let target = if renamed {
                        mocks::new_name(&wanted)
                    } else {
                        ml.managed.clone()
                    };
                    let dest = Source::Stored(target.clone());
                    let bytes = if ml.dirty {
                        Some(ml.content.clone().into_bytes())
                    } else if let Some(s) = &ml.staged {
                        Some(s.bytes.clone())
                    } else {
                        let old = if renamed { &ml.managed } else { &target };
                        match Source::Stored(old.clone()).read() {
                            Ok(b) => renamed.then_some(b),
                            Err(_) if editor.is_some() => editor,
                            Err(e) => {
                                ml.fail(format!(
                                    "The stored copy is missing and can't be rebuilt: {e}"
                                ));
                                return;
                            }
                        }
                    };
                    (bytes.map(|b| (dest, b)), Some(target))
                }
                MapStorage::Disk => {
                    let dest = Source::Disk(ml.file.trim().into());
                    let bytes = ml.staged.as_ref().filter(|s| s.dest == dest).map(|s| {
                        if ml.dirty {
                            ml.content.clone().into_bytes()
                        } else {
                            s.bytes.clone()
                        }
                    });
                    (bytes.map(|b| (dest, b)), None)
                }
            };
            if let Some((dest, bytes)) = write {
                if let Some(s) = ml.staged.as_ref().filter(|s| s.dest == dest) {
                    let now = match dest.read() {
                        Ok(b) => Some(b),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                        Err(e) => {
                            ml.fail(format!("Cannot read {}: {e}", dest.label()));
                            return;
                        }
                    };
                    // changed since the user decided: ask again instead of overwriting
                    if let Some(theirs) = now
                        && s.seen.as_ref() != Some(&theirs)
                        && theirs != bytes
                    {
                        ml.prompt = Some(ml.make_prompt(ml.storage, dest, bytes, theirs));
                        return;
                    }
                }
                if let Err(e) = dest.write(&bytes) {
                    ml.fail(format!("Cannot save {}: {e}", dest.label()));
                    return;
                }
            }
            if let Some(target) = stored_as {
                doomed.push(std::mem::replace(&mut ml.managed, target));
            }
            ml.staged = None;
            ml.dirty = false;
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
            doomed.extend(r.action.managed_file().map(str::to_string));
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
        cfg.doomed_mocks.extend(doomed);
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
            Ok((collections, mut rules)) => {
                for i in 0..rules.len() {
                    if let Err(e) = mocks::adopt(&mut rules[i].action, None) {
                        for done in &rules[..i] {
                            if let Some(name) = done.action.managed_file() {
                                mocks::delete(name);
                            }
                        }
                        status.set(format!("Error: {e}"));
                        return;
                    }
                }
                self.cfg.lock().unwrap().merge_rules(collections, rules);
                status.set("Rules imported");
            }
            Err(e) => status.set(format!("Error: {e}")),
        }
    }

    // config-shaped so older builds can import it, but only the rules: the rest
    // (upstream credentials, keylog path, scope) shouldn't travel with a shared file
    pub fn rules_json(&self) -> String {
        let (mut rules, collections) = {
            let cfg = self.cfg.lock().unwrap();
            (cfg.rules.clone(), cfg.collections.clone())
        };
        mocks::embed(&mut rules);
        let only_rules = Config {
            rules,
            collections,
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
        vm.form.ml.file = a.display().to_string();
        vm.handle(RuleIntent::LoadMapLocalFile);
        assert_eq!(vm.form.ml.ext(), "json");
        vm.form.ml.file = b.display().to_string();
        vm.handle(RuleIntent::SaveMapLocalFile);
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
        assert!(vm.form.ml.error.is_some());
        std::fs::remove_file(a).ok();
        std::fs::remove_file(b).ok();
    }

    #[test]
    fn map_local_refuses_files_too_large_to_edit() {
        let mut vm = vm();
        let p = std::env::temp_dir().join(format!("pxp-map-big-{}.bin", std::process::id()));
        std::fs::write(&p, vec![b'x'; (1 << 20) + 1]).unwrap();
        vm.form.ml.file = p.display().to_string();
        vm.handle(RuleIntent::LoadMapLocalFile);
        assert!(!vm.form.ml.editable());
        assert!(vm.form.ml.limit.as_deref().unwrap().contains("over 1 MB"));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn map_local_file_load_edit_and_save_are_explicit() {
        let mut vm = vm();
        let path = std::env::temp_dir().join(format!("pxp-map-edit-{}.txt", std::process::id()));
        std::fs::write(&path, "before").unwrap();
        vm.form.ml.file = path.display().to_string();
        vm.handle(RuleIntent::LoadMapLocalFile);
        assert_eq!(vm.form.ml.content, "before");
        assert!(!vm.form.ml.dirty);

        vm.form.ml.content = "after".into();
        vm.form.ml.dirty = true;
        vm.handle(RuleIntent::SaveMapLocalFile);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
        assert_eq!(vm.form.ml.status.as_deref(), Some("Saved directly to disk"));
        assert!(!vm.form.ml.dirty);
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
        vm.handle(RuleIntent::AddCollection);
        let id = vm.cfg.lock().unwrap().collections.last().unwrap().id;
        vm.handle(RuleIntent::RenameCollection(id, name.into()));
        id
    }

    #[test]
    fn collection_toggle_applies_to_the_next_request() {
        let mut vm = vm();
        let a = add_collection(&mut vm, "Errors");
        add_block(&mut vm, "a.com");
        assert_eq!(matches(&vm, "https://a.com/").len(), 1);
        vm.handle(RuleIntent::SetCollectionEnabled(a, false));
        assert!(matches(&vm, "https://a.com/").is_empty());
        assert_eq!(vm.active_rule_count(), 0);
        assert!(vm.message.is_none(), "a plain toggle stays quiet");
        vm.handle(RuleIntent::SetCollectionEnabled(a, true));
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
    fn rule_list_puts_ungrouped_first_and_hides_it_when_empty() {
        let mut vm = vm();
        assert_eq!(vm.rule_list().groups.len(), 1);
        let a = add_collection(&mut vm, "A");
        let list = vm.rule_list();
        assert_eq!(list.groups.len(), 1, "empty Ungrouped is hidden");
        assert_eq!(list.groups[0].id, Some(a));
        vm.handle(RuleIntent::NewRuleIn(Some(a)));
        add_block(&mut vm, "a.com");
        vm.handle(RuleIntent::NewRuleIn(None));
        add_block(&mut vm, "c.com");
        vm.handle(RuleIntent::SetCollectionEnabled(a, false));
        let list = vm.rule_list();
        assert_eq!(list.groups[0].id, None);
        assert_eq!(list.groups[1].rows.len(), 1);
        assert!(!list.groups[1].enabled);
    }

    #[test]
    fn new_collection_starts_in_rename_mode() {
        let mut vm = vm();
        vm.handle(RuleIntent::AddCollection);
        let id = vm.cfg.lock().unwrap().collections[0].id;
        assert_eq!(vm.cfg.lock().unwrap().collections[0].name, "New collection");
        assert_eq!(vm.renaming, Some(id));
        assert!(vm.rename_focus && vm.rename_input == "New collection");
        assert_eq!(vm.working, Some(id));
    }

    fn answered(id: u64, host: &str, path: &str, body: &[u8]) -> Flow {
        let mut f = Flow::sample(id);
        f.host = host.into();
        f.path = path.into();
        f.status = Some(200);
        f.response = Some(crate::domain::flow::Message {
            headers: vec![("content-type".into(), "application/json".into())],
            body: Bytes::copy_from_slice(body),
        });
        f
    }

    fn mock_rules(vm: &RulesVm) -> Vec<(Option<u64>, String, String)> {
        let cfg = vm.cfg.lock().unwrap();
        cfg.rules
            .iter()
            .filter_map(|r| match &r.action {
                Action::Mock { body, .. } => Some((r.collection, r.pattern.clone(), body.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn mocking_a_response_creates_a_rule_without_opening_the_form() {
        let mut vm = vm();
        vm.store
            .lock()
            .push(answered(1, "api.x.com", "/u?q=1", b"{\"a\":1}"));
        let a = add_collection(&mut vm, "A");
        vm.handle(RuleIntent::CancelEdit);
        vm.mock_flows(&[1], MockTarget::Collection(a));
        assert_eq!(mock_rules(&vm).len(), 1);
        assert_eq!(mock_rules(&vm)[0].0, Some(a));
        assert_eq!(mock_rules(&vm)[0].1, "api.x.com/u");
        assert!(vm.form.pattern.is_empty() && vm.editing.is_none());
        assert_eq!(vm.message.as_deref(), Some("Mocked GET api.x.com/u into A"));
        vm.mock_flows(&[1], MockTarget::NoCollection);
        assert_eq!(mock_rules(&vm)[1].0, None);
    }

    #[test]
    fn mocking_into_a_new_collection_names_it_after_the_host_and_opens_rename() {
        let mut vm = vm();
        vm.store.lock().push(answered(1, "api.x.com", "/u", b"{}"));
        vm.mock_flows(&[1], MockTarget::NewCollection);
        let (id, name) = {
            let cfg = vm.cfg.lock().unwrap();
            (cfg.collections[0].id, cfg.collections[0].name.clone())
        };
        assert_eq!(name, "api.x.com");
        assert!(vm.show && vm.tab == RulesTab::Rules);
        assert_eq!(vm.renaming, Some(id));
        assert_eq!(mock_rules(&vm)[0].0, Some(id));
    }

    #[test]
    fn mocking_again_replaces_the_response_in_the_same_collection() {
        let mut vm = vm();
        vm.store.lock().push(answered(1, "a.com", "/x", b"old"));
        let c = add_collection(&mut vm, "C");
        vm.mock_flows(&[1], MockTarget::Collection(c));
        vm.store.lock().push(answered(2, "a.com", "/x", b"new"));
        vm.mock_flows(&[2], MockTarget::Collection(c));
        assert_eq!(
            mock_rules(&vm),
            vec![(Some(c), "a.com/x".to_string(), "new".to_string())]
        );
        vm.mock_flows(&[2], MockTarget::NoCollection);
        assert_eq!(
            mock_rules(&vm).len(),
            2,
            "another collection gets its own rule"
        );
    }

    #[test]
    fn mocked_bodies_are_the_captured_bytes_not_a_reformat() {
        let mut vm = vm();
        vm.store
            .lock()
            .push(answered(1, "a.com", "/j", br#"{"b":1,"a":2}"#));
        vm.store
            .lock()
            .push(answered(2, "a.com", "/t", b"plain  text\n"));
        vm.mock_flows(&[1, 2], MockTarget::NoCollection);
        let bodies: Vec<String> = mock_rules(&vm).into_iter().map(|r| r.2).collect();
        assert_eq!(bodies, [r#"{"b":1,"a":2}"#, "plain  text\n"]);
    }

    #[test]
    fn mocking_into_a_deleted_collection_changes_nothing() {
        let mut vm = vm();
        vm.store.lock().push(answered(1, "a.com", "/x", b"{}"));
        let a = add_collection(&mut vm, "A");
        vm.cfg.lock().unwrap().delete_collection(a, false);
        vm.mock_flows(&[1], MockTarget::Collection(a));
        assert!(mock_rules(&vm).is_empty());
        assert_eq!(
            vm.message.as_deref(),
            Some("That collection no longer exists")
        );
    }

    #[test]
    fn mocking_opens_the_rules_tab_and_marks_what_changed() {
        let mut vm = vm();
        vm.store.lock().push(answered(1, "a.com", "/x", b"{}"));
        let c = add_collection(&mut vm, "C");
        vm.collapsed.insert(Some(c));
        vm.show = false;
        vm.mock_flows(&[1], MockTarget::Collection(c));
        let id = vm.cfg.lock().unwrap().rules[0].id;
        assert!(vm.show && vm.tab == RulesTab::Rules);
        assert!(!vm.collapsed.contains(&Some(c)));
        assert_eq!(vm.scroll_to, Some(id));
        assert!(vm.highlighted(id) && !vm.highlighted(id + 1));
    }

    #[test]
    fn mocking_a_flow_without_a_text_response_does_nothing() {
        let mut vm = vm();
        vm.store.lock().push(Flow::sample(1));
        vm.store
            .lock()
            .push(answered(2, "a.com", "/bin", &[0xff, 0xfe]));
        assert_eq!(
            vm.store.lock().find(1).unwrap().mock_problem(),
            Some("No response yet")
        );
        assert_eq!(
            vm.store.lock().find(2).unwrap().mock_problem(),
            Some("Compressed or binary response")
        );
        vm.mock_flows(&[1, 2], MockTarget::NewCollection);
        assert!(mock_rules(&vm).is_empty() && vm.cfg.lock().unwrap().collections.is_empty());
        assert!(vm.message.is_some());
    }

    #[test]
    fn mocking_all_visible_keeps_the_latest_per_endpoint_and_counts_skips() {
        let mut vm = vm();
        {
            let mut st = vm.store.lock();
            st.push(answered(1, "a.com", "/x", b"first"));
            st.push(answered(2, "a.com", "/y", b"y"));
            st.push(answered(3, "a.com", "/x", b"second"));
            st.push(Flow::sample(4));
            st.push(answered(5, "b.com", "/z", &[0xff]));
        }
        vm.mock_flows(&[1, 2, 3, 4, 5], MockTarget::NewCollection);
        let rules = mock_rules(&vm);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules.iter().find(|r| r.1 == "a.com/x").unwrap().2, "second");
        assert_eq!(
            vm.message.as_deref(),
            Some("Mocked 2 responses into a.com (2 skipped)")
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
                storage: MapStorage::Disk,
                managed: String::new(),
                embedded: None,
            },
            Action::MapLocal {
                path: "/back.json".into(),
                status: 200,
                storage: MapStorage::AppData,
                managed: "0123456789abcdef-r.json".into(),
                embedded: None,
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
