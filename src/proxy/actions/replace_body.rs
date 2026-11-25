use super::{ResParts, RuleAction};
use crate::proxy::{http_util::replace_body, sink::FlowSink};
use bytes::Bytes;
use hudsucker::hyper::http::request::Parts;
use regex::Regex;
use std::sync::OnceLock;

pub struct ReplaceBody {
    pub response: bool,
    pub find: String,
    pub replace: String,
    pub regex: bool,
    // compiled on first use; an invalid pattern stays None and rewrites nothing
    compiled: OnceLock<Option<Regex>>,
}

impl ReplaceBody {
    pub fn new(response: bool, find: String, replace: String, regex: bool) -> Self {
        Self {
            response,
            find,
            replace,
            regex,
            compiled: OnceLock::new(),
        }
    }

    fn rewrite(&self, body: &mut Bytes) -> bool {
        let re = if self.regex {
            match self.compiled.get_or_init(|| Regex::new(&self.find).ok()) {
                Some(re) => Some(re),
                None => return false,
            }
        } else {
            None
        };
        match replace_body(body, &self.find, &self.replace, re) {
            Some(b) => {
                *body = Bytes::from(b);
                true
            }
            None => false,
        }
    }
}

impl RuleAction for ReplaceBody {
    fn tag(&self) -> &'static str {
        "Rewritten"
    }

    fn runs_on_response(&self) -> bool {
        self.response
    }

    fn apply_request(&self, _: &mut Parts, body: &mut Bytes, _: &dyn FlowSink) -> bool {
        !self.response && self.rewrite(body)
    }

    fn apply_response(&self, _: &mut ResParts, body: &mut Bytes, _: &dyn FlowSink) {
        self.rewrite(body);
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{TestSink, req_parts, res_parts};
    use super::*;

    fn rule(response: bool) -> ReplaceBody {
        ReplaceBody::new(response, "world".into(), "there".into(), false)
    }

    #[test]
    fn rewrites_request_body_only_when_configured_for_requests() {
        let mut b = Bytes::from_static(b"hello world");
        assert!(rule(false).apply_request(
            &mut req_parts("http://a/"),
            &mut b,
            &TestSink::default()
        ));
        assert_eq!(&b[..], b"hello there");
        let mut b = Bytes::from_static(b"hello world");
        assert!(!rule(true).apply_request(
            &mut req_parts("http://a/"),
            &mut b,
            &TestSink::default()
        ));
        assert_eq!(&b[..], b"hello world");
    }

    #[test]
    fn binary_responses_are_left_alone() {
        let mut b = Bytes::from_static(&[0xff, 0xfe]);
        rule(true).apply_response(&mut res_parts(200), &mut b, &TestSink::default());
        assert_eq!(&b[..], &[0xff, 0xfe]);
    }

    #[test]
    fn regex_mode_rewrites_and_an_invalid_pattern_changes_nothing() {
        let sink = TestSink::default();
        let run = |find: &str| {
            let mut b = Bytes::from_static(b"a1b22");
            let edited = ReplaceBody::new(false, find.into(), "#".into(), true).apply_request(
                &mut req_parts("http://a/"),
                &mut b,
                &sink,
            );
            (edited, b)
        };
        assert_eq!(run(r"\d+"), (true, Bytes::from_static(b"a#b#")));
        assert_eq!(run("("), (false, Bytes::from_static(b"a1b22")));
    }

    #[test]
    fn a_rule_that_matches_nothing_does_not_mark_the_request_edited() {
        let sink = TestSink::default();
        for (find, regex) in [("absent", false), ("ab+sent", true), ("hello", false)] {
            // "hello" replaced by itself is also no change
            let replace = if find == "hello" { "hello" } else { "x" };
            let mut b = Bytes::from_static(b"hello world");
            let edited = ReplaceBody::new(false, find.into(), replace.into(), regex).apply_request(
                &mut req_parts("http://a/"),
                &mut b,
                &sink,
            );
            assert!(!edited, "{find}");
            assert_eq!(&b[..], b"hello world");
        }
    }
}
