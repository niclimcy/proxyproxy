use super::{ResParts, RuleAction};
use crate::domain::flow::Event;
use crate::proxy::{
    http_util::{header_map, headers},
    script,
    sink::FlowSink,
};
use bytes::Bytes;
use hudsucker::hyper::{Method, StatusCode, Uri, http::request::Parts};

pub struct Script {
    pub code: String,
}

// Scripts run on a blocking thread; logs and errors go to the console via the sink.
fn run(sink: &dyn FlowSink, code: &str, hook: &str, ctx: &mut script::Ctx) {
    let (ltx, lrx) = std::sync::mpsc::channel();
    let res = tokio::task::block_in_place(|| script::run(code, hook, ctx, &ltx));
    for l in lrx.try_iter() {
        sink.emit(Event::Log(l));
    }
    if let Err(e) = res {
        sink.emit(Event::Log(format!("[error] {hook}: {e}")));
    }
}

impl RuleAction for Script {
    fn tag(&self) -> &'static str {
        "Script"
    }

    fn runs_on_response(&self) -> bool {
        true
    }

    fn apply_request(&self, parts: &mut Parts, body: &mut Bytes, sink: &dyn FlowSink) -> bool {
        // compared against what the script saw, so a non-UTF-8 body it left alone stays byte for byte
        let sent = String::from_utf8_lossy(body).into_owned();
        let mut c = script::Ctx {
            method: parts.method.to_string(),
            url: parts.uri.to_string(),
            status: 0,
            headers: headers(&parts.headers),
            body: sent.clone(),
        };
        run(sink, &self.code, "onRequest", &mut c);
        if let Ok(m) = c.method.parse::<Method>() {
            parts.method = m;
        }
        if let Ok(u) = c.url.parse::<Uri>() {
            parts.uri = u;
        }
        parts.headers = header_map(&c.headers);
        if c.body != sent {
            *body = Bytes::from(c.body.into_bytes());
        }
        true
    }

    fn apply_response(&self, parts: &mut ResParts, body: &mut Bytes, sink: &dyn FlowSink) {
        let sent = String::from_utf8_lossy(body).into_owned();
        let mut c = script::Ctx {
            method: String::new(),
            url: String::new(),
            status: parts.status.as_u16(),
            headers: headers(&parts.headers),
            body: sent.clone(),
        };
        run(sink, &self.code, "onResponse", &mut c);
        if let Ok(s) = StatusCode::from_u16(c.status) {
            parts.status = s;
        }
        parts.headers = header_map(&c.headers);
        if c.body != sent {
            *body = Bytes::from(c.body.into_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{TestSink, req_parts, res_parts};
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn request_hook_logs_and_mutates() {
        let sink = TestSink::default();
        let mut parts = req_parts("https://a.com/");
        let code =
            "function onRequest(c){ c.headers['x-s']='1'; console.log('hi', c.method); return c; }";
        assert!(Script { code: code.into() }.apply_request(&mut parts, &mut Bytes::new(), &sink));
        assert_eq!(*sink.0.lock().unwrap(), vec!["hi GET".to_string()]);
        assert_eq!(parts.headers.get("x-s").unwrap(), "1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn response_hook_can_change_status_and_body() {
        let sink = TestSink::default();
        let mut parts = res_parts(200);
        let mut body = Bytes::from_static(b"old");
        let code = "function onResponse(c){ c.status = 418; c.body = 'new'; return c; }";
        Script { code: code.into() }.apply_response(&mut parts, &mut body, &sink);
        assert_eq!((parts.status.as_u16(), &body[..]), (418, &b"new"[..]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn untouched_binary_bodies_stay_byte_for_byte() {
        let sink = TestSink::default();
        let raw = Bytes::from_static(&[0x89, b'P', 0xff, 0xfe, 0x00]);
        let code = "function onRequest(c){ c.headers['x']='1'; return c; } function onResponse(c){ return c; }";
        let s = Script { code: code.into() };
        let mut body = raw.clone();
        s.apply_request(&mut req_parts("https://a.com/"), &mut body, &sink);
        assert_eq!(body, raw);
        s.apply_response(&mut res_parts(200), &mut body, &sink);
        assert_eq!(body, raw);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn script_errors_are_reported_not_panicked() {
        let sink = TestSink::default();
        let code = "function onRequest(){ throw new Error('boom') }";
        Script { code: code.into() }.apply_request(
            &mut req_parts("http://a/"),
            &mut Bytes::new(),
            &sink,
        );
        assert!(sink.0.lock().unwrap()[0].starts_with("[error] onRequest"));
    }
}
