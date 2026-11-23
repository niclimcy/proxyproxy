use crate::domain::config::Shared;
use crate::domain::rule::{Action, Rule};
use crate::services::flow_store::FlowStore;
use crate::services::replay;
use serde_json::{Value, json};
use std::io::{BufRead, Write};

pub fn handle(store: &FlowStore, cfg: &Shared, request: Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error_response(id, -32600, "Invalid Request"));
    }
    let method = match request.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => return Some(error_response(id, -32600, "Invalid Request")),
    };
    let params = request.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "resources": {"subscribe": false, "listChanged": false},
                "tools": {"listChanged": false}
            },
            "serverInfo": {"name": "proxyproxy", "version": env!("CARGO_PKG_VERSION")}
        }),
        "ping" => json!({}),
        "resources/list" => {
            let st = store.lock();
            let mut resources = vec![json!({
                "uri": "proxyproxy://capture",
                "name": "Captured traffic",
                "description": "Capture summary and current flow count",
                "mimeType": "application/json"
            })];
            resources.extend(st.flows.iter().rev().take(200).map(|flow| {
                json!({
                    "uri": format!("proxyproxy://flows/{}", flow.id),
                    "name": format!("{} {}", flow.method, flow.url()),
                    "description": format!("Captured HTTP flow {}", flow.id),
                    "mimeType": "application/json"
                })
            }));
            json!({"resources": resources})
        }
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
            match read_resource(store, uri) {
                Some(contents) => contents,
                None => return Some(error_response(id, -32602, "Unknown resource")),
            }
        }
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(store, cfg, name, arguments) {
                Ok(value) => json!({
                    "content": [{"type": "text", "text": value.to_string()}],
                    "structuredContent": value
                }),
                Err(message) => json!({
                    "content": [{"type": "text", "text": message}],
                    "isError": true
                }),
            }
        }
        "notifications/initialized" => return None,
        _ => return Some(error_response(id, -32601, "Method not found")),
    };

    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn read_resource(store: &FlowStore, uri: &str) -> Option<Value> {
    if uri == "proxyproxy://capture" {
        let st = store.lock();
        return Some(json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": serde_json::to_string(&json!({
                    "flows": st.flows.len()
                })).unwrap_or_default()
            }]
        }));
    }
    if let Some(raw_id) = uri.strip_prefix("proxyproxy://flows/") {
        let id = raw_id.parse::<u64>().ok()?;
        let st = store.lock();
        let flow = st.flows.iter().find(|flow| flow.id == id)?;
        return Some(json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": serde_json::to_string_pretty(&crate::services::api::detail(flow))
                    .unwrap_or_default()
            }]
        }));
    }
    None
}

fn tools() -> Vec<Value> {
    vec![
        tool(
            "proxyproxy_list_flows",
            "List captured HTTP flows",
            json!({
                "type": "object",
                "properties": {
                    "q": {"type": "string", "description": "Proxyproxy search query"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20000}
                }
            }),
        ),
        tool(
            "proxyproxy_get_flow",
            "Get one captured HTTP flow",
            json!({
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }),
        ),
        tool(
            "proxyproxy_replay",
            "Replay an HTTP request directly",
            json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Existing flow to replay"},
                    "method": {"type": "string"},
                    "url": {"type": "string"},
                    "headers": {"type": "object", "additionalProperties": {"type": "string"}},
                    "body": {"type": "string"}
                }
            }),
        ),
        tool(
            "proxyproxy_list_rules",
            "List proxy rules",
            json!({
                "type": "object",
                "properties": {}
            }),
        ),
        tool(
            "proxyproxy_add_rule",
            "Add or replace a proxy rule",
            json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "name": {"type": "string"},
                    "pattern": {"type": "string"},
                    "method": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "action": {"type": "object"}
                },
                "required": ["pattern", "action"]
            }),
        ),
        tool(
            "proxyproxy_delete_rule",
            "Delete a proxy rule",
            json!({
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }),
        ),
        tool(
            "proxyproxy_export_har",
            "Export captured traffic as HAR",
            json!({
                "type": "object",
                "properties": {}
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": schema})
}

fn call_tool(
    store: &FlowStore,
    cfg: &Shared,
    name: &str,
    arguments: Value,
) -> Result<Value, String> {
    match name {
        "proxyproxy_list_flows" => {
            let query = arguments.get("q").and_then(Value::as_str).unwrap_or("");
            let limit = arguments
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .min(20_000) as usize;
            let parsed = crate::domain::filter::Query::parse(query);
            let st = store.lock();
            let flows: Vec<Value> = st
                .flows
                .iter()
                .rev()
                .filter(|flow| parsed.matches(flow))
                .take(limit)
                .map(crate::services::api::summary)
                .collect();
            Ok(json!(flows))
        }
        "proxyproxy_get_flow" => {
            let id = arguments
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| "id is required".to_string())?;
            let st = store.lock();
            let flow = st
                .flows
                .iter()
                .find(|flow| flow.id == id)
                .ok_or_else(|| "no such flow".to_string())?;
            Ok(crate::services::api::detail(flow))
        }
        "proxyproxy_replay" => replay_tool(store, arguments),
        "proxyproxy_list_rules" => {
            let rules = cfg.lock().unwrap().rules.clone();
            Ok(serde_json::to_value(rules).unwrap_or(json!([])))
        }
        "proxyproxy_add_rule" => add_rule(cfg, arguments),
        "proxyproxy_delete_rule" => {
            let id = arguments
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| "id is required".to_string())?;
            let mut cfg = cfg.lock().unwrap();
            let old_len = cfg.rules.len();
            cfg.rules.retain(|rule| rule.id != id);
            Ok(json!({"deleted": cfg.rules.len() != old_len}))
        }
        "proxyproxy_export_har" => {
            let st = store.lock();
            let flows: Vec<&crate::domain::flow::Flow> = st.flows.iter().collect();
            let har = crate::services::exporter::har(&flows);
            Ok(serde_json::from_str(&har).unwrap_or(Value::Null))
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn replay_tool(store: &FlowStore, arguments: Value) -> Result<Value, String> {
    let id = arguments.get("id").and_then(Value::as_u64);
    let (method, url, headers, body) = if let Some(id) = id {
        let st = store.lock();
        let flow = st
            .flows
            .iter()
            .find(|flow| flow.id == id)
            .ok_or_else(|| "no such flow".to_string())?;
        (
            flow.method.clone(),
            flow.url(),
            flow.request.headers.clone(),
            flow.request.body.clone(),
        )
    } else {
        let method = arguments
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_string();
        let url = arguments
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| "url is required".to_string())?
            .to_string();
        let headers = arguments
            .get("headers")
            .and_then(Value::as_object)
            .map(|headers| {
                headers
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .as_str()
                            .map(|value| (name.clone(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let body = arguments
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .as_bytes()
            .to_vec();
        (method, url, headers, body)
    };
    let sent = replay::send(&method, &url, &headers, &body)?;
    let flow = replay::replayed_flow(&method, &url, headers, body, &sent);
    store.push(flow.clone());
    Ok(crate::services::api::detail(&flow))
}

fn add_rule(cfg: &Shared, arguments: Value) -> Result<Value, String> {
    let pattern = arguments
        .get("pattern")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "pattern is required".to_string())?
        .to_string();
    let action_value = arguments
        .get("action")
        .ok_or_else(|| "action is required".to_string())?;
    let action: Action = serde_json::from_value(action_value.clone())
        .map_err(|error| format!("invalid action: {error}"))?;
    let mut cfg = cfg.lock().unwrap();
    let id = arguments
        .get("id")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| cfg.next_rule_id());
    let rule = Rule {
        id,
        enabled: arguments
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        name: arguments
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        pattern,
        method: arguments
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        action,
        hits: 0,
    };
    if let Some(existing) = cfg.rules.iter_mut().find(|existing| existing.id == id) {
        *existing = rule.clone();
    } else {
        cfg.rules.push(rule.clone());
    }
    Ok(serde_json::to_value(rule).unwrap_or(Value::Null))
}

// Bridges stdio MCP clients to the running app's HTTP endpoint, so both share one capture.
pub fn serve_stdio(port: u16) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => client
                .post(&url)
                .header("content-type", "application/json")
                .body(request.to_string())
                .send()
                .and_then(|response| response.text())
                .map_err(|error| format!("cannot reach proxyproxy on port {port}: {error}"))?,
            Err(_) => error_response(Value::Null, -32700, "Parse error").to_string(),
        };
        if response.trim().is_empty() {
            continue;
        }
        writeln!(stdout, "{response}").map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use std::sync::{Arc, Mutex};

    fn setup() -> (FlowStore, Shared) {
        (
            FlowStore::default(),
            Arc::new(Mutex::new(Config::default())),
        )
    }

    #[test]
    fn initialize_returns_mcp_capabilities() {
        let (store, cfg) = setup();
        let response = handle(
            &store,
            &cfg,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
        )
        .unwrap();
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["serverInfo"]["name"], "proxyproxy");
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_and_resources_are_listed() {
        let (store, cfg) = setup();
        let tools = handle(
            &store,
            &cfg,
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        )
        .unwrap();
        assert!(
            tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| { tool["name"] == "proxyproxy_list_flows" })
        );

        let resources = handle(
            &store,
            &cfg,
            json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"}),
        )
        .unwrap();
        assert!(
            resources["result"]["resources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|resource| { resource["uri"] == "proxyproxy://capture" })
        );
    }

    #[test]
    fn unknown_resource_is_a_json_rpc_error() {
        let (store, cfg) = setup();
        let response = handle(
            &store,
            &cfg,
            json!({"jsonrpc": "2.0", "id": 7, "method": "resources/read", "params": {"uri": "proxyproxy://nope"}}),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32602);
        assert!(response.get("result").is_none());
    }

    #[test]
    fn unknown_method_returns_json_rpc_error() {
        let (store, cfg) = setup();
        let response = handle(
            &store,
            &cfg,
            json!({"jsonrpc": "2.0", "id": 4, "method": "unknown"}),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn add_and_delete_rule_work_through_tools() {
        let (store, cfg) = setup();
        let add = handle(
            &store,
            &cfg,
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "proxyproxy_add_rule",
                    "arguments": {
                        "name": "Block API",
                        "pattern": "api.example.com",
                        "method": "GET",
                        "action": {"Block": {"status": 503}}
                    }
                }
            }),
        )
        .unwrap();
        assert_eq!(add["result"]["isError"], Value::Null);

        let rules = cfg.lock().unwrap().rules.clone();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "api.example.com");

        let delete = handle(
            &store,
            &cfg,
            json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "proxyproxy_delete_rule",
                    "arguments": {"id": rules[0].id}
                }
            }),
        )
        .unwrap();
        assert!(
            delete["result"]["structuredContent"]["deleted"]
                .as_bool()
                .unwrap()
        );
        assert!(cfg.lock().unwrap().rules.is_empty());
    }

    #[test]
    fn replay_records_a_flow_without_holding_the_store_lock() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            use std::io::{Read, Write};
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        });

        let (store, _cfg) = setup();
        let result = replay_tool(
            &store,
            json!({
                "method": "POST",
                "url": format!("http://127.0.0.1:{port}/echo"),
                "headers": {"Content-Type": "text/plain"},
                "body": "test"
            }),
        )
        .unwrap();
        assert_eq!(result["status"], 200);
        assert_eq!(store.lock().flows.len(), 1);
    }
}
