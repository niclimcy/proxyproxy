use serde::{Deserialize, Serialize};

/// Where a Map Local rule's content lives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapStorage {
    /// `path` is the user's own file.
    #[default]
    Disk,
    /// proxyproxy serves its own copy, `managed`, from its data folder.
    AppData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Breakpoint {
        request: bool,
        response: bool,
    },
    MapLocal {
        /// The user's file in `Disk` mode; in `AppData` mode the file to go back to, if any.
        path: String,
        status: u16,
        #[serde(default)]
        storage: MapStorage,
        /// File name under the data folder's `mocks/`. Kept while the rule is on `Disk`, so
        /// switching back finds the copy again. Never shared between rules.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        managed: String,
        /// Base64 of the managed file, only inside exported rules.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedded: Option<String>,
    },
    MapRemote {
        target: String,
    },
    Block {
        status: u16,
    },
    Delay {
        ms: u64,
        kbps: u64,
    },
    SetHeader {
        response: bool,
        name: String,
        value: String,
        remove: bool,
    },
    ReplaceBody {
        response: bool,
        find: String,
        replace: String,
        regex: bool,
    },
    Mock {
        status: u16,
        content_type: String,
        body: String,
    },
    Script {
        code: String,
    },
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

    /// The managed file a Map Local rule owns, if any.
    pub fn managed_file(&self) -> Option<&str> {
        match self {
            Action::MapLocal { managed, .. } if !managed.is_empty() => Some(managed),
            _ => None,
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
            Action::MapLocal {
                path,
                status,
                storage: MapStorage::Disk,
                ..
            } => format!("Serve {path} with status {status}"),
            Action::MapLocal {
                status, managed, ..
            } => format!("Serve stored file {managed} with status {status}"),
            Action::MapRemote { target } => format!("Redirect to {target}"),
            Action::Block { status } => format!("Reply {status} without contacting the server"),
            Action::Delay { ms, kbps } => {
                format!(
                    "Add {ms} ms latency{}",
                    if *kbps > 0 {
                        format!(", cap {kbps} KB/s")
                    } else {
                        String::new()
                    }
                )
            }
            Action::SetHeader {
                response,
                name,
                value,
                remove,
            } => format!(
                "{} {} header {name}{}",
                if *remove { "Remove" } else { "Set" },
                if *response { "response" } else { "request" },
                if *remove {
                    String::new()
                } else {
                    format!(": {value}")
                }
            ),
            Action::ReplaceBody {
                response,
                find,
                replace,
                regex,
            } => format!(
                "In {} body replace {}{find} with {replace}",
                if *response { "response" } else { "request" },
                if *regex { "regex " } else { "" }
            ),
            Action::Mock {
                status,
                content_type,
                body,
            } => format!("Reply {status} {content_type} ({} bytes)", body.len()),
            Action::Script { code } => format!("JS script ({} lines)", code.lines().count()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleCollection {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
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
    /// `None` is "Ungrouped", which is always active.
    #[serde(default)]
    pub collection: Option<u64>,
    #[serde(skip)]
    pub hits: u64,
}
