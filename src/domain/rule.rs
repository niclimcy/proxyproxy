use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Breakpoint { request: bool, response: bool },
    MapLocal { path: String, status: u16 },
    MapRemote { target: String },
    Block { status: u16 },
    Delay { ms: u64, kbps: u64 },
    SetHeader { response: bool, name: String, value: String, remove: bool },
    ReplaceBody { response: bool, find: String, replace: String, regex: bool },
    Mock { status: u16, content_type: String, body: String },
    Script { code: String },
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Action::Breakpoint { .. } => "BREAKPOINT",
            Action::MapLocal { .. } => "MAP LOCAL",
            Action::MapRemote { .. } => "REDIRECT",
            Action::Block { .. } => "BLOCK",
            Action::Delay { .. } => "THROTTLE",
            Action::SetHeader { .. } => "HEADER",
            Action::ReplaceBody { .. } => "REWRITE BODY",
            Action::Mock { .. } => "MOCK",
            Action::Script { .. } => "SCRIPT",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Action::Breakpoint { request, response } => format!(
                "Pause on {}",
                match (request, response) {
                    (true, true) => "request and response",
                    (true, false) => "request",
                    _ => "response",
                }
            ),
            Action::MapLocal { path, status } => format!("Serve {path} with status {status}"),
            Action::MapRemote { target } => format!("Redirect to {target}"),
            Action::Block { status } => format!("Reply {status} without contacting the server"),
            Action::Delay { ms, kbps } => {
                format!("Add {ms} ms latency{}", if *kbps > 0 { format!(", cap {kbps} KB/s") } else { String::new() })
            }
            Action::SetHeader { response, name, value, remove } => format!(
                "{} {} header {name}{}",
                if *remove { "Remove" } else { "Set" },
                if *response { "response" } else { "request" },
                if *remove { String::new() } else { format!(": {value}") }
            ),
            Action::ReplaceBody { response, find, replace, regex } => format!(
                "In {} body replace {}{find} with {replace}",
                if *response { "response" } else { "request" },
                if *regex { "regex " } else { "" }
            ),
            Action::Mock { status, content_type, body } => format!("Reply {status} {content_type} ({} bytes)", body.len()),
            Action::Script { code } => format!("JS script ({} lines)", code.lines().count()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub id: u64,
    pub enabled: bool,
    #[serde(default)]
    pub name: String,
    pub pattern: String,
    pub method: String,
    pub action: Action,
    #[serde(skip)]
    pub hits: u64,
}
