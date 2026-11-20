use crate::domain::flow::Message;
use base64::Engine;
use serde_json::Value;

#[derive(Clone, Copy, Default)]
pub struct Ctx<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub response: bool,
    // for responses: the paired request was GraphQL
    pub gql_req: bool,
}

pub fn gql_report(m: &Message, c: &Ctx) -> Option<String> {
    if c.response {
        return if c.gql_req || c.path.contains("graphql") { crate::services::graphql::response_report(m) } else { None };
    }
    crate::services::graphql::parse_request(c.method, c.path, m).map(|ops| crate::services::graphql::request_report(&ops))
}

pub fn is_graphql_request(method: &str, path: &str, m: &Message) -> bool {
    crate::services::graphql::parse_request(method, path, m).is_some()
}

pub fn parse_form(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let b = s.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 + 0 {
            if let Ok(v) = u8::from_str_radix(&String::from_utf8_lossy(&b[i + 1..i + 3]), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn is_json(m: &Message) -> bool {
    !m.body.is_empty() && serde_json::from_slice::<Value>(&m.body).is_ok()
}

pub fn is_form(m: &Message) -> bool {
    m.content_type().contains("x-www-form-urlencoded")
}

pub fn is_protobuf(m: &Message) -> bool {
    let ct = m.content_type();
    ct.contains("protobuf") || ct.contains("grpc")
}

pub fn schema_decode(m: &Message, c: &Ctx, forced: Option<&str>) -> Option<crate::services::schemas::Decoded> {
    if !crate::services::schemas::is_loaded() || is_json(m) || m.body.is_empty() {
        return None;
    }
    crate::services::schemas::decode(m, c.path, c.response, forced)
}

pub fn jwts(m: &Message) -> Vec<String> {
    let mut found = vec![];
    let mut scan = |s: &str| {
        for w in s.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))) {
            let parts: Vec<&str> = w.split('.').collect();
            if parts.len() == 3 && parts[0].starts_with("eyJ") && parts[1].len() > 4 && !found.contains(&w.to_string()) {
                found.push(w.to_string());
            }
        }
    };
    for (k, v) in &m.headers {
        if matches!(k.to_ascii_lowercase().as_str(), "authorization" | "cookie" | "set-cookie") {
            scan(v);
        }
    }
    if m.body.len() < 64 * 1024 {
        scan(&m.text());
    }
    found
}

pub fn decode_jwt(t: &str) -> Option<(Value, Value)> {
    let b = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut it = t.split('.');
    let h = serde_json::from_slice(&b.decode(it.next()?).ok()?).ok()?;
    let p = serde_json::from_slice(&b.decode(it.next()?).ok()?).ok()?;
    Some((h, p))
}

pub fn pretty_text(m: &Message) -> String {
    if m.body.is_empty() {
        return String::new();
    }
    if let Ok(v) = serde_json::from_slice::<Value>(&m.body) {
        return serde_json::to_string_pretty(&v).unwrap_or_default();
    }
    match std::str::from_utf8(&m.body) {
        Ok(s) => {
            if m.content_type().contains("xml") || m.content_type().contains("html") { indent_markup(s) } else { s.to_string() }
        }
        Err(_) => format!("<{} bytes binary — see Hex>", m.body.len()),
    }
}

fn indent_markup(s: &str) -> String {
    if s.contains('\n') {
        return s.to_string();
    }
    let (mut out, mut depth) = (String::new(), 0usize);
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else { break };
        let tag = &rest[i..i + j + 1];
        if tag.starts_with("</") {
            depth = depth.saturating_sub(1);
        }
        if !out.ends_with('\n') && !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&"  ".repeat(depth));
        out.push_str(tag);
        if !tag.starts_with("</") && !tag.ends_with("/>") && !tag.starts_with("<?") && !tag.starts_with("<!") {
            depth += 1;
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

// Best-effort schemaless protobuf wire decode
pub fn protobuf(data: &[u8], depth: usize) -> Option<String> {
    let (mut i, mut out) = (0, String::new());
    let pad = "  ".repeat(depth);
    let varint = |i: &mut usize| -> Option<u64> {
        let (mut v, mut s) = (0u64, 0);
        loop {
            let b = *data.get(*i)?;
            *i += 1;
            v |= ((b & 0x7f) as u64).checked_shl(s)?;
            if b & 0x80 == 0 {
                return Some(v);
            }
            s += 7;
            if s > 63 {
                return None;
            }
        }
    };
    if data.is_empty() {
        return None;
    }
    while i < data.len() {
        let key = varint(&mut i)?;
        let (field, wt) = (key >> 3, key & 7);
        if field == 0 {
            return None;
        }
        match wt {
            0 => out.push_str(&format!("{pad}{field}: {}\n", varint(&mut i)?)),
            1 => {
                let b: [u8; 8] = data.get(i..i + 8)?.try_into().ok()?;
                i += 8;
                out.push_str(&format!("{pad}{field}: {} (fixed64)\n", u64::from_le_bytes(b)));
            }
            5 => {
                let b: [u8; 4] = data.get(i..i + 4)?.try_into().ok()?;
                i += 4;
                out.push_str(&format!("{pad}{field}: {} (fixed32)\n", u32::from_le_bytes(b)));
            }
            2 => {
                let n = varint(&mut i)? as usize;
                let sub = data.get(i..i.checked_add(n)?)?;
                i += n;
                if let Some(nested) = (depth < 6).then(|| protobuf(sub, depth + 1)).flatten().filter(|_| !sub.is_empty()) {
                    out.push_str(&format!("{pad}{field} {{\n{nested}{pad}}}\n"));
                } else if let Ok(s) = std::str::from_utf8(sub) {
                    out.push_str(&format!("{pad}{field}: {s:?}\n"));
                } else {
                    out.push_str(&format!("{pad}{field}: <{n} bytes>\n"));
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

pub fn query_params(path: &str) -> Vec<(String, String)> {
    path.split_once('?').map(|(_, q)| parse_form(q.split('#').next().unwrap_or(q))).unwrap_or_default()
}

pub fn diff_text(a: &str, b: &str) -> Vec<(char, String)> {
    use similar::{ChangeTag, TextDiff};
    TextDiff::from_lines(a, b)
        .iter_all_changes()
        .map(|c| (match c.tag() { ChangeTag::Equal => ' ', ChangeTag::Delete => '-', ChangeTag::Insert => '+' }, c.value().trim_end_matches('\n').to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoders() {
        assert_eq!(parse_form("a=1%202&b=x+y"), vec![("a".into(), "1 2".into()), ("b".into(), "x y".into())]);
        assert_eq!(url_decode("100%"), "100%");
        let pb = protobuf(&[0x08, 0x96, 0x01, 0x12, 0x03, b'a', b'b', b'c'], 0).unwrap();
        assert!(pb.contains("1: 150") && pb.contains("2: \"abc\""), "{pb}");
        let (h, p) = decode_jwt("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig").unwrap();
        assert_eq!(h["alg"], "HS256");
        assert_eq!(p["sub"], "1");
        let d = diff_text("a\nb\n", "a\nc\n");
        assert!(d.contains(&('-', "b".into())) && d.contains(&('+', "c".into())));
    }
}
