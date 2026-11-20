use super::{ResParts, RuleAction};
use crate::proxy::{http_util::replace_body, sink::FlowSink};
use bytes::Bytes;
use hudsucker::hyper::http::request::Parts;

pub struct ReplaceBody {
    pub response: bool,
    pub find: String,
    pub replace: String,
    pub regex: bool,
}

impl ReplaceBody {
    fn rewrite(&self, body: &mut Bytes) -> bool {
        match replace_body(body, &self.find, &self.replace, self.regex) {
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
        ReplaceBody { response, find: "world".into(), replace: "there".into(), regex: false }
    }

    #[test]
    fn rewrites_request_body_only_when_configured_for_requests() {
        let mut b = Bytes::from_static(b"hello world");
        assert!(rule(false).apply_request(&mut req_parts("http://a/"), &mut b, &TestSink::default()));
        assert_eq!(&b[..], b"hello there");
        let mut b = Bytes::from_static(b"hello world");
        assert!(!rule(true).apply_request(&mut req_parts("http://a/"), &mut b, &TestSink::default()));
        assert_eq!(&b[..], b"hello world");
    }

    #[test]
    fn binary_responses_are_left_alone() {
        let mut b = Bytes::from_static(&[0xff, 0xfe]);
        rule(true).apply_response(&mut res_parts(200), &mut b, &TestSink::default());
        assert_eq!(&b[..], &[0xff, 0xfe]);
    }
}
