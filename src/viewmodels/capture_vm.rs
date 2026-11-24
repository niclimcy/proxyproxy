use super::intent::{Effect, RowIntent, ShortcutIntent, SidebarIntent, ToolbarIntent};
use super::status::{Flash, Status};
use crate::domain::config::{Config, Shared};
use crate::domain::filter::Query;
use crate::domain::flow::Flow;
use crate::services::app_icons::IconStore;
use crate::services::flow_store::FlowStore;
use crate::domain::app::AppIdentity;
use std::collections::{BTreeMap, HashMap};

pub const ALL_COLS: [&str; 8] = [
    "#", "App", "Method", "Status", "Host", "Path", "Time", "Size",
];

/// How long a new flow may wait for its identity before the sidebar lists it by process name.
const IDENTITY_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

pub type Tree = Vec<(String, Vec<(String, usize)>)>;
type VisKey = (u64, String, Option<(usize, bool)>);

pub struct CaptureVm {
    pub store: FlowStore,
    pub icons: IconStore,
    pub cfg: Shared,
    pub selected: Option<u64>,
    pub filter: String,
    pub query: Query,
    query_src: String,
    pub visible: Vec<usize>,
    vis_key: VisKey,
    pub sort: Option<(usize, bool)>,
    pub tree: Tree,
    /// Simulator and emulator apps, one folder per platform, kept apart from host apps.
    pub platforms: Vec<(String, Tree)>,
    pub app_ids: HashMap<String, AppIdentity>,
    tree_rev: u64,
    /// Flows still waiting for their app identity; the sidebar keeps repainting until they settle.
    pub tree_pending: usize,
    pub show_sidebar: bool,
    pub follow: bool,
    pub focus_search: bool,
    pub test_command_copied: Flash,
}

fn sort_cmp(col: usize, x: &Flow, y: &Flow) -> std::cmp::Ordering {
    match col {
        0 => x.id.cmp(&y.id),
        1 => x.app_label().cmp(&y.app_label()),
        2 => x.method.cmp(&y.method),
        3 => x.status.cmp(&y.status),
        4 => x.host.cmp(&y.host),
        5 => x.path.cmp(&y.path),
        6 => x.duration_ms.cmp(&y.duration_ms),
        _ => x.res_size().cmp(&y.res_size()),
    }
}

// `/v2/users/1?x` scopes to `host/v2/users/*`: the id alone would match one flow
fn scope_path_pattern(f: &Flow) -> String {
    let dir = f.path_only().rsplit_once('/').map_or("", |(d, _)| d);
    format!("{}{dir}/*", f.host)
}

fn exclude_pattern(f: &Flow) -> String {
    format!("{}{}", f.host, f.path_only())
}

impl CaptureVm {
    pub fn new(store: FlowStore, cfg: Shared, icons: IconStore) -> Self {
        Self {
            store,
            icons,
            cfg,
            selected: None,
            filter: String::new(),
            query: Query::default(),
            query_src: String::new(),
            visible: vec![],
            vis_key: (0, String::new(), None),
            sort: None,
            tree: vec![],
            platforms: vec![],
            app_ids: HashMap::new(),
            tree_rev: 0,
            tree_pending: 0,
            show_sidebar: true,
            follow: true,
            focus_search: false,
            test_command_copied: Flash::default(),
        }
    }

    pub fn refresh_visible(&mut self) {
        if self.query_src != self.filter {
            self.query = Query::parse(&self.filter);
            self.query_src = self.filter.clone();
        }
        let st = self.store.lock();
        let key = (st.rev(), self.filter.clone(), self.sort);
        if key == self.vis_key {
            return;
        }
        self.visible = st
            .flows
            .iter()
            .enumerate()
            .filter(|(_, f)| self.query.matches(f))
            .map(|(i, _)| i)
            .collect();
        if let Some((col, asc)) = self.sort {
            let fl = &st.flows;
            self.visible.sort_by(|&a, &b| {
                let o = sort_cmp(col, &fl[a], &fl[b]);
                if asc { o } else { o.reverse() }
            });
        }
        self.vis_key = key;
    }

    pub fn refresh_tree(&mut self) {
        let st = self.store.lock();
        if self.tree_rev == st.rev() {
            return;
        }
        type Counts = BTreeMap<String, BTreeMap<String, usize>>;
        let mut host: Counts = BTreeMap::new();
        let mut by_platform: BTreeMap<String, Counts> = BTreeMap::new();
        let mut ids = HashMap::new();
        let mut pending = 0;
        for f in &st.flows {
            // A flow lands under its process name first and moves into its platform folder once the
            // identity resolves, so hold it back briefly instead of letting the sidebar jump.
            if f.app_id.is_none() && f.started.elapsed().is_ok_and(|e| e < IDENTITY_GRACE) {
                pending += 1;
                continue;
            }
            let platform = f.app_id.as_ref().map(|i| i.platform.as_str()).unwrap_or_default();
            if let Some(id) = f.display_identity() {
                // identities without an icon (lookup misses) must not shadow a real one
                let slot = ids.entry(f.app_label()).or_insert_with(|| id.clone().into_owned());
                if slot.icon_key.is_empty() && !id.icon_key.is_empty() {
                    *slot = id.into_owned();
                }
            }
            let m = if platform.is_empty() { &mut host } else { by_platform.entry(platform.to_string()).or_default() };
            *m.entry(f.app_label()).or_default().entry(f.hostname().to_string()).or_default() += 1;
        }
        let by_volume = |m: Counts| -> Tree {
            let mut t: Tree = m.into_iter().map(|(a, h)| (a, h.into_iter().collect())).collect();
            t.sort_by_key(|(_, h)| std::cmp::Reverse(h.iter().map(|(_, n)| n).sum::<usize>()));
            t
        };
        self.tree = by_volume(host);
        self.platforms = by_platform.into_iter().map(|(p, m)| (p, by_volume(m))).collect();
        self.app_ids = ids;
        self.tree_pending = pending;
        // left stale while flows are pending so the next frame recomputes
        if pending == 0 {
            self.tree_rev = st.rev();
        }
    }

    pub fn flow_count(&self) -> usize {
        self.store.lock().flows.len()
    }

    pub fn has_flows(&self) -> bool {
        !self.store.lock().flows.is_empty()
    }

    pub fn remove_flows(&mut self, keep: impl Fn(&Flow) -> bool) {
        let mut st = self.store.lock();
        st.retain(keep);
        if self.selected.is_some_and(|id| st.find(id).is_none()) {
            self.selected = None;
        }
    }

    pub fn toggle_token(&mut self, tok: &str) {
        let parts: Vec<&str> = self.filter.split_whitespace().collect();
        self.filter = if parts.contains(&tok) {
            parts
                .into_iter()
                .filter(|p| *p != tok)
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            format!("{} {tok}", self.filter).trim().to_string()
        };
    }

    pub fn token_on(&self, tok: &str) -> bool {
        self.filter.split_whitespace().any(|p| p == tok)
    }

    // picking the active filter again clears it
    pub fn set_filter(&mut self, q: String) {
        self.filter = if self.filter.trim() == q {
            String::new()
        } else {
            q
        };
    }

    pub fn cycle_sort(&mut self, col: usize) {
        self.sort = match self.sort {
            Some((sc, true)) if sc == col => Some((col, false)),
            Some((sc, false)) if sc == col => None,
            _ => Some((col, true)),
        };
    }

    pub fn handle_toolbar(&mut self, intent: ToolbarIntent) {
        match intent {
            ToolbarIntent::ClearUnpinned => self.remove_flows(|f| f.pinned),
            ToolbarIntent::ClearAll => self.remove_flows(|_| false),
            ToolbarIntent::ToggleCapture => {
                let mut c = self.cfg.lock().unwrap();
                c.capture_paused = !c.capture_paused;
            }
            ToolbarIntent::ToggleToken(t) => self.toggle_token(t),
            ToolbarIntent::LoadQuery(q) => self.filter = q,
            ToolbarIntent::SaveQuery => {
                let cur = self.filter.trim().to_string();
                if cur.is_empty() {
                    return;
                }
                let mut cfg = self.cfg.lock().unwrap();
                if !cfg.saved.iter().any(|(_, q)| *q == cur) {
                    cfg.saved.push((cur.clone(), cur));
                }
            }
            ToolbarIntent::DeleteQuery(i) => {
                let mut cfg = self.cfg.lock().unwrap();
                if i < cfg.saved.len() {
                    cfg.saved.remove(i);
                }
            }
            ToolbarIntent::SetColumn(c, on) => {
                let mut cfg = self.cfg.lock().unwrap();
                cfg.hidden_cols.retain(|h| h != c);
                if !on {
                    cfg.hidden_cols.push(c.to_string());
                }
            }
            ToolbarIntent::Compose
            | ToolbarIntent::About
            | ToolbarIntent::Settings
            | ToolbarIntent::ShowBreakpoint
            | ToolbarIntent::ShowScope
            | ToolbarIntent::Onboarding
            | ToolbarIntent::Session(_)
            | ToolbarIntent::Device(_) => {}
        }
    }

    pub fn handle_row(&mut self, intent: RowIntent, status: &mut Status) -> Option<Effect> {
        match intent {
            RowIntent::Select(id) => self.selected = Some(id),
            RowIntent::FilterApp(app) => {
                let term = if app.contains(char::is_whitespace) {
                    format!("app:\"{app}\"")
                } else {
                    format!("app:{app}")
                };
                self.filter = if self.filter.trim() == term { String::new() } else { term };
            }
            RowIntent::Sort(c) => self.cycle_sort(c),
            RowIntent::ClearFilter => self.filter.clear(),
            RowIntent::Pin(id) => {
                let mut st = self.store.lock();
                if let Some(f) = st.find_mut(id) {
                    f.pinned = !f.pinned;
                    st.touch();
                }
            }
            RowIntent::Color(id, c) => {
                let mut st = self.store.lock();
                if let Some(f) = st.find_mut(id) {
                    f.color = c;
                    st.touch();
                }
            }
            RowIntent::Comment(id) => {
                self.selected = Some(id);
                return Some(Effect::ShowOverview);
            }
            RowIntent::Replay(id) => return Some(Effect::OpenReplay(id)),
            RowIntent::Rule(id, kind) => return Some(Effect::RuleFromFlow(id, kind)),
            RowIntent::Mute(h) => {
                self.cfg.lock().unwrap().muted.push(h.clone());
                status.set(format!(
                    "Muted {h} — hidden from the record; traffic still flows"
                ));
            }
            RowIntent::Scope(h) => self.add_scope(h, status),
            RowIntent::ScopePath(id) => {
                let pattern = self.store.lock().find(id).map(scope_path_pattern)?;
                self.add_scope(pattern, status);
            }
            RowIntent::ProxyOnlyHost(h) => self.set_proxy_only(h, true, status),
            RowIntent::ProxyOnlyAdd(h) => self.set_proxy_only(h, false, status),
            RowIntent::Exclude(id) => {
                let pattern = self.store.lock().find(id).map(exclude_pattern)?;
                let mut c = self.cfg.lock().unwrap();
                if !c.exclude.contains(&pattern) {
                    c.exclude.push(pattern.clone());
                }
                status.set(format!(
                    "Excluding {pattern} — hidden from the record; traffic still flows"
                ));
            }
            RowIntent::NoTls(h) => {
                self.cfg.lock().unwrap().bypass_tls.push(h.clone());
                status.set(format!("{h} will be tunnelled without decryption — applies to new connections only; existing ones keep their TLS session"));
            }
            RowIntent::Delete(id) => self.remove_flows(|f| f.id != id),
            RowIntent::TestCommandCopied => self.test_command_copied.mark(),
            RowIntent::DiffBase(id) => {
                status.set(format!(
                    "#{id} is the diff base — select another request to compare"
                ));
                return Some(Effect::DiffBase(id));
            }
        }
        None
    }

    fn add_scope(&self, entry: String, status: &mut Status) {
        let mut c = self.cfg.lock().unwrap();
        if !c.scope.contains(&entry) {
            c.scope.push(entry);
        }
        c.scope_only = true;
        status.set(format!(
            "Recording only {} — other traffic still flows, unrecorded",
            c.scope.join(", ")
        ));
    }

    fn set_proxy_only(&self, host: String, replace: bool, status: &mut Status) {
        let mut c = self.cfg.lock().unwrap();
        if replace {
            c.proxy_only_hosts.clear();
        }
        if !c.proxy_only_hosts.contains(&host) {
            c.proxy_only_hosts.push(host);
        }
        c.proxy_only = true;
        status.set(format!(
            "Proxy-only: {} — applies when the system proxy is on; open connections keep their route",
            c.proxy_only_hosts.join(", ")
        ));
    }

    /// Whether scope or exclude currently narrows what is recorded.
    pub fn scope_active(&self) -> bool {
        let c = self.cfg.lock().unwrap();
        (c.scope_only && !c.scope.is_empty()) || !c.exclude.is_empty()
    }

    pub fn handle_sidebar(&mut self, intent: SidebarIntent) {
        match intent {
            SidebarIntent::Pick(q) => self.set_filter(q),
            SidebarIntent::Mute(h) => self.cfg.lock().unwrap().muted.push(h),
            SidebarIntent::Scope(h) => {
                let mut c = self.cfg.lock().unwrap();
                c.scope.push(h);
                c.scope_only = true;
            }
            SidebarIntent::NoTls(h) => self.cfg.lock().unwrap().bypass_tls.push(h),
            SidebarIntent::Unmute(m) => self.cfg.lock().unwrap().muted.retain(|x| *x != m),
        }
    }

    pub fn handle_shortcut(&mut self, intent: ShortcutIntent) -> Option<Effect> {
        match intent {
            ShortcutIntent::ToggleSidebar => self.show_sidebar = !self.show_sidebar,
            ShortcutIntent::FocusSearch => self.focus_search = true,
            ShortcutIntent::ClearFilter => self.filter.clear(),
            ShortcutIntent::Step(down) => self.step(down),
            ShortcutIntent::DeleteSelected => {
                if let Some(id) = self.selected {
                    self.remove_flows(|f| f.id != id);
                }
            }
            ShortcutIntent::ReplaySelected => return self.selected.map(Effect::OpenReplay),
            ShortcutIntent::Deselect => self.selected = None,
            ShortcutIntent::OpenSettings => return Some(Effect::OpenSettings),
        }
        None
    }

    fn step(&mut self, down: bool) {
        let st = self.store.lock();
        let pos = self
            .selected
            .and_then(|id| self.visible.iter().position(|&i| st.flows[i].id == id));
        let next = match (pos, down) {
            (None, _) => self.visible.first().copied(),
            (Some(p), true) => self
                .visible
                .get((p + 1).min(self.visible.len().saturating_sub(1)))
                .copied(),
            (Some(p), false) => self.visible.get(p.saturating_sub(1)).copied(),
        };
        if let Some(i) = next {
            self.selected = Some(st.flows[i].id);
            self.follow = false;
        }
    }

    pub fn capture_paused(&self) -> bool {
        self.cfg.lock().unwrap().capture_paused
    }

    pub fn hidden_cols(&self) -> Vec<String> {
        self.cfg.lock().unwrap().hidden_cols.clone()
    }

    pub fn saved_queries(&self) -> Vec<(String, String)> {
        self.cfg.lock().unwrap().saved.clone()
    }

    pub fn config_snapshot(&self) -> Config {
        self.cfg.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewmodels::status::NoRepaint;
    use std::sync::{Arc, Mutex};

    fn setup(n: u64) -> (CaptureVm, Status) {
        let store = FlowStore::default();
        for id in 1..=n {
            let mut f = Flow::sample(id);
            f.host = format!("h{}.com", id % 2);
            f.status = Some(if id % 2 == 0 { 500 } else { 200 });
            store.lock().push(f);
        }
        let vm = CaptureVm::new(
            store,
            Arc::new(Mutex::new(Config::default())),
            IconStore::new(std::env::temp_dir(), Arc::new(NoRepaint)),
        );
        (vm, Status::new(String::new(), Arc::new(NoRepaint)))
    }

    fn ids(vm: &CaptureVm) -> Vec<u64> {
        let st = vm.store.lock();
        vm.visible.iter().map(|&i| st.flows[i].id).collect()
    }

    #[test]
    fn filter_narrows_visible_rows() {
        let (mut vm, _) = setup(4);
        vm.refresh_visible();
        assert_eq!(ids(&vm), vec![1, 2, 3, 4]);
        vm.filter = "status:>=500".into();
        vm.refresh_visible();
        assert_eq!(ids(&vm), vec![2, 4]);
    }

    #[test]
    fn clear_filter_intent_restores_all_requests() {
        let (mut vm, mut status) = setup(4);
        vm.filter = "status:>=500".into();
        vm.refresh_visible();
        assert_eq!(ids(&vm), vec![2, 4]);
        vm.handle_row(RowIntent::ClearFilter, &mut status);
        vm.refresh_visible();
        assert_eq!(vm.filter, "");
        assert_eq!(ids(&vm), vec![1, 2, 3, 4]);
    }

    #[test]
    fn sort_cycles_asc_desc_off_and_orders_rows() {
        let (mut vm, _) = setup(3);
        vm.cycle_sort(0);
        assert_eq!(vm.sort, Some((0, true)));
        vm.cycle_sort(0);
        assert_eq!(vm.sort, Some((0, false)));
        vm.refresh_visible();
        assert_eq!(ids(&vm), vec![3, 2, 1]);
        vm.cycle_sort(0);
        assert_eq!(vm.sort, None);
        vm.cycle_sort(4);
        assert_eq!(vm.sort, Some((4, true)));
    }

    #[test]
    fn new_flow_refreshes_visible_via_store_rev() {
        let (mut vm, _) = setup(1);
        vm.refresh_visible();
        vm.store.lock().push(Flow::sample(2));
        vm.refresh_visible();
        assert_eq!(ids(&vm), vec![1, 2]);
    }

    #[test]
    fn toggle_token_adds_and_removes() {
        let (mut vm, _) = setup(0);
        vm.toggle_token("is:error");
        vm.toggle_token("is:json");
        assert_eq!(vm.filter, "is:error is:json");
        vm.toggle_token("is:error");
        assert_eq!(vm.filter, "is:json");
        assert!(vm.token_on("is:json") && !vm.token_on("is:error"));
    }

    #[test]
    fn set_filter_toggles_off_when_repeated() {
        let (mut vm, _) = setup(0);
        vm.set_filter("host:a".into());
        assert_eq!(vm.filter, "host:a");
        vm.set_filter("host:a".into());
        assert_eq!(vm.filter, "");
    }

    #[test]
    fn mute_intent_updates_policy_and_status_but_not_flows() {
        let (mut vm, mut status) = setup(2);
        vm.handle_row(RowIntent::Mute("h1.com".into()), &mut status);
        assert!(!vm.config_snapshot().captured("https://h1.com/"));
        assert!(status.text.starts_with("Muted h1.com"));
        assert_eq!(vm.flow_count(), 2);
    }

    #[test]
    fn proxy_only_intents_replace_or_extend_the_list() {
        let (mut vm, mut status) = setup(0);
        vm.handle_row(RowIntent::ProxyOnlyAdd("a.com".into()), &mut status);
        vm.handle_row(RowIntent::ProxyOnlyAdd("b.com".into()), &mut status);
        vm.handle_row(RowIntent::ProxyOnlyAdd("a.com".into()), &mut status);
        let c = vm.config_snapshot();
        assert!(c.proxy_only && c.proxy_only_hosts == ["a.com", "b.com"]);
        vm.handle_row(RowIntent::ProxyOnlyHost("c.com".into()), &mut status);
        assert_eq!(vm.config_snapshot().proxy_only_hosts, ["c.com"]);
        assert!(status.text.contains("c.com"));
        assert!(vm.config_snapshot().captured("https://b.com/"), "recording is untouched");
    }

    #[test]
    fn scope_intent_turns_scope_on() {
        let (mut vm, mut status) = setup(0);
        vm.handle_row(RowIntent::Scope("a.com".into()), &mut status);
        let c = vm.config_snapshot();
        assert!(c.scope_only && c.captured("https://a.com/") && !c.captured("https://b.com/"));
        assert!(status.text.contains("a.com"));
    }

    #[test]
    fn no_tls_intent_adds_bypass() {
        let (mut vm, mut status) = setup(0);
        vm.handle_row(RowIntent::NoTls("a.com".into()), &mut status);
        assert!(!vm.config_snapshot().should_decrypt("a.com"));
        assert!(status.text.contains("new connections only"));
    }

    #[test]
    fn pin_color_and_delete() {
        let (mut vm, mut status) = setup(2);
        vm.handle_row(RowIntent::Pin(1), &mut status);
        vm.handle_row(RowIntent::Color(1, Some(2)), &mut status);
        {
            let st = vm.store.lock();
            let f = st.find(1).unwrap();
            assert!(f.pinned && f.color == Some(2));
        }
        vm.selected = Some(2);
        vm.handle_row(RowIntent::Delete(2), &mut status);
        assert_eq!(vm.flow_count(), 1);
        assert_eq!(vm.selected, None);
    }

    #[test]
    fn cross_vm_row_intents_return_effects() {
        let (mut vm, mut status) = setup(1);
        assert!(matches!(
            vm.handle_row(RowIntent::Replay(1), &mut status),
            Some(Effect::OpenReplay(1))
        ));
        assert!(matches!(
            vm.handle_row(RowIntent::DiffBase(1), &mut status),
            Some(Effect::DiffBase(1))
        ));
        assert!(status.text.starts_with("#1 is the diff base"));
        assert!(matches!(
            vm.handle_row(RowIntent::Comment(1), &mut status),
            Some(Effect::ShowOverview)
        ));
        assert_eq!(vm.selected, Some(1));
    }

    #[test]
    fn arrow_stepping_selects_and_stops_follow() {
        let (mut vm, _) = setup(3);
        vm.refresh_visible();
        vm.handle_shortcut(ShortcutIntent::Step(true));
        assert_eq!(vm.selected, Some(1));
        assert!(!vm.follow);
        vm.handle_shortcut(ShortcutIntent::Step(true));
        vm.handle_shortcut(ShortcutIntent::Step(true));
        vm.handle_shortcut(ShortcutIntent::Step(true));
        assert_eq!(vm.selected, Some(3));
        vm.handle_shortcut(ShortcutIntent::Step(false));
        assert_eq!(vm.selected, Some(2));
    }

    #[test]
    fn settings_shortcut_opens_settings() {
        let (mut vm, _) = setup(0);
        assert!(matches!(
            vm.handle_shortcut(ShortcutIntent::OpenSettings),
            Some(Effect::OpenSettings)
        ));
    }

    #[test]
    fn delete_and_replay_shortcuts_need_a_selection() {
        let (mut vm, _) = setup(2);
        assert!(vm.handle_shortcut(ShortcutIntent::ReplaySelected).is_none());
        vm.handle_shortcut(ShortcutIntent::DeleteSelected);
        assert_eq!(vm.flow_count(), 2);
        vm.selected = Some(1);
        assert!(matches!(
            vm.handle_shortcut(ShortcutIntent::ReplaySelected),
            Some(Effect::OpenReplay(1))
        ));
        vm.handle_shortcut(ShortcutIntent::DeleteSelected);
        assert_eq!(vm.flow_count(), 1);
        assert_eq!(vm.selected, None);
    }

    #[test]
    fn clear_unpinned_keeps_pinned() {
        let (mut vm, mut status) = setup(3);
        vm.handle_row(RowIntent::Pin(2), &mut status);
        vm.handle_toolbar(ToolbarIntent::ClearUnpinned);
        assert_eq!(
            vm.store
                .lock()
                .flows
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn clear_all_removes_pinned_too() {
        let (mut vm, mut status) = setup(3);
        vm.handle_row(RowIntent::Pin(2), &mut status);
        vm.handle_toolbar(ToolbarIntent::ClearAll);
        assert_eq!(vm.flow_count(), 0);
        assert_eq!(vm.selected, None);
    }

    #[test]
    fn saved_queries_dedupe_and_delete() {
        let (mut vm, _) = setup(0);
        vm.filter = "is:error".into();
        vm.handle_toolbar(ToolbarIntent::SaveQuery);
        vm.handle_toolbar(ToolbarIntent::SaveQuery);
        assert_eq!(vm.config_snapshot().saved.len(), 1);
        vm.handle_toolbar(ToolbarIntent::DeleteQuery(0));
        assert!(vm.config_snapshot().saved.is_empty());
    }

    #[test]
    fn column_visibility_and_capture_pause() {
        let (mut vm, _) = setup(0);
        vm.handle_toolbar(ToolbarIntent::SetColumn("Host", false));
        vm.handle_toolbar(ToolbarIntent::SetColumn("Host", false));
        assert_eq!(vm.config_snapshot().hidden_cols, vec!["Host".to_string()]);
        vm.handle_toolbar(ToolbarIntent::SetColumn("Host", true));
        assert!(vm.config_snapshot().hidden_cols.is_empty());
        vm.handle_toolbar(ToolbarIntent::ToggleCapture);
        assert!(vm.config_snapshot().capture_paused);
    }

    #[test]
    fn filter_app_toggles_and_quotes_names_with_spaces() {
        let (mut vm, mut status) = setup(1);
        vm.handle_row(RowIntent::FilterApp("Google Chrome".into()), &mut status);
        assert_eq!(vm.filter, "app:\"Google Chrome\"");
        vm.handle_row(RowIntent::FilterApp("Google Chrome".into()), &mut status);
        assert_eq!(vm.filter, "");
    }

    #[test]
    fn tree_groups_by_app_then_host_sorted_by_volume() {
        let (mut vm, _) = setup(0);
        for (id, app, host) in [
            (1, "curl", "a.com"),
            (2, "curl", "a.com"),
            (3, "curl", "b.com"),
            (4, "node", "c.com"),
        ] {
            let mut f = Flow::sample(id);
            f.app = app.into();
            f.host = host.into();
            vm.store.lock().push(f);
        }
        vm.refresh_tree();
        assert_eq!(
            vm.tree[0],
            (
                "curl".to_string(),
                vec![("a.com".to_string(), 2), ("b.com".to_string(), 1)]
            )
        );
        assert_eq!(vm.tree[1].0, "node");
    }

    #[test]
    fn platform_filter_scopes_app_and_host_terms() {
        let (mut vm, _) = setup(0);
        for (id, platform) in [(1, "iOS Simulator"), (2, "")] {
            let mut f = Flow::sample(id);
            f.app = "Safari".into();
            f.app_id = Some(AppIdentity { platform: platform.into(), ..Default::default() });
            vm.store.lock().push(f);
        }
        vm.set_filter("platform:=\"iOS Simulator\" app:=\"Safari\"".into());
        vm.refresh_visible();
        assert_eq!(vm.visible.len(), 1);
    }

    #[test]
    fn simulator_safari_page_loads_are_labelled_safari_but_webviews_are_not() {
        let mk = |ua: &str| {
            let mut f = Flow::sample(1);
            f.app = "iOS Simulator".into();
            f.app_id = Some(AppIdentity { name: "iOS Simulator".into(), platform: "iOS Simulator".into(), ..Default::default() });
            f.request.headers.push(("user-agent".into(), ua.into()));
            f
        };
        let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/27.0 Mobile/15E148 Safari/604.1";
        let webview = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148";
        assert_eq!(mk(safari).app_label(), "Safari");
        assert_eq!(mk(webview).app_label(), "iOS Simulator");
    }

    #[test]
    fn unresolved_new_flows_stay_out_of_the_tree_until_the_grace_passes() {
        let (mut vm, _) = setup(0);
        let mut f = Flow::sample(1);
        f.app = "curl".into();
        f.started = std::time::SystemTime::now();
        vm.store.lock().push(f);
        vm.refresh_tree();
        assert!(vm.tree.is_empty());
        assert_eq!(vm.tree_pending, 1);
        vm.store.lock().find_mut(1).unwrap().started -= std::time::Duration::from_secs(10);
        vm.refresh_tree();
        assert_eq!(vm.tree.len(), 1);
        assert_eq!(vm.tree_pending, 0);
    }

    #[test]
    fn simulator_apps_get_their_own_platform_folder() {
        let (mut vm, _) = setup(0);
        for (id, app, platform) in [(1, "curl", ""), (2, "Maps", "iOS Simulator"), (3, "Maps", "iOS Simulator")] {
            let mut f = Flow::sample(id);
            f.app = app.into();
            f.app_id = Some(AppIdentity { name: app.into(), platform: platform.into(), ..Default::default() });
            vm.store.lock().push(f);
        }
        vm.refresh_tree();
        assert_eq!(vm.tree.len(), 1);
        assert_eq!(vm.platforms.len(), 1);
        assert_eq!(vm.platforms[0].0, "iOS Simulator");
        assert_eq!(vm.platforms[0].1[0].0, "Maps");
    }

    #[test]
    fn scope_path_and_exclude_intents_use_the_flow_url() {
        let (mut vm, mut status) = setup(1);
        {
            let mut st = vm.store.lock();
            let f = st.find_mut(1).unwrap();
            f.host = "api.a.com".into();
            f.path = "/v2/users/7?x=1".into();
        }
        vm.handle_row(RowIntent::ScopePath(1), &mut status);
        vm.handle_row(RowIntent::ScopePath(1), &mut status);
        let c = vm.config_snapshot();
        assert_eq!(c.scope, vec!["api.a.com/v2/users/*".to_string()]);
        assert!(c.scope_only && c.captured("https://api.a.com/v2/users/9"));
        assert!(!c.captured("https://api.a.com/v2/orders/9"));
        assert!(vm.scope_active());

        vm.handle_row(RowIntent::Exclude(1), &mut status);
        let c = vm.config_snapshot();
        assert_eq!(c.exclude, vec!["api.a.com/v2/users/7".to_string()]);
        assert!(!c.captured("https://api.a.com/v2/users/7?y=2"));
        assert!(c.captured("https://api.a.com/v2/users/8"));
        assert_eq!(vm.flow_count(), 1);
    }

    #[test]
    fn scope_active_reflects_scope_and_exclude() {
        let (vm, _) = setup(0);
        assert!(!vm.scope_active());
        vm.cfg.lock().unwrap().scope_only = true;
        assert!(!vm.scope_active());
        vm.cfg.lock().unwrap().exclude.push("x.com".into());
        assert!(vm.scope_active());
    }

    #[test]
    fn sidebar_intents_edit_config() {
        let (mut vm, _) = setup(0);
        vm.handle_sidebar(SidebarIntent::Mute("x.com".into()));
        assert!(vm.config_snapshot().muted.contains(&"x.com".to_string()));
        vm.handle_sidebar(SidebarIntent::Unmute("x.com".into()));
        assert!(vm.config_snapshot().muted.is_empty());
        vm.handle_sidebar(SidebarIntent::Pick("host:a".into()));
        assert_eq!(vm.filter, "host:a");
    }

    #[test]
    fn safari_page_loads_from_the_shared_sim_process_get_safari_icon() {
        let (mut vm, _) = setup(0);
        let mut f = Flow::sample(1);
        f.app = "iOS Simulator".into();
        f.app_id = Some(AppIdentity {
            name: "iOS Simulator".into(),
            platform: "iOS Simulator".into(),
            safari_icon_key: "safari-1".into(),
            ..Default::default()
        });
        f.request.headers.push(("user-agent".into(), "Mozilla/5.0 (iPhone) Mobile/15E148 Safari/604.1".into()));
        f.started -= std::time::Duration::from_secs(10);
        vm.store.lock().push(f);
        vm.refresh_tree();
        assert_eq!(vm.app_ids["Safari"].icon_key, "safari-1");
    }
}
