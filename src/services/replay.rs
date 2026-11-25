use crate::domain::flow::{self, Flow, Message};
use reqwest::{
    Method,
    blocking::Client,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use std::time::{Duration, Instant};

pub struct Sent {
    pub status: u16,
    pub response: Message,
    pub ms: u128,
}

// Sends direct (no proxy) so replays don't loop back through us.
pub fn send(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<Sent, String> {
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let mut h = HeaderMap::new();
    for (k, v) in headers {
        // reqwest is built without decompression, so ask for an identity body
        if matches!(
            k.to_ascii_lowercase().as_str(),
            "content-length" | "host" | "accept-encoding"
        ) {
            continue;
        }
        if let (Ok(k), Ok(v)) = (
            HeaderName::try_from(k.trim()),
            HeaderValue::try_from(v.trim()),
        ) {
            h.append(k, v);
        }
    }
    let m =
        Method::from_bytes(method.trim().to_uppercase().as_bytes()).map_err(|e| e.to_string())?;
    let t = Instant::now();
    let res = client
        .request(m, url.trim())
        .headers(h)
        .body(body.to_vec())
        .send()
        .map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let headers = res
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = res.bytes().map_err(|e| e.to_string())?;
    Ok(Sent {
        status,
        response: Message { headers, body },
        ms: t.elapsed().as_millis(),
    })
}

fn split_url(url: &str) -> (&str, &str, &str) {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let (host, path) = rest
        .find(['/', '?'])
        .map_or((rest, "/"), |i| (&rest[..i], &rest[i..]));
    (scheme, host, path)
}

// Successful replays are recorded as flows so they show up next to captured traffic.
pub fn replayed_flow(
    method: &str,
    url: &str,
    headers: Vec<(String, String)>,
    body: bytes::Bytes,
    sent: &Sent,
) -> Flow {
    let (scheme, host, path) = split_url(url);
    Flow {
        id: flow::next_id(),
        method: method.trim().to_uppercase(),
        scheme: scheme.into(),
        host: host.into(),
        path: path.into(),
        version: "HTTP/1.1".into(),
        started: std::time::SystemTime::now(),
        started_at: std::time::Instant::now(),
        request: Message { headers, body },
        status: Some(sent.status),
        response: Some(sent.response.clone()),
        duration_ms: Some(sent.ms),
        tag: "Replayed".into(),
        app: "proxyproxy".into(),
        app_id: None,
        pinned: false,
        comment: String::new(),
        color: None,
        is_ws: false,
        ws: vec![],
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemeless_url_defaults_to_https_and_root_path() {
        assert_eq!(split_url("a.com"), ("https", "a.com", "/"));
        assert_eq!(split_url(" http://a.com/p "), ("http", "a.com", "/p"));
        assert_eq!(split_url("http://a.com?x=1"), ("http", "a.com", "?x=1"));
    }
}
