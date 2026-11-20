use super::{ResParts, RuleAction};
use crate::proxy::{http_util::set_header, sink::FlowSink};
use bytes::Bytes;
use hudsucker::hyper::http::request::Parts;

pub struct SetHeader {
    pub response: bool,
    pub name: String,
    pub value: String,
    pub remove: bool,
}

impl RuleAction for SetHeader {
    fn tag(&self) -> &'static str {
        "Rewritten"
    }

    fn runs_on_response(&self) -> bool {
        self.response
    }

    fn apply_request(&self, parts: &mut Parts, _: &mut Bytes, _: &dyn FlowSink) -> bool {
        if self.response {
            return false;
        }
        set_header(&mut parts.headers, &self.name, &self.value, self.remove);
        true
    }

    fn apply_response(&self, parts: &mut ResParts, _: &mut Bytes, _: &dyn FlowSink) {
        set_header(&mut parts.headers, &self.name, &self.value, self.remove);
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{TestSink, req_parts, res_parts};
    use super::*;

    fn rule(response: bool) -> SetHeader {
        SetHeader { response, name: "x-a".into(), value: "1".into(), remove: false }
    }

    #[test]
    fn request_variant_only_touches_requests() {
        let mut p = req_parts("https://a.com/");
        assert!(rule(false).apply_request(&mut p, &mut Bytes::new(), &TestSink::default()));
        assert_eq!(p.headers.get("x-a").unwrap(), "1");
        let mut p = req_parts("https://a.com/");
        assert!(!rule(true).apply_request(&mut p, &mut Bytes::new(), &TestSink::default()));
        assert!(p.headers.is_empty());
    }

    #[test]
    fn response_variant_sets_and_removes() {
        let mut p = res_parts(200);
        rule(true).apply_response(&mut p, &mut Bytes::new(), &TestSink::default());
        assert_eq!(p.headers.get("x-a").unwrap(), "1");
        SetHeader { remove: true, ..rule(true) }.apply_response(&mut p, &mut Bytes::new(), &TestSink::default());
        assert!(p.headers.is_empty());
    }
}
