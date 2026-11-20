use super::RuleAction;
use crate::proxy::{http_util::remap, sink::FlowSink};
use bytes::Bytes;
use hudsucker::hyper::{header::{HOST, HeaderValue}, http::request::Parts};

pub struct MapRemote {
    pub target: String,
}

impl RuleAction for MapRemote {
    fn tag(&self) -> &'static str {
        "Redirect"
    }

    fn apply_request(&self, parts: &mut Parts, _: &mut Bytes, _: &dyn FlowSink) -> bool {
        let Some(uri) = remap(&parts.uri, &self.target) else { return false };
        if let Some(a) = uri.authority().and_then(|a| HeaderValue::try_from(a.as_str()).ok()) {
            parts.headers.insert(HOST, a);
        }
        parts.uri = uri;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{TestSink, req_parts};
    use super::*;

    #[test]
    fn rewrites_host_and_keeps_path() {
        let mut parts = req_parts("https://a.com/x?y=1");
        assert!(MapRemote { target: "http://b.com:8080".into() }.apply_request(&mut parts, &mut Bytes::new(), &TestSink::default()));
        assert_eq!(parts.uri.to_string(), "http://b.com:8080/x?y=1");
        assert_eq!(parts.headers.get("host").unwrap(), "b.com:8080");
    }

    #[test]
    fn unparsable_target_changes_nothing() {
        let mut parts = req_parts("https://a.com/");
        assert!(!MapRemote { target: "/just-a-path".into() }.apply_request(&mut parts, &mut Bytes::new(), &TestSink::default()));
        assert_eq!(parts.uri.to_string(), "https://a.com/");
    }
}
