mod block;
mod breakpoint;
mod delay;
mod map_local;
mod map_remote;
mod mock;
mod replace_body;
mod script;
mod set_header;

use super::sink::FlowSink;
use crate::domain::rule::Action;
use bytes::Bytes;
use hudsucker::{
    Body,
    hyper::{Response, http::request::Parts},
};
use std::{sync::Arc, time::Duration};

pub type ResParts = hudsucker::hyper::http::response::Parts;

// One implementation per rule type. Every hook is pure with respect to the
// network: it mutates the message it is given and never panics.
pub trait RuleAction: Send + Sync {
    fn tag(&self) -> &'static str;

    // (pause request, pause response)
    fn breakpoint(&self) -> (bool, bool) {
        (false, false)
    }

    fn runs_on_response(&self) -> bool {
        false
    }

    fn short_circuit(&self) -> Option<Response<Body>> {
        None
    }

    fn request_delay(&self) -> Option<Duration> {
        None
    }

    // returns true when the request was modified
    fn apply_request(&self, _parts: &mut Parts, _body: &mut Bytes, _sink: &dyn FlowSink) -> bool {
        false
    }

    fn apply_response(&self, _parts: &mut ResParts, _body: &mut Bytes, _sink: &dyn FlowSink) {}

    fn response_delay(&self, _body_len: usize) -> Option<Duration> {
        None
    }
}

pub type Rule = Arc<dyn RuleAction>;

// The only place that maps persisted `Action` data onto behaviour.
pub fn resolve(a: &Action) -> Rule {
    match a.clone() {
        Action::Breakpoint { request, response } => Arc::new(breakpoint::Breakpoint { request, response }),
        Action::MapLocal { path, status } => Arc::new(map_local::MapLocal { path, status }),
        Action::MapRemote { target } => Arc::new(map_remote::MapRemote { target }),
        Action::Block { status } => Arc::new(block::Block { status }),
        Action::Delay { ms, kbps } => Arc::new(delay::Delay { ms, kbps }),
        Action::SetHeader { response, name, value, remove } => Arc::new(set_header::SetHeader { response, name, value, remove }),
        Action::ReplaceBody { response, find, replace, regex } => Arc::new(replace_body::ReplaceBody { response, find, replace, regex }),
        Action::Mock { status, content_type, body } => Arc::new(mock::Mock { status, content_type, body }),
        Action::Script { code } => Arc::new(script::Script { code }),
    }
}

pub fn resolve_all(actions: &[Action]) -> Vec<Rule> {
    actions.iter().map(resolve).collect()
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::domain::flow::Event;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct TestSink(pub Mutex<Vec<String>>);

    impl FlowSink for TestSink {
        fn emit(&self, ev: Event) {
            if let Event::Log(l) = ev {
                self.0.lock().unwrap().push(l);
            }
        }
    }

    pub fn req_parts(uri: &str) -> Parts {
        hudsucker::hyper::Request::builder().method("GET").uri(uri).body(()).unwrap().into_parts().0
    }

    pub fn res_parts(status: u16) -> ResParts {
        Response::builder().status(status).body(()).unwrap().into_parts().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_variant_resolves_with_expected_tag() {
        let cases = [
            (Action::Breakpoint { request: true, response: false }, "Breakpoint"),
            (Action::MapLocal { path: "/x".into(), status: 200 }, "Mock"),
            (Action::MapRemote { target: "http://b".into() }, "Redirect"),
            (Action::Block { status: 403 }, "Blocked"),
            (Action::Delay { ms: 1, kbps: 0 }, "Throttled"),
            (Action::SetHeader { response: false, name: "a".into(), value: "b".into(), remove: false }, "Rewritten"),
            (Action::ReplaceBody { response: false, find: "a".into(), replace: "b".into(), regex: false }, "Rewritten"),
            (Action::Mock { status: 200, content_type: "t".into(), body: "b".into() }, "Mock"),
            (Action::Script { code: String::new() }, "Script"),
        ];
        for (a, tag) in cases {
            assert_eq!(resolve(&a).tag(), tag, "{a:?}");
        }
    }

    #[test]
    fn response_phase_membership() {
        let on = |a: Action| resolve(&a).runs_on_response();
        assert!(on(Action::Delay { ms: 0, kbps: 0 }));
        assert!(on(Action::Script { code: String::new() }));
        assert!(on(Action::SetHeader { response: true, name: "a".into(), value: String::new(), remove: false }));
        assert!(!on(Action::SetHeader { response: false, name: "a".into(), value: String::new(), remove: false }));
        assert!(!on(Action::Block { status: 403 }));
    }
}
