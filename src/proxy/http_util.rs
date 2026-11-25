use bytes::Bytes;
use http_body_util::Full;
use hudsucker::{
    Body,
    hyper::{
        Response, Uri,
        header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue},
    },
};

pub fn headers(h: &HeaderMap) -> Vec<(String, String)> {
    h.iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect()
}

pub fn header_map(h: &[(String, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in h {
        if let (Ok(k), Ok(v)) = (
            HeaderName::try_from(k.trim()),
            HeaderValue::try_from(v.trim()),
        ) {
            map.append(k, v);
        }
    }
    map
}

pub fn remap(orig: &Uri, target: &str) -> Option<Uri> {
    let t: Uri = target.trim().parse().ok()?;
    let auth = t.authority()?.clone();
    let scheme = t
        .scheme_str()
        .or(orig.scheme_str())
        .unwrap_or("https")
        .to_string();
    let path = if t.path() == "/" {
        orig.path()
    } else {
        t.path()
    };
    let pq = match t.query().or(orig.query()) {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    Uri::builder()
        .scheme(scheme.as_str())
        .authority(auth)
        .path_and_query(pq)
        .build()
        .ok()
}

pub fn reply(status: u16, content_type: &str, body: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .header("access-control-allow-origin", "*")
        .body(Body::from(Full::new(Bytes::from(body))))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

pub fn text_response(status: u16, body: String) -> Response<Body> {
    reply(status, "text/plain; charset=utf-8", body.into_bytes())
}

pub fn set_header(h: &mut HeaderMap, name: &str, value: &str, remove: bool) {
    let Ok(n) = HeaderName::try_from(name.trim()) else {
        return;
    };
    h.remove(&n);
    if !remove && let Ok(v) = HeaderValue::try_from(value.trim()) {
        h.insert(n, v);
    }
}

pub fn replace_body(
    body: &[u8],
    find: &str,
    with: &str,
    regex: Option<&regex::Regex>,
) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(body).ok()?;
    // `None` also means nothing changed, so the caller doesn't flag an edit or copy the body
    let out = match regex {
        Some(re) => match re.replace_all(text, with) {
            std::borrow::Cow::Owned(s) => s,
            std::borrow::Cow::Borrowed(_) => return None,
        },
        None if text.contains(find) => text.replace(find, with),
        None => return None,
    };
    (out != text).then(|| out.into_bytes())
}

// drops the default port so "a.com:443" and "a.com" are one host
pub fn strip_default_port(scheme: &str, host: String) -> String {
    match (scheme, host.rsplit_once(':')) {
        ("https" | "wss", Some((h, "443"))) | ("http" | "ws", Some((h, "80"))) => h.to_string(),
        _ => host,
    }
}

/// `%XX` escapes only; `+` stays as is because it is literal outside form bodies.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_body_untouched() {
        assert!(replace_body(&[0xff, 0xfe], "a", "b", None).is_none());
    }

    #[test]
    fn default_ports_are_stripped() {
        assert_eq!(strip_default_port("https", "a.com:443".into()), "a.com");
        assert_eq!(strip_default_port("http", "a.com:80".into()), "a.com");
        assert_eq!(strip_default_port("http", "a.com:443".into()), "a.com:443");
        assert_eq!(strip_default_port("wss", "a.com:443".into()), "a.com");
        assert_eq!(strip_default_port("ws", "a.com:80".into()), "a.com");
    }

    #[test]
    fn set_header_remove_and_replace() {
        let mut h = HeaderMap::new();
        set_header(&mut h, "x-a", "1", false);
        set_header(&mut h, "x-a", "2", false);
        assert_eq!(h.get_all("x-a").iter().count(), 1);
        set_header(&mut h, "x-a", "", true);
        assert!(h.is_empty());
    }

    #[test]
    fn remap_keeps_path_unless_target_has_one() {
        let orig: Uri = "https://a.com/x?y=1".parse().unwrap();
        assert_eq!(
            remap(&orig, "http://b.com:8080").unwrap().to_string(),
            "http://b.com:8080/x?y=1"
        );
        assert_eq!(
            remap(&orig, "http://b.com/other").unwrap().to_string(),
            "http://b.com/other?y=1"
        );
        assert!(remap(&orig, "not a uri").is_none());
    }
}
