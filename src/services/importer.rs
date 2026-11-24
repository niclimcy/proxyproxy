use crate::domain::flow::{Flow, Message};
use serde_json::Value;
use std::time::SystemTime;

fn parse_hdrs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|h| {
                    Some((
                        h.get("name")?.as_str()?.to_string(),
                        h.get("value")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn import_har(text: &str) -> Result<Vec<Flow>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let entries = v
        .pointer("/log/entries")
        .and_then(|e| e.as_array())
        .ok_or("not a HAR file")?;
    let mut out = vec![];
    for e in entries {
        let req = &e["request"];
        let url = req["url"].as_str().unwrap_or("");
        let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
        let (host, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let res = &e["response"];
        let status = res["status"].as_u64().filter(|s| *s > 0).map(|s| s as u16);
        out.push(Flow {
            id: crate::domain::flow::next_id(),
            method: req["method"].as_str().unwrap_or("GET").into(),
            scheme: scheme.into(),
            host: host.into(),
            path: path.into(),
            version: req["httpVersion"].as_str().unwrap_or("HTTP/1.1").into(),
            started: SystemTime::now(),
            started_at: std::time::Instant::now(),
            request: Message {
                headers: parse_hdrs(&req["headers"]),
                body: req
                    .pointer("/postData/text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .as_bytes()
                    .to_vec(),
            },
            status,
            response: status.map(|_| Message {
                headers: parse_hdrs(&res["headers"]),
                body: match (
                    res.pointer("/content/text").and_then(|t| t.as_str()),
                    res.pointer("/content/encoding").and_then(|t| t.as_str()),
                ) {
                    (Some(t), Some("base64")) => {
                        use base64::Engine;
                        base64::engine::general_purpose::STANDARD
                            .decode(t)
                            .unwrap_or_default()
                    }
                    (Some(t), _) => t.as_bytes().to_vec(),
                    _ => vec![],
                },
            }),
            duration_ms: e["time"].as_f64().map(|t| t as u128),
            tag: "Imported".into(),
            app: String::new(),
            app_id: None,
            pinned: false,
            comment: String::new(),
            color: None,
            is_ws: false,
            ws: vec![],
            error: None,
        });
    }
    Ok(out)
}

const CHARLES_HINT: &str = "binary Charles .chls isn't supported — in Charles use File > Export Session… > JSON Session File (.chlsj) or HAR";

pub fn import_bytes(data: &[u8]) -> Result<Vec<Flow>, String> {
    let Ok(text) = std::str::from_utf8(data) else {
        return Err(CHARLES_HINT.into());
    };
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if !t.starts_with(['{', '[']) {
        return Err(CHARLES_HINT.into());
    }
    match import_har(t) {
        Ok(f) => Ok(f),
        Err(_) => import_chlsj(t),
    }
}

// Charles JSON session export (.chlsj)
pub fn import_chlsj(text: &str) -> Result<Vec<Flow>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let arr = v.as_array().ok_or("not a Charles JSON session")?;
    let hdr = |m: &Value| -> Vec<(String, String)> {
        m.pointer("/header/headers")
            .and_then(|h| h.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|h| {
                        Some((
                            h.get("name")?.as_str()?.to_string(),
                            h.get("value")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let body = |m: &Value| {
        m.pointer("/body/text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .as_bytes()
            .to_vec()
    };
    Ok(arr
        .iter()
        .map(|e| {
            let status = e["response"]["status"].as_u64().map(|s| s as u16);
            Flow {
                id: crate::domain::flow::next_id(),
                method: e["method"].as_str().unwrap_or("GET").into(),
                scheme: e["scheme"].as_str().unwrap_or("https").into(),
                host: e["host"].as_str().unwrap_or("").into(),
                path: format!(
                    "{}{}",
                    e["path"].as_str().unwrap_or("/"),
                    e["query"]
                        .as_str()
                        .map(|q| format!("?{q}"))
                        .unwrap_or_default()
                ),
                version: "HTTP/1.1".into(),
                started: SystemTime::now(),
                started_at: std::time::Instant::now(),
                request: Message {
                    headers: hdr(&e["request"]),
                    body: body(&e["request"]),
                },
                status,
                response: status.map(|_| Message {
                    headers: hdr(&e["response"]),
                    body: body(&e["response"]),
                }),
                duration_ms: e["durations"]["total"].as_u64().map(|d| d as u128),
                tag: "Imported".into(),
                app: String::new(),
                app_id: None,
                pinned: false,
                comment: String::new(),
                color: None,
                is_ws: false,
                ws: vec![],
                error: None,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHLSJ: &str = r#"[{"method":"POST","scheme":"https","host":"api.x.com","path":"/v1","query":"a=1",
        "request":{"header":{"headers":[{"name":"X-A","value":"b"}]},"body":{"text":"hi"}},
        "response":{"status":201,"header":{"headers":[]},"body":{"text":"ok"}},"durations":{"total":42}}]"#;

    #[test]
    fn charles_json_session_imports() {
        let f = import_bytes(CHLSJ.as_bytes()).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(
            (f[0].method.as_str(), f[0].host.as_str(), f[0].path.as_str()),
            ("POST", "api.x.com", "/v1?a=1")
        );
        assert_eq!(f[0].status, Some(201));
        assert_eq!(f[0].request.body, b"hi");
        assert_eq!(f[0].duration_ms, Some(42));
    }

    #[test]
    fn binary_chls_is_rejected_with_hint() {
        let err = import_bytes(&[0xC0, 0xDE, 0x00, 0xFF, 0x80]).unwrap_err();
        assert!(err.contains(".chlsj"));
        assert!(
            import_bytes(b"PK\x03\x04 not json")
                .unwrap_err()
                .contains(".chls")
        );
    }

    #[test]
    fn har_roundtrips_via_import() {
        let har = r#"{"log":{"entries":[{"request":{"method":"GET","url":"https://a.com/p?q=1","headers":[]},"response":{"status":200,"headers":[],"content":{"text":"x"}},"time":5}]}}"#;
        let f = import_bytes(har.as_bytes()).unwrap();
        assert_eq!(
            (f[0].host.as_str(), f[0].path.as_str(), f[0].status),
            ("a.com", "/p?q=1", Some(200))
        );
    }
}

#[cfg(test)]
mod characterization {
    use super::*;
    use crate::domain::flow::Message;
    use crate::services::exporter::har;

    fn flow() -> Flow {
        Flow {
            id: 1,
            method: "POST".into(),
            scheme: "https".into(),
            host: "api.x.com".into(),
            path: "/v1?a=1".into(),
            version: "HTTP/1.1".into(),
            started: std::time::SystemTime::now(),
            started_at: std::time::Instant::now(),
            request: Message {
                headers: vec![("X-A".into(), "b".into())],
                body: b"hi".to_vec(),
            },
            status: Some(200),
            response: Some(Message {
                headers: vec![("content-type".into(), "text/plain".into())],
                body: b"ok".to_vec(),
            }),
            duration_ms: Some(12),
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

    #[test]
    fn har_export_then_import_preserves_core_fields() {
        let f = flow();
        let back = import_har(&har(&[&f])).unwrap();
        assert_eq!(back.len(), 1);
        let b = &back[0];
        assert_eq!(
            (
                b.method.as_str(),
                b.host.as_str(),
                b.path.as_str(),
                b.status
            ),
            ("POST", "api.x.com", "/v1?a=1", Some(200))
        );
        assert_eq!(b.request.body, b"hi");
        assert_eq!(b.response.as_ref().unwrap().body, b"ok");
    }

    #[test]
    fn import_bytes_accepts_exported_har() {
        let f = flow();
        assert_eq!(import_bytes(har(&[&f]).as_bytes()).unwrap().len(), 1);
    }

    #[test]
    fn import_rejects_garbage() {
        assert!(import_bytes(b"not a session").is_err());
    }
}
