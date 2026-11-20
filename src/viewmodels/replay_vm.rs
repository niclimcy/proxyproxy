use crate::domain::flow::{self, Flow, Message};
use crate::domain::repaint::Repaint;
use crate::services::decoders::{ModeId, PRETTY};
use crate::services::flow_store::FlowStore;
use crate::services::replay::{self, Sent};
use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};

pub struct ReplayDraft {
    pub id: u64,
    pub method: String,
    pub url: String,
    pub headers: String,
    pub body: String,
    pub result: Option<Result<Sent, String>>,
    pub sending: bool,
    pub open: bool,
    pub mode: ModeId,
    pub prev: Option<Message>,
    pub show_diff: bool,
    pub source: Option<u64>,
}

type Done = (u64, Result<Sent, String>);

pub struct ReplayVm {
    pub store: FlowStore,
    pub drafts: Vec<ReplayDraft>,
    next_win: u64,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    repaint: Arc<dyn Repaint>,
}

pub fn hdr_text(h: &[(String, String)]) -> String {
    h.iter().filter(|(k, _)| !matches!(k.to_ascii_lowercase().as_str(), "content-length" | "host")).map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n")
}

pub fn parse_headers(text: &str) -> Vec<(String, String)> {
    text.lines().filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().into(), v.trim().into())).collect()
}

fn split_url(url: &str) -> (&str, &str, &str) {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let (host, path) = rest.find('/').map(|i| (&rest[..i], &rest[i..])).unwrap_or((rest, "/"));
    (scheme, host, path)
}

fn flow_from_draft(d: &ReplayDraft, s: &Sent) -> Flow {
    let (scheme, host, path) = split_url(&d.url);
    Flow {
        id: flow::next_id(),
        method: d.method.trim().to_uppercase(),
        scheme: scheme.into(),
        host: host.into(),
        path: path.into(),
        version: "HTTP/1.1".into(),
        started: std::time::SystemTime::now(),
        started_at: std::time::Instant::now(),
        request: Message { headers: parse_headers(&d.headers), body: d.body.clone().into_bytes() },
        status: Some(s.status),
        response: Some(s.response.clone()),
        duration_ms: Some(s.ms),
        tag: "Replayed".into(),
        app: "proxyproxy".into(),
        pinned: false,
        comment: String::new(),
        color: None,
        is_ws: false,
        ws: vec![],
        error: None,
    }
}

impl ReplayVm {
    pub fn new(store: FlowStore, repaint: Arc<dyn Repaint>) -> Self {
        let (tx, rx) = channel();
        Self { store, drafts: vec![], next_win: 1, tx, rx, repaint }
    }

    fn push(&mut self, mut d: ReplayDraft) {
        d.id = self.next_win;
        self.next_win += 1;
        self.drafts.push(d);
    }

    pub fn open_replay(&mut self, id: u64) {
        let Some(f) = self.store.lock().find(id).cloned() else { return };
        self.push(ReplayDraft {
            id: 0,
            method: f.method.clone(),
            url: f.url(),
            headers: hdr_text(&f.request.headers),
            body: f.request.text(),
            result: None,
            sending: false,
            open: true,
            mode: PRETTY,
            prev: f.response.clone(),
            show_diff: false,
            source: Some(f.id),
        });
    }

    pub fn open_compose(&mut self) {
        self.push(ReplayDraft {
            id: 0,
            method: "GET".into(),
            url: "https://".into(),
            headers: "Accept: */*".into(),
            body: String::new(),
            result: None,
            sending: false,
            open: true,
            mode: PRETTY,
            prev: None,
            show_diff: false,
            source: None,
        });
    }

    pub fn send(&mut self, id: u64) {
        let Some(d) = self.drafts.iter_mut().find(|d| d.id == id) else { return };
        d.sending = true;
        d.result = None;
        let (m, u, h, b) = (d.method.clone(), d.url.clone(), parse_headers(&d.headers), d.body.clone().into_bytes());
        let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
        std::thread::spawn(move || {
            let _ = tx.send((id, replay::send(&m, &u, &h, &b)));
            repaint.request();
        });
    }

    // successful replays are recorded as flows
    pub fn poll(&mut self) {
        while let Ok((id, res)) = self.rx.try_recv() {
            self.finish(id, res);
        }
    }

    fn finish(&mut self, id: u64, res: Result<Sent, String>) {
        let Some(d) = self.drafts.iter_mut().find(|d| d.id == id) else { return };
        d.sending = false;
        if let Ok(s) = &res {
            self.store.lock().push(flow_from_draft(d, s));
        }
        d.result = Some(res);
    }

    pub fn prune_closed(&mut self) {
        self.drafts.retain(|d| d.open);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewmodels::status::NoRepaint;

    fn vm() -> ReplayVm {
        ReplayVm::new(FlowStore::default(), Arc::new(NoRepaint))
    }

    fn sent(status: u16) -> Sent {
        Sent { status, ms: 7, response: Message { headers: vec![], body: b"ok".to_vec() } }
    }

    #[test]
    fn open_replay_copies_flow_and_drops_hop_headers() {
        let mut vm = vm();
        let mut f = Flow::sample(5);
        f.method = "POST".into();
        f.path = "/p?q=1".into();
        f.request = Message { headers: vec![("Host".into(), "a.com".into()), ("Content-Length".into(), "2".into()), ("X-A".into(), "1".into())], body: b"hi".to_vec() };
        f.response = Some(Message::default());
        vm.store.lock().push(f);
        vm.open_replay(5);
        let d = &vm.drafts[0];
        assert_eq!((d.method.as_str(), d.url.as_str(), d.headers.as_str(), d.body.as_str()), ("POST", "https://a.com/p?q=1", "X-A: 1", "hi"));
        assert_eq!(d.source, Some(5));
        assert!(d.prev.is_some() && d.open);
        vm.open_replay(99);
        assert_eq!(vm.drafts.len(), 1);
    }

    #[test]
    fn compose_gets_unique_window_ids() {
        let mut vm = vm();
        vm.open_compose();
        vm.open_compose();
        assert_ne!(vm.drafts[0].id, vm.drafts[1].id);
        assert_eq!(vm.drafts[0].url, "https://");
    }

    #[test]
    fn successful_send_is_recorded_as_replayed_flow() {
        let mut vm = vm();
        vm.open_compose();
        let id = vm.drafts[0].id;
        vm.drafts[0].method = " post ".into();
        vm.drafts[0].url = "http://b.com:8080/x?y=2".into();
        vm.drafts[0].headers = "A: 1\nbad line".into();
        vm.drafts[0].sending = true;
        vm.finish(id, Ok(sent(201)));
        let st = vm.store.lock();
        let f = &st.flows[0];
        assert_eq!((f.method.as_str(), f.scheme.as_str(), f.host.as_str(), f.path.as_str()), ("POST", "http", "b.com:8080", "/x?y=2"));
        assert_eq!((f.tag.as_str(), f.app.as_str(), f.status), ("Replayed", "proxyproxy", Some(201)));
        assert_eq!(f.request.headers, vec![("A".to_string(), "1".to_string())]);
        drop(st);
        assert!(!vm.drafts[0].sending && matches!(vm.drafts[0].result, Some(Ok(_))));
    }

    #[test]
    fn failed_send_is_not_recorded() {
        let mut vm = vm();
        vm.open_compose();
        let id = vm.drafts[0].id;
        vm.finish(id, Err("refused".into()));
        assert!(vm.store.lock().flows.is_empty());
        assert!(matches!(&vm.drafts[0].result, Some(Err(e)) if e == "refused"));
    }

    #[test]
    fn schemeless_url_defaults_to_https_and_root_path() {
        assert_eq!(split_url("a.com"), ("https", "a.com", "/"));
        assert_eq!(split_url(" http://a.com/p "), ("http", "a.com", "/p"));
    }

    #[test]
    fn closed_windows_are_pruned_and_stale_results_ignored() {
        let mut vm = vm();
        vm.open_compose();
        let id = vm.drafts[0].id;
        vm.drafts[0].open = false;
        vm.prune_closed();
        assert!(vm.drafts.is_empty());
        vm.finish(id, Ok(sent(200)));
        assert!(vm.store.lock().flows.is_empty());
    }
}
