use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime};
use tokio::sync::oneshot;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn reserve_ids(above: u64) {
    NEXT_ID.fetch_max(above + 1, Ordering::Relaxed);
}

pub mod b64 {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<bytes::Bytes, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD
            .decode(s)
            .map(bytes::Bytes::from)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Message {
    pub headers: Vec<(String, String)>,
    #[serde(with = "b64")]
    pub body: bytes::Bytes,
}

impl Message {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn content_type(&self) -> String {
        self.header("content-type")
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase()
    }

    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Dir {
    Out,
    In,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsMsg {
    pub dir: Dir,
    pub binary: bool,
    pub ms: u128,
    #[serde(with = "b64")]
    pub data: bytes::Bytes,
}

fn now() -> Instant {
    Instant::now()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flow {
    pub id: u64,
    pub method: String,
    pub scheme: String,
    pub host: String,
    pub path: String,
    pub version: String,
    pub started: SystemTime,
    #[serde(skip, default = "now")]
    #[allow(dead_code)]
    pub started_at: Instant,
    pub request: Message,
    pub status: Option<u16>,
    pub response: Option<Message>,
    pub duration_ms: Option<u128>,
    pub tag: String,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub app_id: Option<crate::domain::app::AppIdentity>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub color: Option<u8>,
    #[serde(default)]
    pub is_ws: bool,
    #[serde(default)]
    pub ws: Vec<WsMsg>,
    #[serde(default)]
    pub error: Option<String>,
}

impl Flow {
    pub fn url(&self) -> String {
        format!("{}://{}{}", self.scheme, self.host, self.path)
    }

    /// Why this flow's response can't become a Mock rule; `None` when it can.
    /// Bodies the proxy could decode are stored decoded, so a non-UTF-8 body is binary or in an encoding it couldn't undo.
    pub fn mock_problem(&self) -> Option<&'static str> {
        match &self.response {
            None => Some("No response yet"),
            Some(r) if std::str::from_utf8(&r.body).is_err() => {
                Some("Compressed or binary response")
            }
            Some(_) => None,
        }
    }

    /// What a rule for this endpoint matches: host and path, no query.
    pub fn rule_pattern(&self) -> String {
        format!("{}{}", self.host, self.path_only())
    }

    pub fn path_only(&self) -> &str {
        self.path.split('?').next().unwrap_or("")
    }

    pub fn query_string(&self) -> &str {
        self.path.split_once('?').map_or("", |(_, q)| q)
    }

    pub fn hostname(&self) -> &str {
        match self.host.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h,
            _ => &self.host,
        }
    }

    pub fn res_size(&self) -> usize {
        self.response.as_ref().map_or(0, |r| r.body.len())
    }

    // Simulator page loads come from the shared WebKit networking process, not the Safari app.
    // Only Safari's UA has the `Safari/` token; a WKWebView inside another app omits it.
    fn is_sim_safari(&self) -> bool {
        let ua = self.request.header("user-agent").unwrap_or("");
        let shared_sim_process = self
            .app_id
            .as_ref()
            .is_some_and(|i| !i.platform.is_empty() && i.bundle_id.is_empty());
        shared_sim_process
            && (ua.contains("iPhone") || ua.contains("iPad"))
            && ua.contains("Safari/")
    }

    /// The identity to draw: the shared simulator process borrows Safari's icon for Safari page loads.
    pub fn display_identity(
        &self,
    ) -> Option<std::borrow::Cow<'_, crate::domain::app::AppIdentity>> {
        use std::borrow::Cow;
        let id = self.app_id.as_ref()?;
        if self.is_sim_safari() && !id.safari_icon_key.is_empty() {
            return Some(Cow::Owned(crate::domain::app::AppIdentity {
                name: "Safari".into(),
                icon_key: id.safari_icon_key.clone(),
                ..id.clone()
            }));
        }
        Some(Cow::Borrowed(id))
    }

    pub fn app_label(&self) -> std::borrow::Cow<'_, str> {
        let ua = self.request.header("user-agent").unwrap_or("");
        if self.is_sim_safari() {
            return "Safari".into();
        }
        if !self.app.is_empty() {
            return self.app.as_str().into();
        }
        for k in ["Edg", "Firefox", "Chrome", "Safari"] {
            if ua.contains(k) {
                return if k == "Edg" { "Edge" } else { k }.into();
            }
        }
        ua.split(['/', ' '])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .into()
    }

    pub fn same_endpoint(&self, o: &Flow) -> bool {
        self.method == o.method && self.host == o.host && self.path_only() == o.path_only()
    }

    #[cfg(test)]
    pub fn sample(id: u64) -> Flow {
        Flow {
            id,
            method: "GET".into(),
            scheme: "https".into(),
            host: "a.com".into(),
            path: "/".into(),
            version: "HTTP/1.1".into(),
            // old enough that the sidebar doesn't hold it back as unresolved
            started: SystemTime::now() - std::time::Duration::from_secs(60),
            started_at: Instant::now(),
            request: Message::default(),
            status: None,
            response: None,
            duration_ms: None,
            tag: String::new(),
            app: String::new(),
            app_id: None,
            pinned: false,
            comment: String::new(),
            color: None,
            is_ws: false,
            ws: vec![],
            error: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stage {
    Request,
    Response,
}

#[derive(Clone, Debug)]
pub struct BpEdit {
    pub method: String,
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: bytes::Bytes,
}

pub enum Decision {
    Resume(BpEdit),
    Abort,
}

pub struct Paused {
    pub id: u64,
    pub stage: Stage,
    pub edit: BpEdit,
    pub reply: oneshot::Sender<Decision>,
}

pub enum Event {
    Request(Box<Flow>),
    Edited {
        id: u64,
        method: String,
        host: String,
        path: String,
        request: Message,
    },
    Response {
        id: u64,
        status: u16,
        response: Message,
        duration_ms: u128,
    },
    Failed {
        id: u64,
        error: String,
    },
    Paused(Paused),
    Ws {
        id: u64,
        msg: WsMsg,
    },
    App {
        id: u64,
        app: String,
        identity: Option<crate::domain::app::AppIdentity>,
    },
    Log(String),
}
