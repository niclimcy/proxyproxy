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
    h.iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect()
}

pub fn header_map(h: &[(String, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in h {
        if let (Ok(k), Ok(v)) = (HeaderName::try_from(k.trim()), HeaderValue::try_from(v.trim())) {
            map.append(k, v);
        }
    }
    map
}

pub fn remap(orig: &Uri, target: &str) -> Option<Uri> {
    let t: Uri = target.trim().parse().ok()?;
    let auth = t.authority()?.clone();
    let scheme = t.scheme_str().or(orig.scheme_str()).unwrap_or("https").to_string();
    let path = if t.path() == "/" { orig.path() } else { t.path() };
    let pq = match t.query().or(orig.query()) {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    Uri::builder().scheme(scheme.as_str()).authority(auth).path_and_query(pq).build().ok()
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
    let Ok(n) = HeaderName::try_from(name.trim()) else { return };
    h.remove(&n);
    if !remove {
        if let Ok(v) = HeaderValue::try_from(value.trim()) {
            h.insert(n, v);
        }
    }
}

pub fn replace_body(body: &[u8], find: &str, with: &str, regex: bool) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(body).ok()?;
    let out = if regex { regex::Regex::new(find).ok()?.replace_all(text, with).into_owned() } else { text.replace(find, with) };
    Some(out.into_bytes())
}

// drops the default port so "a.com:443" and "a.com" are one host
pub fn strip_default_port(scheme: &str, host: String) -> String {
    match (scheme, host.rsplit_once(':')) {
        ("https", Some((h, "443"))) | ("http", Some((h, "80"))) => h.to_string(),
        _ => host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_replace_and_binary_body_untouched() {
        assert_eq!(replace_body(b"a1b22", r"\d+", "#", true).unwrap(), b"a#b#");
        assert!(replace_body(&[0xff, 0xfe], "a", "b", false).is_none());
        assert!(replace_body(b"x", "(", "b", true).is_none());
    }

    #[test]
    fn default_ports_are_stripped() {
        assert_eq!(strip_default_port("https", "a.com:443".into()), "a.com");
        assert_eq!(strip_default_port("http", "a.com:80".into()), "a.com");
        assert_eq!(strip_default_port("http", "a.com:443".into()), "a.com:443");
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
        assert_eq!(remap(&orig, "http://b.com:8080").unwrap().to_string(), "http://b.com:8080/x?y=1");
        assert_eq!(remap(&orig, "http://b.com/other").unwrap().to_string(), "http://b.com/other?y=1");
        assert!(remap(&orig, "not a uri").is_none());
    }
}
