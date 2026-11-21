use crate::domain::flow::Message;
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
        if matches!(k.to_ascii_lowercase().as_str(), "content-length" | "host") {
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
    let body = res.bytes().map_err(|e| e.to_string())?.to_vec();
    Ok(Sent {
        status,
        response: Message { headers, body },
        ms: t.elapsed().as_millis(),
    })
}
