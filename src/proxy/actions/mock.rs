use super::RuleAction;
use crate::proxy::http_util::reply;
use hudsucker::{Body, hyper::Response};

pub struct Mock {
    pub status: u16,
    pub content_type: String,
    pub body: String,
}

impl RuleAction for Mock {
    fn tag(&self) -> &'static str {
        "Mock"
    }

    fn short_circuit(&self) -> Option<Response<Body>> {
        Some(reply(self.status, &self.content_type, self.body.clone().into_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_with_configured_status_and_type() {
        let r = Mock { status: 201, content_type: "text/plain".into(), body: "b".into() }.short_circuit().unwrap();
        assert_eq!(r.status().as_u16(), 201);
        assert_eq!(r.headers().get("content-type").unwrap(), "text/plain");
    }
}
