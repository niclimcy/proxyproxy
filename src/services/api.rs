use crate::domain::flow::Flow;
use crate::services::flow_store::FlowStore;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;

pub fn summary(f: &Flow) -> Value {
    json!({"id": f.id, "method": f.method, "url": f.url(), "status": f.status, "ms": f.duration_ms, "size": f.res_size(), "app": f.app_label(), "tag": f.tag})
}

pub fn detail(f: &Flow) -> Value {
    let m = |m: &crate::domain::flow::Message| json!({"headers": m.headers, "body": m.text()});
    let mut v = summary(f);
    v["request"] = m(&f.request);
    v["response"] = f.response.as_ref().map(m).unwrap_or(Value::Null);
    v
}

fn handle(store: &FlowStore, target: &str) -> (u16, Value) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let st = store.lock();
    let flows = &st.flows;
    let q: Vec<(&str, &str)> = query.split('&').filter_map(|p| p.split_once('=')).collect();
    let get = |k: &str| q.iter().find(|(a, _)| *a == k).map(|(_, v)| *v);
    match path.trim_end_matches('/') {
        "/flows" => {
            let query = get("q")
                .map(|s| {
                    s.replace('+', " ")
                        .replace("%20", " ")
                        .replace("%3A", ":")
                        .replace("%3a", ":")
                })
                .unwrap_or_default();
            let query = crate::domain::filter::Query::parse(&query);
            let limit = get("limit")
                .and_then(|l| l.parse().ok())
                .unwrap_or(200usize);
            let list: Vec<Value> = flows
                .iter()
                .rev()
                .filter(|f| query.matches(f))
                .take(limit)
                .map(summary)
                .collect();
            (200, json!(list))
        }
        p if p.starts_with("/flows/") => match p[7..]
            .parse::<u64>()
            .ok()
            .and_then(|id| flows.iter().find(|f| f.id == id))
        {
            Some(f) => (200, detail(f)),
            None => (404, json!({"error": "no such flow"})),
        },
        "" | "/" => (
            200,
            json!({"name": "proxyproxy", "flows": flows.len(), "endpoints": ["/flows?q=&limit=", "/flows/{id}", "/har"]}),
        ),
        "/har" => {
            let all: Vec<&Flow> = flows.iter().collect();
            (
                200,
                serde_json::from_str(&crate::services::exporter::har(&all)).unwrap_or(Value::Null),
            )
        }
        _ => (404, json!({"error": "not found"})),
    }
}

// Read-only JSON API on loopback so scripts and agents can query captured flows.
pub fn serve(port: u16, store: FlowStore, cfg: crate::domain::config::Shared) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else {
        return;
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            let mut header_end = None;
            let mut content_length = 0usize;
            loop {
                let n = match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                request.extend_from_slice(&buf[..n]);
                if header_end.is_none()
                    && let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n")
                {
                    header_end = Some(i + 4);
                    let header = String::from_utf8_lossy(&request[..i]);
                    content_length = header
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse().ok())
                        .unwrap_or(0);
                }
                if let Some(end) = header_end {
                    if request.len() >= end + content_length {
                        break;
                    }
                } else if n < buf.len() {
                    break;
                }
            }
            let req = String::from_utf8_lossy(&request);
            let line = req.lines().next().unwrap_or("");
            let mut it = line.split(' ');
            let (method, target) = (it.next().unwrap_or(""), it.next().unwrap_or("/"));
            let (code, content_type, body) = if method == "GET" {
                let (code, body) = handle(&store, target);
                (
                    code,
                    "application/json",
                    serde_json::to_string_pretty(&body).unwrap_or_default(),
                )
            } else if method == "POST" && target.split('?').next() == Some("/mcp") {
                let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
                let body_end = body_start.saturating_add(content_length).min(request.len());

                if let Ok(request) = serde_json::from_slice::<Value>(&request[body_start..body_end])
                {
                    if matches!(
                        request.get("method").and_then(Value::as_str),
                        Some("notifications/initialized")
                    ) {
                        (202, "application/json", String::new())
                    } else {
                        let result = crate::services::mcp::handle(&store, &cfg, request);
                        match result {
                            Some(result) => (
                                200,
                                "application/json",
                                serde_json::to_string(&result).unwrap_or_default(),
                            ),
                            None => (202, "application/json", String::new()),
                        }
                    }
                } else {
                    (
                        400,
                        "application/json",
                        serde_json::to_string(&json!({
                            "jsonrpc": "2.0",
                            "id": Value::Null,
                            "error": {"code": -32700, "message": "Parse error"}
                        }))
                        .unwrap_or_default(),
                    )
                }
            } else {
                (
                    405,
                    "application/json",
                    serde_json::to_string(&json!({"error": "method not allowed"}))
                        .unwrap_or_default(),
                )
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {code} {}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                if code == 200 {
                    "OK"
                } else if code == 202 {
                    "Accepted"
                } else if code == 400 {
                    "Bad Request"
                } else if code == 404 {
                    "Not Found"
                } else if code == 405 {
                    "Method Not Allowed"
                } else {
                    "OK"
                },
                body.len()
            );
        }
    });
}
