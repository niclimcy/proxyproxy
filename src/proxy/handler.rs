use super::actions::{self, Rule};
use super::app_cache::{AppCache, Found, Join};
use super::http_util::{header_map, headers, strip_default_port, text_response};
use super::pipeline::{
    self, apply_request_actions, apply_response_actions, request_delay, short_circuit,
};
use super::sink::FlowSink;
use crate::domain::app::AppIdentity;
use crate::domain::config::Shared;
use crate::domain::flow::{
    self, BpEdit, Decision, Dir, Event, Flow, Message, Paused, Stage, WsMsg,
};
use crate::services::app_icons::resolve;
use crate::services::client_proc::lookup_app;
use bytes::Bytes;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use http_body_util::{BodyExt, Full};
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse, WebSocketContext, WebSocketHandler,
    decode_request,
    hyper::{
        HeaderMap, Method, Request, Response, StatusCode, Uri,
        header::{CONTENT_ENCODING, CONTENT_LENGTH, HeaderValue, PROXY_AUTHORIZATION},
    },
    tokio_tungstenite::tungstenite::Message as WsMessage,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

pub type WsMap = Arc<Mutex<HashMap<String, u64>>>;

fn emit_found(sink: &dyn FlowSink, id: u64, found: &Found, ua_platform: Option<&'static str>) {
    match found {
        Some((app, identity)) => sink.emit(Event::App {
            id,
            app: identity
                .as_ref()
                .map_or_else(|| app.clone(), |i| i.name.clone()),
            identity: identity.clone(),
        }),
        None => {
            if let Some(p) = ua_platform {
                let identity = AppIdentity {
                    platform: p.into(),
                    ..Default::default()
                };
                sink.emit(Event::App {
                    id,
                    app: String::new(),
                    identity: Some(identity),
                });
            }
        }
    }
}

// Decodes a copy so an unsupported or corrupt encoding forwards the original bytes and
// headers instead of an empty body (hudsucker consumes the body before it errors).
async fn decode_body(headers: &mut HeaderMap, body: Body) -> Bytes {
    let raw = body
        .collect()
        .await
        .map_or_default(http_body_util::Collected::to_bytes);
    if !headers.contains_key(CONTENT_ENCODING) {
        return raw;
    }
    let mut probe = Request::new(Body::from(Full::new(raw.clone())));
    *probe.headers_mut() = headers.clone();
    // hudsucker matches encoding names case-sensitively
    let lower: Vec<HeaderValue> = headers
        .get_all(CONTENT_ENCODING)
        .iter()
        .filter_map(|v| HeaderValue::from_bytes(&v.as_bytes().to_ascii_lowercase()).ok())
        .collect();
    probe.headers_mut().remove(CONTENT_ENCODING);
    for v in lower {
        probe.headers_mut().append(CONTENT_ENCODING, v);
    }
    let Ok(decoded) = decode_request(probe) else {
        return raw;
    };
    let (p, b) = decoded.into_parts();
    match b.collect().await {
        Ok(b) => {
            *headers = p.headers;
            b.to_bytes()
        }
        Err(_) => raw,
    }
}

// The upgrade request and the socket handler see the same URL in different forms
// (`https`/`a.com` vs `wss`/`a.com:443`), so both go through here.
fn ws_key(scheme: &str, authority: &str, path: &str) -> String {
    format!(
        "{}{path}",
        strip_default_port(scheme, authority.to_string())
    )
}

#[derive(Clone)]
pub struct Capture {
    pub sink: Arc<dyn FlowSink>,
    pub cfg: Shared,
    pub ws: WsMap,
    pub current: Option<u64>,
    pub started: Option<Instant>,
    pub res_actions: Vec<Rule>,
    pub bp_response: bool,
    pub apps: Arc<AppCache>,
}

impl Capture {
    fn add_upstream_auth(&self, parts: &mut hudsucker::hyper::http::request::Parts) {
        if parts.uri.scheme_str() != Some("http") {
            return;
        }
        let auth = super::connector::upstream::plain_http_auth(&self.cfg.lock().unwrap().upstream);
        if let Some(v) = auth.and_then(|a| HeaderValue::try_from(a).ok()) {
            parts.headers.insert(PROXY_AUTHORIZATION, v);
        }
    }

    async fn pause(&self, id: u64, stage: Stage, edit: BpEdit) -> Decision {
        let (reply, rx) = oneshot::channel();
        let orig = edit.clone();
        self.sink.emit(Event::Paused(Paused {
            id,
            stage,
            edit,
            reply,
        }));
        rx.await.unwrap_or(Decision::Resume(orig))
    }

    fn elapsed(&self) -> u128 {
        self.started.map_or(0, |s| s.elapsed().as_millis())
    }
}

impl HttpHandler for Capture {
    fn should_intercept_connect(
        &mut self,
        _: &HttpContext,
        req: &Request<Body>,
    ) -> impl Future<Output = bool> + Send {
        let host = req.uri().host().unwrap_or("");
        std::future::ready(self.cfg.lock().unwrap().should_decrypt(host))
    }

    async fn handle_request(
        &mut self,
        hctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        self.current = None;
        self.res_actions.clear();
        self.bp_response = false;

        if req.method() == Method::CONNECT {
            return req.into();
        }

        let is_ws = req
            .headers()
            .get("upgrade")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));

        let (mut parts, body) = req.into_parts();
        let mut bytes = decode_body(&mut parts.headers, body).await;

        let scheme = parts.uri.scheme_str().unwrap_or("https").to_string();
        let host = parts
            .uri
            .authority()
            .map(std::string::ToString::to_string)
            .or_else(|| {
                parts
                    .headers
                    .get("host")
                    .and_then(|h| h.to_str().ok())
                    .map(String::from)
            })
            .unwrap_or_default();
        let host = strip_default_port(&scheme, host);
        let path = parts
            .uri
            .path_and_query()
            .map_or_else(|| "/".into(), std::string::ToString::to_string);
        let url = format!("{scheme}://{host}{path}");
        let method = parts.method.to_string();

        let (captured, actions) = {
            let mut cfg = self.cfg.lock().unwrap();
            let c = cfg.captured(&url);
            (
                c,
                if c {
                    cfg.matching(&method, &url)
                } else {
                    vec![]
                },
            )
        };
        if !captured {
            self.add_upstream_auth(&mut parts);
            return Request::from_parts(parts, Body::from(Full::new(bytes))).into();
        }

        let id = flow::next_id();
        self.current = Some(id);
        self.started = Some(Instant::now());

        let header = |n: &str| {
            parts
                .headers
                .get(n)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
        };
        let ua_platform = hctx
            .client_addr
            .ip()
            .is_loopback()
            .then(|| crate::domain::app::platform_from_ua(header("user-agent")))
            .flatten();
        let addr = hctx.client_addr;
        match self.apps.join(addr, (id, ua_platform), Instant::now()) {
            Join::Known(found) => emit_found(self.sink.as_ref(), id, &found, ua_platform),
            Join::Wait => {}
            Join::Start => {
                let (sink, apps, own) = (self.sink.clone(), self.apps.clone(), std::process::id());
                let package = crate::domain::app::package_hint(
                    header("x-requested-with"),
                    header("user-agent"),
                );
                std::thread::spawn(move || {
                    let found = lookup_app(addr.port(), own).map(|(app, pid)| {
                        // bundle and icon work is slower, so the name shows first
                        for (w, _) in apps.waiting(addr) {
                            sink.emit(Event::App {
                                id: w,
                                app: app.clone(),
                                identity: None,
                            });
                        }
                        let identity = resolve(pid, &app, package);
                        (app, identity)
                    });
                    for (w, ua) in apps.finish(addr, found.clone(), Instant::now()) {
                        emit_found(sink.as_ref(), w, &found, ua);
                    }
                });
            }
        }

        let actions = actions::resolve_all(&actions);
        let plan = pipeline::plan(&actions);
        self.bp_response |= plan.break_response;
        self.res_actions = plan.response_actions;

        let mut f = Flow {
            id,
            method,
            scheme,
            host: host.clone(),
            path,
            version: format!("{:?}", parts.version),
            started: SystemTime::now(),
            started_at: Instant::now(),
            request: Message {
                headers: headers(&parts.headers),
                body: bytes.to_vec(),
            },
            status: None,
            response: None,
            duration_ms: None,
            tag: plan.tag,
            app: String::new(),
            app_id: None,
            pinned: false,
            comment: String::new(),
            color: None,
            is_ws,
            ws: vec![],
            error: None,
        };
        if is_ws {
            f.status = Some(101);
            self.ws
                .lock()
                .unwrap()
                .insert(ws_key(&f.scheme, &f.host, &f.path), id);
        }
        self.sink.emit(Event::Request(Box::new(f)));
        if is_ws {
            self.current = None;
            return Request::from_parts(parts, Body::from(Full::new(bytes))).into();
        }

        let short = short_circuit(&actions);
        request_delay(&actions).await;
        if let Some(res) = short {
            let (p, b) = res.into_parts();
            let data = b
                .collect()
                .await
                .map_or_default(http_body_util::Collected::to_bytes);
            self.sink.emit(Event::Response {
                id,
                status: p.status.as_u16(),
                response: Message {
                    headers: headers(&p.headers),
                    body: data.to_vec(),
                },
                duration_ms: self.elapsed(),
            });
            self.current = None;
            return Response::from_parts(p, Body::from(Full::new(data))).into();
        }

        let mut edited =
            apply_request_actions(&actions, &mut parts, &mut bytes, self.sink.as_ref());

        if plan.break_request {
            let edit = BpEdit {
                method: parts.method.to_string(),
                url: parts.uri.to_string(),
                status: 0,
                headers: headers(&parts.headers),
                body: bytes.to_vec(),
            };
            match self.pause(id, Stage::Request, edit).await {
                Decision::Resume(e) => {
                    if let Ok(m) = e.method.trim().parse::<Method>() {
                        parts.method = m;
                    }
                    if let Ok(u) = e.url.trim().parse::<Uri>() {
                        parts.uri = u;
                    }
                    parts.headers = header_map(&e.headers);
                    bytes = Bytes::from(e.body);
                    edited = true;
                }
                Decision::Abort => {
                    self.current = None;
                    self.sink.emit(Event::Response {
                        id,
                        status: 499,
                        response: Message {
                            headers: vec![],
                            body: b"Aborted by breakpoint".to_vec(),
                        },
                        duration_ms: self.elapsed(),
                    });
                    return text_response(499, "Aborted by proxyproxy breakpoint".into()).into();
                }
            }
        }
        parts.headers.remove(CONTENT_LENGTH);

        if edited {
            let auth = parts
                .uri
                .authority()
                .map(std::string::ToString::to_string)
                .unwrap_or(host);
            let auth = strip_default_port(parts.uri.scheme_str().unwrap_or("https"), auth);
            self.sink.emit(Event::Edited {
                id,
                method: parts.method.to_string(),
                host: auth,
                path: parts
                    .uri
                    .path_and_query()
                    .map_or_else(|| "/".into(), std::string::ToString::to_string),
                request: Message {
                    headers: headers(&parts.headers),
                    body: bytes.to_vec(),
                },
            });
        }
        // after recording, so the credentials never show up in the flow
        self.add_upstream_auth(&mut parts);
        Request::from_parts(parts, Body::from(Full::new(bytes))).into()
    }

    async fn handle_response(&mut self, _: &HttpContext, res: Response<Body>) -> Response<Body> {
        let (mut parts, body) = res.into_parts();
        let mut bytes = decode_body(&mut parts.headers, body).await;
        let Some(id) = self.current.take() else {
            return Response::from_parts(parts, Body::from(Full::new(bytes)));
        };

        apply_response_actions(
            std::mem::take(&mut self.res_actions),
            &mut parts,
            &mut bytes,
            self.sink.as_ref(),
        )
        .await;

        if std::mem::take(&mut self.bp_response) {
            let edit = BpEdit {
                method: String::new(),
                url: String::new(),
                status: parts.status.as_u16(),
                headers: headers(&parts.headers),
                body: bytes.to_vec(),
            };
            match self.pause(id, Stage::Response, edit).await {
                Decision::Resume(e) => {
                    if let Ok(s) = StatusCode::from_u16(e.status) {
                        parts.status = s;
                    }
                    parts.headers = header_map(&e.headers);
                    bytes = Bytes::from(e.body);
                }
                Decision::Abort => {
                    self.sink.emit(Event::Response {
                        id,
                        status: 499,
                        response: Message::default(),
                        duration_ms: self.elapsed(),
                    });
                    return text_response(499, "Aborted by proxyproxy breakpoint".into());
                }
            }
        }
        parts.headers.remove(CONTENT_LENGTH);
        self.sink.emit(Event::Response {
            id,
            status: parts.status.as_u16(),
            response: Message {
                headers: headers(&parts.headers),
                body: bytes.to_vec(),
            },
            duration_ms: self.elapsed(),
        });
        Response::from_parts(parts, Body::from(Full::new(bytes)))
    }

    fn handle_error(
        &mut self,
        _: &HttpContext,
        err: hudsucker::hyper_util::client::legacy::Error,
    ) -> impl Future<Output = Response<Body>> + Send {
        let msg = format!(
            "{err}: {}",
            std::error::Error::source(&err).map_or_default(std::string::ToString::to_string)
        );
        if let Some(id) = self.current.take() {
            self.sink.emit(Event::Failed {
                id,
                error: msg.clone(),
            });
        }
        std::future::ready(text_response(
            502,
            format!("proxyproxy: upstream failed: {msg}"),
        ))
    }
}

#[derive(Clone)]
pub struct WsCap {
    pub sink: Arc<dyn FlowSink>,
    pub map: WsMap,
}

impl WebSocketHandler for WsCap {
    async fn handle_websocket(
        self,
        ctx: WebSocketContext,
        stream: impl Stream<Item = Result<WsMessage, hudsucker::tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send
        + 'static,
        sink: impl Sink<WsMessage, Error = hudsucker::tokio_tungstenite::tungstenite::Error>
        + Unpin
        + Send
        + 'static,
    ) {
        let (dir, uri) = match &ctx {
            WebSocketContext::ClientToServer { dst, .. } => (Dir::Out, dst.clone()),
            WebSocketContext::ServerToClient { src, .. } => (Dir::In, src.clone()),
        };
        self.relay(dir, uri, stream, sink).await;
    }
}

impl WsCap {
    // split from the trait method because tests can't build a non-exhaustive WebSocketContext
    async fn relay(
        &self,
        dir: Dir,
        uri: Uri,
        mut stream: impl Stream<
            Item = Result<WsMessage, hudsucker::tokio_tungstenite::tungstenite::Error>,
        > + Unpin,
        mut sink: impl Sink<WsMessage, Error = hudsucker::tokio_tungstenite::tungstenite::Error> + Unpin,
    ) {
        let key = ws_key(
            uri.scheme_str().unwrap_or("ws"),
            uri.authority().map_or(
                "",
                hudsucker::tokio_tungstenite::tungstenite::http::uri::Authority::as_str,
            ),
            uri.path_and_query().map_or(
                "/",
                hudsucker::tokio_tungstenite::tungstenite::http::uri::PathAndQuery::as_str,
            ),
        );
        // read once: a later socket to the same URL takes over the key, not this socket's frames
        let id = self.map.lock().unwrap().get(&key).copied();
        while let Some(m) = stream.next().await {
            let Ok(m) = m else {
                let _ = sink.send(WsMessage::Close(None)).await;
                break;
            };
            let (binary, data) = match &m {
                WsMessage::Text(t) => (false, Some(t.as_bytes().to_vec())),
                WsMessage::Binary(b) => (true, Some(b.to_vec())),
                _ => (false, None),
            };
            if let (Some(data), Some(id)) = (data, id) {
                let ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                self.sink.emit(Event::Ws {
                    id,
                    msg: WsMsg {
                        dir,
                        binary,
                        ms,
                        data,
                    },
                });
            }
            if sink.send(m).await.is_err() {
                break;
            }
        }
        let mut map = self.map.lock().unwrap();
        if id.is_some() && map.get(&key).copied() == id {
            map.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn encoded(enc: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_ENCODING, HeaderValue::from_str(enc).unwrap());
        h.insert(CONTENT_LENGTH, HeaderValue::from_static("3"));
        h
    }

    fn body(b: Vec<u8>) -> Body {
        Body::from(Full::new(Bytes::from(b)))
    }

    #[tokio::test]
    async fn gzip_decodes_whatever_the_case() {
        for enc in ["gzip", "GZIP"] {
            let mut h = encoded(enc);
            let out = decode_body(&mut h, body(gzip(b"hello"))).await;
            assert_eq!(&out[..], b"hello");
            assert!(!h.contains_key(CONTENT_ENCODING) && !h.contains_key(CONTENT_LENGTH));
        }
    }

    #[tokio::test]
    async fn unsupported_or_corrupt_encoding_passes_original_through() {
        for (enc, data) in [
            ("compress", b"abc".to_vec()),
            ("gzip", b"not gzip".to_vec()),
        ] {
            let mut h = encoded(enc);
            let out = decode_body(&mut h, body(data.clone())).await;
            assert_eq!(out.to_vec(), data);
            assert_eq!(h.get(CONTENT_ENCODING).unwrap(), enc);
            assert!(h.contains_key(CONTENT_LENGTH));
        }
    }

    #[derive(Default)]
    struct Frames(Mutex<Vec<(u64, Vec<u8>)>>);

    impl FlowSink for Frames {
        fn emit(&self, ev: Event) {
            if let Event::Ws { id, msg } = ev {
                self.0.lock().unwrap().push((id, msg.data));
            }
        }
    }

    #[tokio::test]
    async fn wss_frames_are_recorded_and_the_key_is_released() {
        let frames = Arc::new(Frames::default());
        let map: WsMap = Arc::default();
        map.lock()
            .unwrap()
            .insert(ws_key("https", "a.com", "/s"), 7);
        let cap = WsCap {
            sink: frames.clone(),
            map: map.clone(),
        };
        let stream = futures_util::stream::iter([Ok(WsMessage::Text("hi".into()))]);
        let sink =
            futures_util::sink::drain().sink_map_err(|e: std::convert::Infallible| match e {});
        cap.relay(Dir::Out, "wss://a.com:443/s".parse().unwrap(), stream, sink)
            .await;
        assert_eq!(*frames.0.lock().unwrap(), vec![(7, b"hi".to_vec())]);
        assert!(map.lock().unwrap().is_empty());
    }

    #[test]
    fn ws_key_matches_between_upgrade_and_socket_forms() {
        // hudsucker keeps the CONNECT authority (with :443) and switches the scheme to wss
        let upgrade = ws_key("https", "a.com", "/socket?x=1");
        let uri: Uri = "wss://a.com:443/socket?x=1".parse().unwrap();
        let socket = ws_key(
            uri.scheme_str().unwrap(),
            uri.authority().unwrap().as_str(),
            uri.path_and_query().unwrap().as_str(),
        );
        assert_eq!(upgrade, socket);
        assert_eq!(
            ws_key("http", "a.com:8080", "/"),
            ws_key("ws", "a.com:8080", "/")
        );
    }
}
