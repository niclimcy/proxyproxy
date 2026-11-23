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

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Message {
    pub headers: Vec<(String, String)>,
    #[serde(with = "b64")]
    pub body: Vec<u8>,
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

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
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
    pub data: Vec<u8>,
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

    pub fn path_only(&self) -> &str {
        self.path.split('?').next().unwrap_or("")
    }

    pub fn query_string(&self) -> &str {
        self.path.split_once('?').map(|(_, q)| q).unwrap_or("")
    }

    pub fn hostname(&self) -> &str {
        match self.host.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h,
            _ => &self.host,
        }
    }

    pub fn res_size(&self) -> usize {
        self.response.as_ref().map(|r| r.body.len()).unwrap_or(0)
    }

    pub fn app_label(&self) -> String {
        if !self.app.is_empty() {
            return self.app.clone();
        }
        let ua = self.request.header("user-agent").unwrap_or("");
        for k in ["Edg", "Firefox", "Chrome", "Safari"] {
            if ua.contains(k) {
                return match k {
                    "Edg" => "Edge".into(),
                    _ => k.into(),
                };
            }
        }
        ua.split(['/', ' '])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn endpoint_key(&self) -> String {
        format!("{} {}{}", self.method, self.host, self.path_only())
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
            started: SystemTime::now(),
            started_at: Instant::now(),
            request: Message::default(),
            status: None,
            response: None,
            duration_ms: None,
            tag: String::new(),
            app: String::new(),
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
    pub body: Vec<u8>,
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
    },
    Log(String),
}
