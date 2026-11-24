use super::actions::{self, Rule};
use crate::services::client_proc::lookup_app;
use super::http_util::{header_map, headers, strip_default_port, text_response};
use super::pipeline::{
    self, apply_request_actions, apply_response_actions, request_delay, short_circuit,
};
use super::sink::FlowSink;
use crate::services::app_icons::resolve;
use crate::domain::app::AppIdentity;
use crate::domain::config::Shared;
use crate::domain::flow::{
    self, BpEdit, Decision, Dir, Event, Flow, Message, Paused, Stage, WsMsg,
};
use bytes::Bytes;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use http_body_util::{BodyExt, Full};
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse, WebSocketContext, WebSocketHandler,
    decode_request, decode_response,
    hyper::{Method, Request, Response, StatusCode, Uri, header::CONTENT_LENGTH},
    tokio_tungstenite::tungstenite::Message as WsMessage,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

pub type WsMap = Arc<Mutex<HashMap<String, u64>>>;

#[derive(Clone)]
pub struct Capture {
    pub sink: Arc<dyn FlowSink>,
    pub cfg: Shared,
    pub ws: WsMap,
    pub current: Option<u64>,
    pub started: Option<Instant>,
    pub res_actions: Vec<Rule>,
    pub bp_response: bool,
    pub app_checked: bool,
}

impl Capture {
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
        self.started.map(|s| s.elapsed().as_millis()).unwrap_or(0)
    }
}

impl HttpHandler for Capture {
    async fn should_intercept_connect(&mut self, _: &HttpContext, req: &Request<Body>) -> bool {
        let host = req.uri().host().unwrap_or("");
        self.cfg.lock().unwrap().should_decrypt(host)
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

        let req = decode_request(req).unwrap_or_else(|_| Request::new(Body::empty()));
        let (mut parts, body) = req.into_parts();
        let mut bytes = body
            .collect()
            .await
            .map(|b| b.to_bytes())
            .unwrap_or_default();

        let scheme = parts.uri.scheme_str().unwrap_or("https").to_string();
        let host = parts
            .uri
            .authority()
            .map(|a| a.to_string())
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
            .map(|p| p.to_string())
            .unwrap_or_else(|| "/".into());
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
            return Request::from_parts(parts, Body::from(Full::new(bytes))).into();
        }

        let id = flow::next_id();
        self.current = Some(id);
        self.started = Some(Instant::now());

        if !self.app_checked {
            self.app_checked = true;
            let (sink, port, own) = (
                self.sink.clone(),
                hctx.client_addr.port(),
                std::process::id(),
            );
            let header = |n: &str| parts.headers.get(n).and_then(|v| v.to_str().ok()).unwrap_or_default();
            let package = crate::domain::app::package_hint(header("x-requested-with"), header("user-agent"));
            let ua_platform = hctx
                .client_addr
                .ip()
                .is_loopback()
                .then(|| crate::domain::app::platform_from_ua(header("user-agent")))
                .flatten();
            std::thread::spawn(move || {
                let Some((app, pid)) = lookup_app(port, own) else {
                    if let Some(p) = ua_platform {
                        let identity = AppIdentity { platform: p.into(), ..Default::default() };
                        sink.emit(Event::App { id, app: String::new(), identity: Some(identity) });
                    }
                    return;
                };
                sink.emit(Event::App { id, app: app.clone(), identity: None });
                // bundle and icon work is slower, so the name shows first
                if let Some(identity) = resolve(pid, &app, package) {
                    sink.emit(Event::App { id, app: identity.name.clone(), identity: Some(identity) });
                }
            });
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
                .insert(format!("{}{}", f.host, f.path), id);
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
            let data = b.collect().await.map(|b| b.to_bytes()).unwrap_or_default();
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
            let auth = parts.uri.authority().map(|a| a.to_string()).unwrap_or(host);
            let auth = strip_default_port(parts.uri.scheme_str().unwrap_or("https"), auth);
            self.sink.emit(Event::Edited {
                id,
                method: parts.method.to_string(),
                host: auth,
                path: parts
                    .uri
                    .path_and_query()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "/".into()),
                request: Message {
                    headers: headers(&parts.headers),
                    body: bytes.to_vec(),
                },
            });
        }
        Request::from_parts(parts, Body::from(Full::new(bytes))).into()
    }

    async fn handle_response(&mut self, _: &HttpContext, res: Response<Body>) -> Response<Body> {
        let Ok(res) = decode_response(res) else {
            return Response::new(Body::empty());
        };
        let (mut parts, body) = res.into_parts();
        let mut bytes = body
            .collect()
            .await
            .map(|b| b.to_bytes())
            .unwrap_or_default();
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

    async fn handle_error(
        &mut self,
        _: &HttpContext,
        err: hudsucker::hyper_util::client::legacy::Error,
    ) -> Response<Body> {
        let msg = format!(
            "{err}: {}",
            std::error::Error::source(&err)
                .map(|s| s.to_string())
                .unwrap_or_default()
        );
        if let Some(id) = self.current.take() {
            self.sink.emit(Event::Failed {
                id,
                error: msg.clone(),
            });
        }
        text_response(502, format!("proxyproxy: upstream failed: {msg}"))
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
        mut stream: impl Stream<
            Item = Result<WsMessage, hudsucker::tokio_tungstenite::tungstenite::Error>,
        > + Unpin
        + Send
        + 'static,
        mut sink: impl Sink<WsMessage, Error = hudsucker::tokio_tungstenite::tungstenite::Error>
        + Unpin
        + Send
        + 'static,
    ) {
        let (dir, uri) = match &ctx {
            WebSocketContext::ClientToServer { dst, .. } => (Dir::Out, dst.clone()),
            WebSocketContext::ServerToClient { src, .. } => (Dir::In, src.clone()),
        };
        let key = format!(
            "{}{}",
            uri.authority().map(|a| a.as_str()).unwrap_or(""),
            uri.path_and_query().map(|p| p.as_str()).unwrap_or("/")
        );
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
            if let (Some(data), Some(id)) = (data, self.map.lock().unwrap().get(&key).copied()) {
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
    }
}
