use crate::domain::flow::Flow;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use crate::services::flow_store::FlowStore;


fn summary(f: &Flow) -> Value {
    json!({"id": f.id, "method": f.method, "url": f.url(), "status": f.status, "ms": f.duration_ms, "size": f.res_size(), "app": f.app_label(), "tag": f.tag})
}

fn detail(f: &Flow) -> Value {
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
            let query = get("q").map(|s| s.replace('+', " ").replace("%20", " ").replace("%3A", ":").replace("%3a", ":")).unwrap_or_default();
            let query = crate::domain::filter::Query::parse(&query);
            let limit = get("limit").and_then(|l| l.parse().ok()).unwrap_or(200usize);
            let list: Vec<Value> = flows.iter().rev().filter(|f| query.matches(f)).take(limit).map(summary).collect();
            (200, json!(list))
        }
        p if p.starts_with("/flows/") => match p[7..].parse::<u64>().ok().and_then(|id| flows.iter().find(|f| f.id == id)) {
            Some(f) => (200, detail(f)),
            None => (404, json!({"error": "no such flow"})),
        },
        "" | "/" => (200, json!({"name": "proxyproxy", "flows": flows.len(), "endpoints": ["/flows?q=&limit=", "/flows/{id}", "/har"]})),
        "/har" => {
            let all: Vec<&Flow> = flows.iter().collect();
            (200, serde_json::from_str(&crate::services::exporter::har(&all)).unwrap_or(Value::Null))
        }
        _ => (404, json!({"error": "not found"})),
    }
}

// Read-only JSON API on loopback so scripts and agents can query captured flows.
pub fn serve(port: u16, store: FlowStore) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else { return };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let line = req.lines().next().unwrap_or("");
            let mut it = line.split(' ');
            let (method, target) = (it.next().unwrap_or(""), it.next().unwrap_or("/"));
            let (code, body) = if method == "GET" { handle(&store, target) } else { (405, json!({"error": "GET only"})) };
            let body = serde_json::to_string_pretty(&body).unwrap_or_default();
            let _ = write!(
                stream,
                "HTTP/1.1 {code} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
}
