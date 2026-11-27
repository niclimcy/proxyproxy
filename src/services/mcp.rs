use crate::domain::config::Shared;
use crate::domain::rule::{Action, MapStorage, Rule};
use crate::services::flow_store::FlowStore;
use crate::services::mocks;
use crate::services::replay;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

pub const DISABLED_MESSAGE: &str = "proxyproxy's MCP server is turned off. Ask the user to open proxyproxy → Settings → MCP and enable it, then retry.";
pub const DISABLED_CODE: i32 = -32001;

pub fn handle(store: &FlowStore, cfg: &Shared, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error_response(&id, -32600, "Invalid Request"));
    }
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(error_response(&id, -32600, "Invalid Request"));
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
                None => return Some(error_response(&id, -32602, "Unknown resource")),
            }
        }
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params.get("arguments").unwrap_or(&Value::Null);
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
        _ => return Some(error_response(&id, -32601, "Method not found")),
    };

    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

pub(crate) fn error_response(id: &Value, code: i32, message: &str) -> Value {
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
            &json!({
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
            &json!({
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }),
        ),
        tool(
            "proxyproxy_replay",
            "Replay an HTTP request directly",
            &json!({
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
            &json!({
                "type": "object",
                "properties": {}
            }),
        ),
        tool(
            "proxyproxy_add_rule",
            "Add or replace a proxy rule. Map Local: action {\"MapLocal\": {path, status, storage}}; storage \"AppData\" keeps content in proxyproxy (path only names the file, e.g. response.json; pass content for text). Changing an existing disk rule to AppData without content copies the disk file's bytes; a file proxyproxy already stores for the rule is kept",
            &json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "name": {"type": "string"},
                    "pattern": {"type": "string"},
                    "method": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "collection": {"type": ["integer", "null"], "description": "Collection id from proxyproxy_list_collections; null for Ungrouped. Omitted on an existing rule keeps its collection"},
                    "action": {"type": "object", "description": "Map Local takes path, status and optionally storage: \"Disk\" (default; path is your file) or \"AppData\" (proxyproxy keeps its own copy)"},
                    "content": {"type": "string", "description": "Text content for a Map Local rule with AppData storage; an existing rule keeps its stored content when omitted"}
                },
                "required": ["pattern", "action"]
            }),
        ),
        tool(
            "proxyproxy_list_collections",
            "List rule collections with their rule counts",
            &json!({
                "type": "object",
                "properties": {}
            }),
        ),
        tool(
            "proxyproxy_set_collection_enabled",
            "Turn a rule collection on or off; its rules keep their own on/off state",
            &json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "enabled": {"type": "boolean"},
                    "solo": {"type": "boolean", "description": "Turn this collection on and all other collections off; enabled is ignored"}
                },
                "required": ["id"]
            }),
        ),
        tool(
            "proxyproxy_delete_rule",
            "Delete a proxy rule",
            &json!({
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }),
        ),
        tool(
            "proxyproxy_export_har",
            "Export captured traffic as HAR",
            &json!({
                "type": "object",
                "properties": {}
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, schema: &Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": schema})
}

fn call_tool(
    store: &FlowStore,
    cfg: &Shared,
    name: &str,
    arguments: &Value,
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
        "proxyproxy_list_collections" => {
            let cfg = cfg.lock().unwrap();
            let counts = cfg.collection_counts();
            let collections: Vec<Value> = cfg
                .collections
                .iter()
                .map(|c| {
                    let (rules, rules_on) = counts.get(&Some(c.id)).copied().unwrap_or_default();
                    json!({
                        "id": c.id,
                        "name": c.name,
                        "enabled": c.enabled,
                        "rules": rules,
                        "rules_on": rules_on
                    })
                })
                .collect();
            Ok(json!(collections))
        }
        "proxyproxy_set_collection_enabled" => {
            let id = arguments
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| "id is required".to_string())?;
            let solo = arguments.get("solo").and_then(Value::as_bool) == Some(true);
            let enabled = arguments
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| "enabled is required unless solo is true".to_string());
            let mut cfg = cfg.lock().unwrap();
            let found = if solo {
                cfg.solo_collection(id)
            } else {
                cfg.set_collection_enabled(id, enabled?)
            };
            if found {
                Ok(json!({"id": id, "collections": cfg.collections}))
            } else {
                Err("no such collection".into())
            }
        }
        "proxyproxy_delete_rule" => {
            let id = arguments
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| "id is required".to_string())?;
            let mut cfg = cfg.lock().unwrap();
            let gone = mocks::names(cfg.rules.iter().filter(|rule| rule.id == id));
            let old_len = cfg.rules.len();
            cfg.rules.retain(|rule| rule.id != id);
            cfg.doomed_mocks.extend(gone);
            Ok(json!({"deleted": cfg.rules.len() != old_len}))
        }
        "proxyproxy_export_har" => Ok(crate::services::api::har_json(store)),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn replay_tool(store: &FlowStore, arguments: &Value) -> Result<Value, String> {
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
            .map_or_default(|headers| {
                headers
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .as_str()
                            .map(|value| (name.clone(), value.to_string()))
                    })
                    .collect()
            });
        let body = arguments.get("body").and_then(Value::as_str).unwrap_or("");
        (
            method,
            url,
            headers,
            bytes::Bytes::copy_from_slice(body.as_bytes()),
        )
    };
    let sent = replay::send(&method, &url, &headers, &body)?;
    let flow = replay::replayed_flow(&method, &url, headers, body, &sent);
    store.push(flow.clone());
    Ok(crate::services::api::detail(&flow))
}

fn add_rule(shared: &Shared, arguments: &Value) -> Result<Value, String> {
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
    let mut action: Action = serde_json::from_value(action_value.clone())
        .map_err(|error| format!("invalid action: {error}"))?;
    if let (Action::MapLocal { embedded, .. }, Some(text)) = (
        &mut action,
        arguments.get("content").and_then(Value::as_str),
    ) {
        *embedded = Some(STANDARD.encode(text));
    }
    let wanted_id = arguments.get("id").and_then(Value::as_u64);
    let (previous, collection) = {
        let cfg = shared.lock().unwrap();
        let existing = wanted_id.and_then(|id| cfg.rules.iter().find(|r| r.id == id));
        let collection = match arguments.get("collection") {
            Some(Value::Number(n)) => {
                let c = n.as_u64().ok_or_else(|| "invalid collection".to_string())?;
                if !cfg.collection_exists(c) {
                    return Err(format!("no such collection: {c}"));
                }
                Some(c)
            }
            Some(Value::Null) => None,
            Some(_) => return Err("collection must be an integer or null".into()),
            None => existing.and_then(|r| r.collection),
        };
        (existing.map(|r| r.action.clone()), collection)
    };
    let keep = previous
        .as_ref()
        .and_then(Action::managed_file)
        .map(str::to_string);
    // moving a rule from a disk file to stored without new content copies the file, like the UI
    if let (
        Action::MapLocal {
            storage: MapStorage::AppData,
            embedded: embedded @ None,
            ..
        },
        Some(Action::MapLocal {
            storage: MapStorage::Disk,
            path,
            ..
        }),
        None,
    ) = (&mut action, &previous, &keep)
    {
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
        *embedded = Some(STANDARD.encode(bytes));
    }
    // file work happens before the config is locked again
    mocks::adopt(&mut action, keep.as_deref())?;
    let mut cfg = shared.lock().unwrap();
    let id = wanted_id.unwrap_or_else(|| cfg.next_rule_id());
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
        collection: collection.filter(|c| cfg.collection_exists(*c)),
        action,
        hits: 0,
    };
    if let Some(old) = keep.filter(|old| rule.action.managed_file() != Some(old.as_str())) {
        cfg.doomed_mocks.push(old);
    }
    if let Some(existing) = cfg.rules.iter_mut().find(|existing| existing.id == id) {
        *existing = rule.clone();
    } else {
        cfg.rules.push(rule.clone());
    }
    Ok(serde_json::to_value(rule).unwrap_or(Value::Null))
}

// What the stdio bridge does with the API's answer.
#[derive(Debug, PartialEq)]
enum Bridged {
    Print(String),
    Nothing,
    // a refusal for a request that had an id
    Failed(String),
}

// Refusals that aren't JSON-RPC (bad host, 413...) are wrapped so the client sees an
// error for its request rather than a stalled call.
fn bridge_reply(id: &Value, status: u16, body: &str) -> Bridged {
    let parsed = serde_json::from_str::<Value>(body).ok();
    if parsed.as_ref().is_some_and(|v| v.get("jsonrpc").is_some()) {
        return Bridged::Print(body.trim().to_string());
    }
    if status < 400 {
        return Bridged::Nothing;
    }
    let message = parsed
        .and_then(|v| v.get("error").and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| format!("proxyproxy returned HTTP {status}"));
    Bridged::Failed(error_response(id, -32000, &message).to_string())
}

// Bridges stdio MCP clients to the running app's HTTP endpoint, so both share one capture.
pub fn serve_stdio(port: u16) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(request) => {
                let id = request.get("id");
                let answered = client
                    .post(&url)
                    .header("content-type", "application/json")
                    .body(line.to_string())
                    .send()
                    .and_then(|r| Ok((r.status().as_u16(), r.text()?)));
                let reply = match answered {
                    Ok((status, body)) => bridge_reply(id.unwrap_or(&Value::Null), status, &body),
                    Err(error) => Bridged::Failed(
                        error_response(
                            id.unwrap_or(&Value::Null),
                            -32000,
                            &format!("cannot reach proxyproxy on port {port}: {error}"),
                        )
                        .to_string(),
                    ),
                };
                match reply {
                    // a notification has no id to answer, so its failure goes to stderr
                    Bridged::Failed(text) if id.is_none() => {
                        eprintln!("proxyproxy mcp: {text}");
                        Bridged::Nothing
                    }
                    Bridged::Failed(text) => Bridged::Print(text),
                    other => other,
                }
            }
            Err(_) => {
                Bridged::Print(error_response(&Value::Null, -32700, "Parse error").to_string())
            }
        };
        if let Bridged::Print(text) = reply {
            writeln!(stdout, "{text}").map_err(|error| error.to_string())?;
            stdout.flush().map_err(|error| error.to_string())?;
        }
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
    fn bridge_relays_the_disabled_error_and_wraps_other_refusals() {
        let disabled = error_response(&json!(3), DISABLED_CODE, DISABLED_MESSAGE).to_string();
        assert_eq!(
            bridge_reply(&json!(3), 403, &disabled),
            Bridged::Print(disabled)
        );
        assert_eq!(bridge_reply(&json!(3), 202, ""), Bridged::Nothing);
        let Bridged::Failed(wrapped) =
            bridge_reply(&json!(4), 403, r#"{"error":"forbidden host"}"#)
        else {
            panic!("expected a failure");
        };
        let v: Value = serde_json::from_str(&wrapped).unwrap();
        assert_eq!(
            (v["id"].clone(), v["error"]["message"].clone()),
            (json!(4), json!("forbidden host"))
        );
        assert!(
            matches!(bridge_reply(&json!(5), 500, "oops"), Bridged::Failed(t) if t.contains("HTTP 500"))
        );
    }

    #[test]
    fn initialize_returns_mcp_capabilities() {
        let (store, cfg) = setup();
        let response = handle(
            &store,
            &cfg,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
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
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
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
            &json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"}),
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
            &json!({"jsonrpc": "2.0", "id": 7, "method": "resources/read", "params": {"uri": "proxyproxy://nope"}}),
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
            &json!({"jsonrpc": "2.0", "id": 4, "method": "unknown"}),
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
            &json!({
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
            &json!({
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

    fn call(store: &FlowStore, cfg: &Shared, name: &str, arguments: &Value) -> Value {
        let response = handle(
            store,
            cfg,
            &json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            }),
        )
        .unwrap();
        response["result"].clone()
    }

    #[test]
    fn moving_a_disk_rule_to_stored_copies_the_file_and_deleting_queues_the_copy() {
        let (store, cfg) = setup();
        let disk = std::env::temp_dir().join(format!("pxp-mcp-disk-{}.json", std::process::id()));
        std::fs::write(&disk, [0u8, 159, 146, 150]).unwrap();
        let disk_action = json!({"MapLocal": {"path": disk.display().to_string(), "status": 200}});
        let added = call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"pattern": "a.com", "action": disk_action}),
        );
        let id = added["structuredContent"]["id"].as_u64().unwrap();

        let moved = call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"id": id, "pattern": "a.com", "action": {"MapLocal": {
                "path": "ignored.json", "status": 200, "storage": "AppData"}}}),
        );
        let name = moved["structuredContent"]["action"]["MapLocal"]["managed"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            mocks::Source::Stored(name.clone()).read().unwrap(),
            [0u8, 159, 146, 150]
        );

        call(&store, &cfg, "proxyproxy_delete_rule", &json!({"id": id}));
        assert_eq!(cfg.lock().unwrap().doomed_mocks, vec![name.clone()]);
        mocks::delete(&name);
        std::fs::remove_file(disk).ok();
    }

    #[test]
    fn collections_are_listed_toggled_and_assigned_through_tools() {
        let (store, cfg) = setup();
        let (a, b) = {
            let mut c = cfg.lock().unwrap();
            (c.add_collection("Errors"), c.add_collection("Slow"))
        };
        let block = json!({"Block": {"status": 503}});
        let added = call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"pattern": "a.com", "action": block, "collection": a}),
        );
        assert_eq!(added["structuredContent"]["collection"], a);
        let id = added["structuredContent"]["id"].clone();

        let listed = call(&store, &cfg, "proxyproxy_list_rules", &json!({}));
        assert_eq!(listed["structuredContent"][0]["collection"], a);

        let missing = call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"pattern": "b.com", "action": block, "collection": 99}),
        );
        assert_eq!(missing["isError"], true);

        // an update that omits the field leaves the rule where it is; null ungroups it
        call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"id": id, "pattern": "a.com", "action": block}),
        );
        assert_eq!(cfg.lock().unwrap().rules[0].collection, Some(a));
        call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"id": id, "pattern": "a.com", "action": block, "collection": null}),
        );
        assert_eq!(cfg.lock().unwrap().rules[0].collection, None);

        let counts = call(&store, &cfg, "proxyproxy_list_collections", &json!({}));
        assert_eq!(counts["structuredContent"][0]["name"], "Errors");
        assert_eq!(counts["structuredContent"][0]["rules"], 0);
        call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"pattern": "c.com", "action": block, "collection": b, "enabled": false}),
        );
        call(
            &store,
            &cfg,
            "proxyproxy_add_rule",
            &json!({"pattern": "d.com", "action": block, "collection": b}),
        );
        let counts = call(&store, &cfg, "proxyproxy_list_collections", &json!({}));
        assert_eq!(counts["structuredContent"][1]["rules"], 2);
        assert_eq!(counts["structuredContent"][1]["rules_on"], 1);

        call(
            &store,
            &cfg,
            "proxyproxy_set_collection_enabled",
            &json!({"id": a, "enabled": false}),
        );
        assert!(!cfg.lock().unwrap().collections[0].enabled);
        call(
            &store,
            &cfg,
            "proxyproxy_set_collection_enabled",
            &json!({"id": a, "solo": true}),
        );
        let on: Vec<bool> = cfg
            .lock()
            .unwrap()
            .collections
            .iter()
            .map(|c| c.enabled)
            .collect();
        assert_eq!(on, [true, false]);
        let bad = call(
            &store,
            &cfg,
            "proxyproxy_set_collection_enabled",
            &json!({"id": b + 10, "enabled": true}),
        );
        assert_eq!(bad["isError"], true);
    }
}
