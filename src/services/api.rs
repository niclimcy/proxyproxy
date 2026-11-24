use crate::domain::config::Shared;
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

// Serializes a copy so the UI and the proxy's store updates don't wait on a large export.
pub fn har_json(store: &FlowStore) -> Value {
    let flows = store.lock().flows.clone();
    let all: Vec<&Flow> = flows.iter().collect();
    serde_json::from_str(&crate::services::exporter::har(&all)).unwrap_or(Value::Null)
}

fn handle(store: &FlowStore, target: &str) -> (u16, Value) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path.trim_end_matches('/') == "/har" {
        return (200, har_json(store));
    }
    let st = store.lock();
    let flows = &st.flows;
    let q: Vec<(&str, &str)> = query.split('&').filter_map(|p| p.split_once('=')).collect();
    let get = |k: &str| q.iter().find(|(a, _)| *a == k).map(|(_, v)| *v);
    match path.trim_end_matches('/') {
        "/flows" => {
            // `+` before decoding, so an encoded `%2B` stays a plus
            let query = get("q")
                .map(|s| crate::proxy::http_util::percent_decode(&s.replace('+', " ")))
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
        _ => (404, json!({"error": "not found"})),
    }
}

const MAX_HEADER: usize = 64 << 10;
const MAX_BODY: usize = 4 << 20;

struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_request(stream: &mut impl Read) -> Result<Request, (u16, &'static str)> {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if data.len() > MAX_HEADER {
            return Err((431, "request headers too large"));
        }
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return Err((400, "incomplete request")),
            Ok(n) => data.extend_from_slice(&buf[..n]),
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut start = lines.next().unwrap_or("").split(' ');
    let (method, target) = (start.next().unwrap_or(""), start.next().unwrap_or("/"));
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let mut req = Request {
        method: method.to_string(),
        target: target.to_string(),
        headers,
        body: data[head_end + 4..].to_vec(),
    };
    let len: usize = req
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        return Err((413, "request body too large"));
    }
    while req.body.len() < len {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return Err((400, "incomplete request body")),
            Ok(n) => req.body.extend_from_slice(&buf[..n]),
        }
    }
    req.body.truncate(len);
    Ok(req)
}

fn is_loopback(authority: &str, port: u16) -> bool {
    let host = authority
        .strip_suffix(&format!(":{port}"))
        .unwrap_or(authority);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

// Any web page can send requests to loopback. The Host check stops DNS rebinding, and
// requiring JSON with no foreign Origin stops cross-site posts from changing rules.
fn reject(req: &Request, port: u16) -> Option<(u16, &'static str)> {
    if req.header("host").is_some_and(|h| !is_loopback(h, port)) {
        return Some((403, "forbidden host"));
    }
    if req.method != "POST" {
        return None;
    }
    if let Some(origin) = req.header("origin")
        && !origin
            .strip_prefix("http://")
            .is_some_and(|a| is_loopback(a, port))
    {
        return Some((403, "forbidden origin"));
    }
    let json = req.header("content-type").is_some_and(|ct| {
        ct.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("application/json")
    });
    if !json {
        return Some((415, "content-type must be application/json"));
    }
    None
}

fn route(store: &FlowStore, cfg: &Shared, req: &Request) -> (u16, String) {
    let path = req.target.split('?').next().unwrap_or("");
    match req.method.as_str() {
        "GET" => {
            let (code, body) = handle(store, &req.target);
            (code, serde_json::to_string_pretty(&body).unwrap_or_default())
        }
        "POST" if path == "/mcp" => match serde_json::from_slice::<Value>(&req.body) {
            Ok(request) => match crate::services::mcp::handle(store, cfg, request) {
                Some(response) => (200, response.to_string()),
                None => (202, String::new()),
            },
            Err(_) => (
                400,
                json!({"jsonrpc": "2.0", "id": Value::Null, "error": {"code": -32700, "message": "Parse error"}})
                    .to_string(),
            ),
        },
        _ => (405, json!({"error": "method not allowed"}).to_string()),
    }
}

fn respond(stream: &mut impl Write, code: u16, body: &str) {
    let reason = match code {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
}

pub fn port_for(proxy_port: u16) -> u16 {
    proxy_port.wrapping_add(1)
}

// Loopback JSON API: read-only REST for captured flows, plus `POST /mcp`, which can also
// replay requests and edit rules.
pub fn serve(port: u16, store: FlowStore, cfg: Shared) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else {
        return;
    };
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let (store, cfg) = (store.clone(), cfg.clone());
            // one thread per connection so a slow MCP replay doesn't stall other callers
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let (code, body) = match read_request(&mut stream) {
                    Ok(req) => match reject(&req, port) {
                        Some((code, msg)) => (code, json!({"error": msg}).to_string()),
                        None => route(&store, &cfg, &req),
                    },
                    Err((code, msg)) => (code, json!({"error": msg}).to_string()),
                };
                respond(&mut stream, code, &body);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(raw: &str) -> Result<Request, (u16, &'static str)> {
        read_request(&mut std::io::Cursor::new(raw.as_bytes().to_vec()))
    }

    const MCP: &str = "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:9091\r\nContent-Type: application/json\r\nContent-Length: 2\r\n";

    #[test]
    fn flow_query_is_percent_decoded() {
        let store = FlowStore::default();
        for (id, host) in [(1, "a.com"), (2, "b.com")] {
            let mut f = Flow::sample(id);
            f.host = host.into();
            store.push(f);
        }
        let (code, v) = handle(&store, "/flows?q=host%3Ab%2Ecom");
        assert_eq!(code, 200);
        let ids: Vec<u64> = v.as_array().unwrap().iter().map(|f| f["id"].as_u64().unwrap()).collect();
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn reads_body_up_to_content_length() {
        let r = req(&format!("{MCP}\r\n{{}}trailing")).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.body, b"{}");
        assert_eq!(reject(&r, 9091), None);
    }

    #[test]
    fn oversized_body_is_refused_before_reading() {
        let raw = format!(
            "POST /mcp HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(req(&raw).err().unwrap().0, 413);
    }

    #[test]
    fn cross_site_posts_are_rejected() {
        let origin = req(&format!("{MCP}Origin: https://evil.test\r\n\r\n{{}}")).unwrap();
        assert_eq!(reject(&origin, 9091).unwrap().0, 403);
        let text = req("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:9091\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        assert_eq!(reject(&text, 9091).unwrap().0, 415);
        let local = req(&format!("{MCP}Origin: http://localhost:9091\r\n\r\n{{}}")).unwrap();
        assert_eq!(reject(&local, 9091), None);
    }

    #[test]
    fn rebound_host_is_rejected_for_reads_too() {
        let r = req("GET /flows HTTP/1.1\r\nHost: evil.test:9091\r\n\r\n").unwrap();
        assert_eq!(reject(&r, 9091).unwrap().0, 403);
        let ok = req("GET /flows HTTP/1.1\r\nHost: localhost:9091\r\n\r\n").unwrap();
        assert_eq!(reject(&ok, 9091), None);
    }
}
