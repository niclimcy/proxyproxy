use super::intent::{DetailIntent, Effect};
use crate::domain::flow::{Flow, Message};
use crate::services::decode;
use crate::services::decoders::{ModeId, PRETTY};
use crate::services::flow_store::FlowStore;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum DTab {
    Split,
    Overview,
    Diff,
    Messages,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Side {
    Headers,
    Body,
    Params,
    Raw,
}

type DiffKey = (u64, u64, usize);

pub struct DetailVm {
    pub store: FlowStore,
    pub tab: DTab,
    pub req_side: Side,
    pub res_side: Side,
    pub req_mode: ModeId,
    pub res_mode: ModeId,
    pub ws_sel: Option<usize>,
    pub diff_base: Option<u64>,
    pub diff_only_changes: bool,
    diff_cache: Option<(DiffKey, Vec<(char, String)>)>,
}

pub struct Overview {
    pub max_ms: u128,
    pub same: Vec<Flow>,
}

fn headers_text(m: &Message) -> String {
    m.headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn request_text(x: &Flow) -> String {
    format!(
        "{} {}\n{}\n\n{}",
        x.method,
        x.path,
        headers_text(&x.request),
        decode::pretty_text(&x.request)
    )
}

fn response_text(x: &Flow) -> String {
    match &x.response {
        Some(r) => format!(
            "# status {}\n{}\n\n{}",
            x.status.unwrap_or(0),
            headers_text(r),
            decode::pretty_text(r)
        ),
        None => String::new(),
    }
}

fn flow_report(x: &Flow) -> String {
    format!(
        "## request\n{}\n## response\n{}",
        request_text(x),
        response_text(x)
    )
}

impl DetailVm {
    pub fn new(store: FlowStore) -> Self {
        Self {
            store,
            tab: DTab::Split,
            req_side: Side::Headers,
            res_side: Side::Body,
            req_mode: PRETTY,
            res_mode: PRETTY,
            ws_sel: None,
            diff_base: None,
            diff_only_changes: true,
            diff_cache: None,
        }
    }

    pub fn flow(&self, id: u64) -> Option<Flow> {
        self.store.lock().find(id).cloned()
    }

    pub fn on_select(&mut self) {
        self.ws_sel = None;
    }

    pub fn overview(&self, f: &Flow) -> Overview {
        let st = self.store.lock();
        let key = f.endpoint_key();
        Overview {
            max_ms: st
                .flows
                .iter()
                .filter_map(|x| x.duration_ms)
                .max()
                .unwrap_or(1),
            same: st
                .flows
                .iter()
                .filter(|x| x.id != f.id && x.endpoint_key() == key)
                .cloned()
                .collect(),
        }
    }

    // explicit base first, else the previous answered call to the same endpoint
    pub fn diff_base_for(&self, f: &Flow) -> Option<Flow> {
        let st = self.store.lock();
        self.diff_base
            .filter(|b| *b != f.id)
            .and_then(|b| st.find(b))
            .or_else(|| {
                st.flows.iter().rev().find(|x| {
                    x.id < f.id && x.endpoint_key() == f.endpoint_key() && x.response.is_some()
                })
            })
            .cloned()
    }

    pub fn diff_lines(&mut self, base: &Flow, f: &Flow) -> &[(char, String)] {
        let key = (
            base.id,
            f.id,
            (f.res_size() + base.res_size()) ^ f.request.body.len(),
        );
        if self.diff_cache.as_ref().map(|(k, _)| *k) != Some(key) {
            self.diff_cache = Some((key, decode::diff_text(&flow_report(base), &flow_report(f))));
        }
        &self.diff_cache.as_ref().unwrap().1
    }

    pub fn handle(&mut self, intent: DetailIntent) -> Option<Effect> {
        match intent {
            DetailIntent::Replay(id) => return Some(Effect::OpenReplay(id)),
            DetailIntent::TogglePin(id) => {
                let mut st = self.store.lock();
                if let Some(f) = st.find_mut(id) {
                    f.pinned = !f.pinned;
                    st.touch();
                }
            }
            DetailIntent::SetComment(id, c) => {
                let mut st = self.store.lock();
                if let Some(f) = st.find_mut(id) {
                    f.comment = c;
                    st.touch();
                }
            }
            DetailIntent::DiffAgainst(id) => self.use_as_base(id),
            DetailIntent::ClearBase => self.diff_base = None,
            DetailIntent::SelectWs(i) => self.ws_sel = Some(i),
            DetailIntent::Close => return Some(Effect::CloseDetail),
        }
        None
    }

    pub fn use_as_base(&mut self, id: u64) {
        self.diff_base = Some(id);
        self.tab = DTab::Diff;
    }

    pub fn show_overview(&mut self) {
        self.tab = DTab::Overview;
    }
}

pub fn side_tabs(cur: &mut Side, has_params: bool) -> Vec<(Side, &'static str)> {
    let mut items = vec![(Side::Headers, "Headers"), (Side::Body, "Body")];
    if has_params {
        items.push((Side::Params, "Params"));
    }
    items.push((Side::Raw, "Raw"));
    if !items.iter().any(|(s, _)| s == cur) {
        *cur = Side::Body;
    }
    items
}

pub fn raw_text(first: &str, m: &Message) -> String {
    let mut raw = format!("{first}\n");
    for (k, v) in &m.headers {
        raw.push_str(&format!("{k}: {v}\n"));
    }
    raw.push('\n');
    raw.push_str(&String::from_utf8_lossy(&m.body));
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(id: u64, path: &str, body: &str) -> Flow {
        let mut f = Flow::sample(id);
        f.path = path.into();
        f.status = Some(200);
        f.duration_ms = Some(id as u128 * 10);
        f.response = Some(Message {
            headers: vec![],
            body: body.as_bytes().to_vec(),
        });
        f
    }

    fn vm_with(flows: Vec<Flow>) -> DetailVm {
        let store = FlowStore::default();
        for f in flows {
            store.lock().push(f);
        }
        DetailVm::new(store)
    }

    #[test]
    fn diff_defaults_to_previous_answered_call_on_same_endpoint() {
        let vm = vm_with(vec![
            flow(1, "/a", "x"),
            flow(2, "/b", "y"),
            flow(3, "/a", "z"),
        ]);
        let f3 = vm.flow(3).unwrap();
        assert_eq!(vm.diff_base_for(&f3).unwrap().id, 1);
        assert!(vm.diff_base_for(&vm.flow(1).unwrap()).is_none());
    }

    #[test]
    fn explicit_base_wins_and_self_base_is_ignored() {
        let mut vm = vm_with(vec![
            flow(1, "/a", "x"),
            flow(2, "/b", "y"),
            flow(3, "/a", "z"),
        ]);
        vm.use_as_base(2);
        assert_eq!(vm.tab, DTab::Diff);
        assert_eq!(vm.diff_base_for(&vm.flow(3).unwrap()).unwrap().id, 2);
        assert_eq!(vm.diff_base_for(&vm.flow(2).unwrap()).map(|f| f.id), None);
        vm.handle(DetailIntent::ClearBase);
        assert_eq!(vm.diff_base_for(&vm.flow(3).unwrap()).unwrap().id, 1);
    }

    #[test]
    fn diff_lines_mark_changes_and_cache_by_key() {
        let mut vm = vm_with(vec![flow(1, "/a", "old"), flow(2, "/a", "new")]);
        let (a, b) = (vm.flow(1).unwrap(), vm.flow(2).unwrap());
        let lines = vm.diff_lines(&a, &b).to_vec();
        assert!(lines.contains(&('-', "old".into())) && lines.contains(&('+', "new".into())));
        assert_eq!(vm.diff_lines(&a, &b), &lines[..]);
        assert!(vm.diff_lines(&a, &a).iter().all(|(t, _)| *t == ' '));
    }

    #[test]
    fn overview_lists_other_calls_and_max_duration() {
        let vm = vm_with(vec![
            flow(1, "/a", ""),
            flow(2, "/a", ""),
            flow(3, "/b", ""),
        ]);
        let o = vm.overview(&vm.flow(1).unwrap());
        assert_eq!(o.max_ms, 30);
        assert_eq!(o.same.iter().map(|f| f.id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn pin_and_comment_update_store_and_bump_rev() {
        let mut vm = vm_with(vec![flow(1, "/a", "")]);
        let r = vm.store.lock().rev();
        vm.handle(DetailIntent::TogglePin(1));
        vm.handle(DetailIntent::SetComment(1, "note".into()));
        let f = vm.flow(1).unwrap();
        assert!(f.pinned && f.comment == "note");
        assert!(vm.store.lock().rev() > r);
        vm.handle(DetailIntent::TogglePin(1));
        assert!(!vm.flow(1).unwrap().pinned);
    }

    #[test]
    fn replay_intent_becomes_effect_and_ws_selection_resets() {
        let mut vm = vm_with(vec![]);
        assert!(matches!(
            vm.handle(DetailIntent::Replay(4)),
            Some(Effect::OpenReplay(4))
        ));
        vm.handle(DetailIntent::SelectWs(2));
        assert_eq!(vm.ws_sel, Some(2));
        vm.on_select();
        assert_eq!(vm.ws_sel, None);
    }

    #[test]
    fn side_tabs_fall_back_to_body_when_params_vanish() {
        let mut s = Side::Params;
        let t = side_tabs(&mut s, false);
        assert_eq!(s, Side::Body);
        assert!(!t.iter().any(|(x, _)| *x == Side::Params));
        let mut s = Side::Params;
        side_tabs(&mut s, true);
        assert_eq!(s, Side::Params);
    }

    #[test]
    fn raw_text_has_start_line_headers_blank_line_body() {
        let m = Message {
            headers: vec![("a".into(), "1".into())],
            body: b"hi".to_vec(),
        };
        assert_eq!(raw_text("GET / HTTP/1.1", &m), "GET / HTTP/1.1\na: 1\n\nhi");
    }
}
