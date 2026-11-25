use crate::domain::flow::{Event, Flow};
use std::sync::{Arc, Mutex, MutexGuard, mpsc::Receiver};

pub const MAX_FLOWS: usize = 20_000;
const DROP_BATCH: usize = 1000;

fn find(flows: &mut [Flow], id: u64) -> Option<(usize, &mut Flow)> {
    flows.iter_mut().enumerate().rev().find(|(_, f)| f.id == id)
}

#[derive(Debug, PartialEq)]
pub enum Applied {
    /// Not a flow update; the caller routes it elsewhere.
    Ignored,
    /// A flow update that matched no flow, or was dropped by a cap.
    Unchanged,
    Changed(usize),
}

impl Applied {
    pub fn handled(&self) -> bool {
        *self != Self::Ignored
    }
}

pub fn apply(flows: &mut [Flow], ev: Event) -> Applied {
    let changed = match ev {
        Event::Response {
            id,
            status,
            response,
            duration_ms,
        } => find(flows, id).map(|(i, f)| {
            f.status = Some(status);
            f.duration_ms = Some(duration_ms);
            f.response = Some(response);
            i
        }),
        Event::Edited {
            id,
            method,
            host,
            path,
            request,
        } => find(flows, id).map(|(i, f)| {
            f.method = method;
            f.host = host;
            f.path = path;
            f.request = request;
            if f.tag.is_empty() {
                f.tag = "Edited".into();
            }
            i
        }),
        Event::Failed { id, error } => find(flows, id).map(|(i, f)| {
            f.error = Some(error);
            f.status = Some(502);
            i
        }),
        Event::Ws { id, msg } => {
            find(flows, id)
                .filter(|(_, f)| f.ws.len() < 5000)
                .map(|(i, f)| {
                    f.ws.push(msg);
                    i
                })
        }
        Event::App { id, app, identity } => find(flows, id).map(|(i, f)| {
            f.app = app;
            if identity.is_some() {
                f.app_id = identity;
            }
            i
        }),
        _ => return Applied::Ignored,
    };
    changed.map_or(Applied::Unchanged, Applied::Changed)
}

// More logged edits than this and a rebuild is cheaper than replaying them.
const LOG_CAP: usize = 4096;

#[derive(Default)]
pub struct Inner {
    pub flows: Vec<Flow>,
    rev: u64,
    // Bumps when indices shift or an edit is untracked; `log` lists the flows `ingest` changed
    // since, so a reader can patch its derived state instead of rescanning every flow.
    epoch: u64,
    log: Vec<usize>,
}

impl Inner {
    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn log(&self) -> &[usize] {
        &self.log
    }

    fn invalidate(&mut self) {
        self.epoch += 1;
        self.log.clear();
        self.rev += 1;
    }

    pub fn touch(&mut self) {
        self.invalidate();
    }

    // appends keep every index valid, so readers see them as growth past their last length
    pub fn push(&mut self, f: Flow) {
        self.flows.push(f);
        self.rev += 1;
    }

    pub fn extend(&mut self, fl: Vec<Flow>) {
        self.flows.extend(fl);
        self.rev += 1;
    }

    pub fn replace(&mut self, fl: Vec<Flow>) {
        self.flows = fl;
        self.invalidate();
    }

    pub fn ingest(&mut self, ev: Event) -> bool {
        let applied = apply(&mut self.flows, ev);
        if !applied.handled() {
            return false;
        }
        self.rev += 1;
        if let Applied::Changed(i) = applied {
            self.log.push(i);
            if self.log.len() > LOG_CAP {
                self.invalidate();
            }
        }
        true
    }

    pub fn find(&self, id: u64) -> Option<&Flow> {
        self.flows.iter().find(|f| f.id == id)
    }

    // the caller's edit can't be tracked, so readers rebuild
    pub fn find_mut(&mut self, id: u64) -> Option<&mut Flow> {
        self.invalidate();
        self.flows.iter_mut().find(|f| f.id == id)
    }

    // pinned flows always survive; drops the oldest unpinned batch
    pub fn enforce_cap(&mut self, max: usize) {
        if self.flows.len() <= max {
            return;
        }
        let mut dropped = 0;
        self.flows.retain(|f| {
            let keep = f.pinned || dropped >= DROP_BATCH;
            if !keep {
                dropped += 1;
            }
            keep
        });
        self.invalidate();
    }

    pub fn retain(&mut self, keep: impl Fn(&Flow) -> bool) {
        self.flows.retain(|f| keep(f));
        self.invalidate();
    }
}

// Single source of truth for captured flows; shared with the REST API.
#[derive(Clone, Default)]
pub struct FlowStore(Arc<Mutex<Inner>>);

impl FlowStore {
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap()
    }

    pub fn push(&self, flow: crate::domain::flow::Flow) {
        self.0.lock().unwrap().push(flow);
    }

    // `other` runs under the store lock: it must not call back into the store
    pub fn pump(&self, rx: &Receiver<Event>, mut other: impl FnMut(Event)) {
        let mut st = self.lock();
        while let Ok(ev) = rx.try_recv() {
            match ev {
                Event::Request(f) => st.push(*f),
                ev @ (Event::Paused(_) | Event::Log(_)) => other(ev),
                ev => {
                    st.ingest(ev);
                }
            }
        }
        st.enforce_cap(MAX_FLOWS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::app::AppIdentity;
    use crate::domain::flow::{Dir, Message, WsMsg};
    use bytes::Bytes;
    use std::time::Instant;

    fn flow(id: u64) -> Flow {
        Flow {
            id,
            method: "GET".into(),
            scheme: "https".into(),
            host: "a.com".into(),
            path: "/".into(),
            version: "HTTP/1.1".into(),
            started: std::time::SystemTime::now(),
            started_at: Instant::now(),
            request: Message::default(),
            status: None,
            response: None,
            duration_ms: None,
            tag: String::new(),
            app: String::new(),
            app_id: None,
            pinned: false,
            comment: String::new(),
            color: None,
            is_ws: false,
            ws: vec![],
            error: None,
        }
    }

    #[test]
    fn apply_response_sets_status_body_duration() {
        let mut fs = vec![flow(1), flow(2)];
        let ev = Event::Response {
            id: 2,
            status: 201,
            response: Message {
                headers: vec![],
                body: Bytes::from_static(b"ok"),
            },
            duration_ms: 9,
        };
        assert!(apply(&mut fs, ev).handled());
        assert_eq!((fs[1].status, fs[1].duration_ms), (Some(201), Some(9)));
        assert_eq!(fs[1].response.as_ref().unwrap().body, &b"ok"[..]);
        assert_eq!(fs[0].status, None);
    }

    #[test]
    fn apply_edited_tags_only_when_untagged() {
        let mut fs = vec![flow(1), flow(2)];
        fs[1].tag = "Mocked".into();
        for id in [1, 2] {
            let ev = Event::Edited {
                id,
                method: "POST".into(),
                host: "b.com".into(),
                path: "/x".into(),
                request: Message::default(),
            };
            assert!(apply(&mut fs, ev).handled());
        }
        assert_eq!(
            (
                fs[0].method.as_str(),
                fs[0].host.as_str(),
                fs[0].path.as_str(),
                fs[0].tag.as_str()
            ),
            ("POST", "b.com", "/x", "Edited")
        );
        assert_eq!(fs[1].tag, "Mocked");
    }

    #[test]
    fn apply_failed_sets_502_and_error() {
        let mut fs = vec![flow(1)];
        assert!(
            apply(
                &mut fs,
                Event::Failed {
                    id: 1,
                    error: "boom".into()
                }
            )
            .handled()
        );
        assert_eq!(
            (fs[0].status, fs[0].error.as_deref()),
            (Some(502), Some("boom"))
        );
    }

    #[test]
    fn apply_ws_caps_at_5000() {
        let mut fs = vec![flow(1)];
        for _ in 0..5010 {
            apply(
                &mut fs,
                Event::Ws {
                    id: 1,
                    msg: WsMsg {
                        dir: Dir::In,
                        binary: false,
                        ms: 0,
                        data: Bytes::new(),
                    },
                },
            );
        }
        assert_eq!(fs[0].ws.len(), 5000);
    }

    #[test]
    fn apply_app_sets_label() {
        let mut fs = vec![flow(1)];
        assert!(
            apply(
                &mut fs,
                Event::App {
                    id: 1,
                    app: "curl".into(),
                    identity: None
                }
            )
            .handled()
        );
        assert_eq!(fs[0].app, "curl");
        let identity = AppIdentity {
            name: "Maps".into(),
            ..Default::default()
        };
        apply(
            &mut fs,
            Event::App {
                id: 1,
                app: "Maps".into(),
                identity: Some(identity.clone()),
            },
        );
        assert_eq!(fs[0].app_id, Some(identity));
        apply(
            &mut fs,
            Event::App {
                id: 1,
                app: "Maps".into(),
                identity: None,
            },
        );
        assert!(fs[0].app_id.is_some());
    }

    #[test]
    fn apply_unknown_id_is_noop_and_other_events_return_false() {
        let mut fs = vec![flow(1)];
        assert!(
            apply(
                &mut fs,
                Event::Failed {
                    id: 99,
                    error: "x".into()
                }
            )
            .handled()
        );
        assert_eq!(fs[0].status, None);
        assert!(!apply(&mut fs, Event::Log("hi".into())).handled());
        assert!(!apply(&mut fs, Event::Request(Box::new(flow(3)))).handled());
    }

    #[test]
    fn ingest_bumps_rev_only_for_handled_events() {
        let mut s = Inner::default();
        s.push(flow(1));
        let r = s.rev();
        assert!(!s.ingest(Event::Log("x".into())));
        assert_eq!(s.rev(), r);
        assert!(s.ingest(Event::App {
            id: 1,
            app: "a".into(),
            identity: None
        }));
        assert!(s.rev() > r);
    }

    #[test]
    fn cap_drops_oldest_unpinned_batch_and_keeps_pinned() {
        let mut s = Inner::default();
        for id in 1..=25 {
            let mut f = flow(id);
            f.pinned = id == 1;
            s.push(f);
        }
        s.enforce_cap(25);
        assert_eq!(s.flows.len(), 25);
        s.enforce_cap(10);
        assert_eq!(s.flows.iter().map(|f| f.id).collect::<Vec<_>>(), vec![1]);

        let mut s = Inner::default();
        for id in 1..=(DROP_BATCH as u64 + 5) {
            s.push(flow(id));
        }
        s.enforce_cap(DROP_BATCH);
        assert_eq!(s.flows.len(), 5);
        assert_eq!(s.flows[0].id, DROP_BATCH as u64 + 1);
    }

    #[test]
    fn retain_removes_and_bumps_rev() {
        let mut s = Inner::default();
        s.push(flow(1));
        s.push(flow(2));
        let r = s.rev();
        s.retain(|f| f.id != 1);
        assert_eq!(s.flows.len(), 1);
        assert!(s.rev() > r);
        assert!(s.find(1).is_none() && s.find(2).is_some());
    }

    #[test]
    fn pump_routes_events_and_enforces_cap() {
        let (tx, rx) = std::sync::mpsc::channel();
        let store = FlowStore::default();
        tx.send(Event::Request(Box::new(flow(1)))).unwrap();
        tx.send(Event::Log("hi".into())).unwrap();
        tx.send(Event::Failed {
            id: 1,
            error: "boom".into(),
        })
        .unwrap();
        let mut other = vec![];
        store.pump(&rx, |e| other.push(e));
        assert_eq!(other.len(), 1);
        assert_eq!(store.lock().find(1).unwrap().status, Some(502));

        for id in 2..=(MAX_FLOWS as u64 + 5) {
            tx.send(Event::Request(Box::new(flow(id)))).unwrap();
        }
        store.pump(&rx, |_| {});
        assert!(store.lock().flows.len() <= MAX_FLOWS);
    }
}
