use super::actions::{ResParts, Rule};
use super::sink::FlowSink;
use bytes::Bytes;
use hudsucker::{Body, hyper::{Response, http::request::Parts}};

#[derive(Default)]
pub struct Plan {
    pub tag: String,
    pub break_request: bool,
    pub break_response: bool,
    pub response_actions: Vec<Rule>,
}

pub fn plan(actions: &[Rule]) -> Plan {
    let mut p = Plan::default();
    let mut tags: Vec<&str> = vec![];
    for a in actions {
        let (rq, rs) = a.breakpoint();
        p.break_request |= rq;
        p.break_response |= rs;
        tags.push(a.tag());
        if a.runs_on_response() {
            p.response_actions.push(a.clone());
        }
    }
    tags.dedup();
    p.tag = tags.join(" + ");
    p
}

// first action that answers on its own wins
pub fn short_circuit(actions: &[Rule]) -> Option<Response<Body>> {
    actions.iter().find_map(|a| a.short_circuit())
}

pub async fn request_delay(actions: &[Rule]) {
    for a in actions {
        if let Some(d) = a.request_delay() {
            tokio::time::sleep(d).await;
        }
    }
}

// returns true when the request was modified
pub fn apply_request_actions(actions: &[Rule], parts: &mut Parts, body: &mut Bytes, sink: &dyn FlowSink) -> bool {
    let mut edited = false;
    for a in actions {
        edited |= a.apply_request(parts, body, sink);
    }
    edited
}

pub async fn apply_response_actions(actions: Vec<Rule>, parts: &mut ResParts, body: &mut Bytes, sink: &dyn FlowSink) {
    for a in actions {
        a.apply_response(parts, body, sink);
        if let Some(d) = a.response_delay(body.len()) {
            tokio::time::sleep(d).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::actions::{RuleAction, resolve_all, testing::{TestSink, req_parts, res_parts}};
    use super::*;
    use crate::domain::rule::Action;
    use std::sync::Arc;
    use std::time::Duration;

    fn rules(a: Vec<Action>) -> Vec<Rule> {
        resolve_all(&a)
    }

    #[test]
    fn plan_tags_and_collects_response_actions() {
        let p = plan(&rules(vec![
            Action::Breakpoint { request: true, response: false },
            Action::SetHeader { response: true, name: "x".into(), value: "y".into(), remove: false },
            Action::SetHeader { response: false, name: "x".into(), value: "y".into(), remove: false },
        ]));
        assert_eq!(p.tag, "Breakpoint + Rewritten");
        assert!(p.break_request && !p.break_response);
        assert_eq!(p.response_actions.len(), 1);
    }

    #[test]
    fn breakpoint_flags_accumulate_across_actions() {
        let p = plan(&rules(vec![Action::Breakpoint { request: true, response: false }, Action::Breakpoint { request: false, response: true }]));
        assert!(p.break_request && p.break_response);
        assert_eq!(p.tag, "Breakpoint");
    }

    #[test]
    fn block_and_mock_short_circuit_first_wins() {
        let r = short_circuit(&rules(vec![Action::Block { status: 403 }, Action::Mock { status: 200, content_type: "x".into(), body: "b".into() }])).unwrap();
        assert_eq!(r.status().as_u16(), 403);
        let r = short_circuit(&rules(vec![Action::Mock { status: 201, content_type: "text/plain".into(), body: "b".into() }])).unwrap();
        assert_eq!(r.status().as_u16(), 201);
        assert!(short_circuit(&rules(vec![Action::Delay { ms: 1, kbps: 0 }])).is_none());
    }

    #[test]
    fn redirect_then_header_and_body_rewrites_apply_in_order() {
        let mut parts = req_parts("https://a.com/x?y=1");
        let mut bytes = Bytes::from_static(b"hello world");
        let actions = rules(vec![
            Action::MapRemote { target: "http://b.com:8080".into() },
            Action::SetHeader { response: false, name: "X-A".into(), value: "1".into(), remove: false },
            Action::SetHeader { response: true, name: "X-R".into(), value: "1".into(), remove: false },
            Action::ReplaceBody { response: false, find: "world".into(), replace: "there".into(), regex: false },
        ]);
        assert!(apply_request_actions(&actions, &mut parts, &mut bytes, &TestSink::default()));
        assert_eq!(parts.uri.to_string(), "http://b.com:8080/x?y=1");
        assert_eq!(parts.headers.get("x-a").unwrap(), "1");
        assert!(parts.headers.get("x-r").is_none());
        assert_eq!(&bytes[..], b"hello there");
    }

    #[test]
    fn no_actions_means_not_edited() {
        assert!(!apply_request_actions(&[], &mut req_parts("https://a.com/"), &mut Bytes::new(), &TestSink::default()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn response_actions_run_in_order_then_throttle() {
        let actions = plan(&rules(vec![
            Action::SetHeader { response: true, name: "x-r".into(), value: "1".into(), remove: false },
            Action::ReplaceBody { response: true, find: "a".into(), replace: "bb".into(), regex: false },
            Action::Delay { ms: 0, kbps: 1024 },
        ]))
        .response_actions;
        let (mut parts, mut body) = (res_parts(200), Bytes::from_static(b"aaa"));
        apply_response_actions(actions, &mut parts, &mut body, &TestSink::default()).await;
        assert_eq!(parts.headers.get("x-r").unwrap(), "1");
        assert_eq!(&body[..], b"bbbbbb");
    }

    // A new rule type needs only a RuleAction impl; the pipeline is untouched.
    struct Stamp;
    impl RuleAction for Stamp {
        fn tag(&self) -> &'static str {
            "Stamped"
        }
        fn request_delay(&self) -> Option<Duration> {
            Some(Duration::ZERO)
        }
        fn apply_request(&self, parts: &mut Parts, _: &mut Bytes, _: &dyn FlowSink) -> bool {
            parts.headers.insert("x-stamp", "1".parse().unwrap());
            true
        }
    }

    #[test]
    fn custom_action_plugs_into_the_pipeline() {
        let all: Vec<Rule> = vec![Arc::new(Stamp)];
        assert_eq!(plan(&all).tag, "Stamped");
        let mut parts = req_parts("http://a/");
        assert!(apply_request_actions(&all, &mut parts, &mut Bytes::new(), &TestSink::default()));
        assert_eq!(parts.headers.get("x-stamp").unwrap(), "1");
    }
}
