use super::RuleAction;
use crate::domain::mime;
use crate::proxy::http_util::{reply, text_response};
use hudsucker::{Body, hyper::Response};

pub struct MapLocal {
    pub path: String,
    pub status: u16,
}

impl RuleAction for MapLocal {
    fn tag(&self) -> &'static str {
        "Mock"
    }

    fn short_circuit(&self) -> Option<Response<Body>> {
        Some(match std::fs::read(&self.path) {
            Ok(data) => reply(self.status, mime::content_type(&self.path), data),
            Err(e) => text_response(502, format!("proxyproxy Map Local: cannot read {}: {e}", self.path)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_502() {
        assert_eq!(MapLocal { path: "/nonexistent/file.json".into(), status: 200 }.short_circuit().unwrap().status().as_u16(), 502);
    }

    #[test]
    fn existing_file_is_served_with_guessed_type() {
        let p = std::env::temp_dir().join(format!("pxp-maplocal-{}.json", std::process::id()));
        std::fs::write(&p, "{}").unwrap();
        let r = MapLocal { path: p.display().to_string(), status: 200 }.short_circuit().unwrap();
        assert_eq!(r.headers().get("content-type").unwrap(), "application/json");
        std::fs::remove_file(p).ok();
    }
}
