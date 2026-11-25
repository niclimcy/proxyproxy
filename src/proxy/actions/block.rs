use super::RuleAction;
use crate::proxy::http_util::text_response;
use hudsucker::{Body, hyper::Response};

pub struct Block {
    pub status: u16,
}

impl RuleAction for Block {
    fn tag(&self) -> &'static str {
        "Blocked"
    }

    fn short_circuit(&self) -> Option<Response<Body>> {
        Some(text_response(self.status, "Blocked by proxyproxy".into()))
    }
}
