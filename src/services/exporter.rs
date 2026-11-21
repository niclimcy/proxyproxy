use crate::domain::flow::Flow;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn curl(f: &Flow) -> String {
    let mut out = format!("curl -X {} {}", f.method, sh_quote(&f.url()));
    for (k, v) in &f.request.headers {
        if matches!(
            k.to_ascii_lowercase().as_str(),
            "content-length" | "host" | "accept-encoding"
        ) {
            continue;
        }
        out.push_str(&format!(" \\\n  -H {}", sh_quote(&format!("{k}: {v}"))));
    }
    if !f.request.body.is_empty() {
        out.push_str(&format!(
            " \\\n  --data-raw {}",
            sh_quote(&String::from_utf8_lossy(&f.request.body))
        ));
    }
    out
}

// Civil-from-days (Howard Hinnant) to avoid a date-time dependency
fn iso8601(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let (secs, ms) = (d.as_secs() as i64, d.subsec_millis());
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (month <= 2) as i64;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn hdrs(h: &[(String, String)]) -> Vec<Value> {
    h.iter()
        .map(|(k, v)| json!({"name": k, "value": v}))
        .collect()
}

fn entry(f: &Flow) -> Value {
    let query: Vec<Value> = f
        .path
        .split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    json!({"name": k, "value": v})
                })
                .collect()
        })
        .unwrap_or_default();
    let req_mime = f.request.header("content-type").unwrap_or("");
    let mut request = json!({
        "method": f.method, "url": f.url(), "httpVersion": f.version,
        "cookies": [], "headers": hdrs(&f.request.headers), "queryString": query,
        "headersSize": -1, "bodySize": f.request.body.len(),
    });
    if !f.request.body.is_empty() {
        request["postData"] =
            json!({"mimeType": req_mime, "text": String::from_utf8_lossy(&f.request.body)});
    }
    let response = match &f.response {
        Some(r) => json!({
            "status": f.status.unwrap_or(0), "statusText": "", "httpVersion": f.version,
            "cookies": [], "headers": hdrs(&r.headers),
            "content": {"size": r.body.len(), "mimeType": r.header("content-type").unwrap_or(""),
                        "text": String::from_utf8_lossy(&r.body)},
            "redirectURL": r.header("location").unwrap_or(""), "headersSize": -1, "bodySize": r.body.len(),
        }),
        None => json!({
            "status": 0, "statusText": "", "httpVersion": f.version, "cookies": [], "headers": [],
            "content": {"size": 0, "mimeType": ""}, "redirectURL": "", "headersSize": -1, "bodySize": 0,
        }),
    };
    let t = f.duration_ms.unwrap_or(0);
    json!({
        "startedDateTime": iso8601(f.started), "time": t, "request": request, "response": response,
        "cache": {}, "timings": {"send": 0, "wait": t, "receive": 0},
    })
}

pub fn har(flows: &[&Flow]) -> String {
    let log = json!({"log": {
        "version": "1.2",
        "creator": {"name": "proxyproxy", "version": env!("CARGO_PKG_VERSION")},
        "entries": flows.iter().map(|f| entry(f)).collect::<Vec<_>>(),
    }});
    serde_json::to_string_pretty(&log).unwrap_or_default()
}

fn skip_hdr(k: &str) -> bool {
    matches!(
        k.to_ascii_lowercase().as_str(),
        "content-length" | "host" | "accept-encoding" | "connection"
    )
}

fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

pub fn fetch(f: &Flow) -> String {
    let hs: Vec<String> = f
        .request
        .headers
        .iter()
        .filter(|(k, _)| !skip_hdr(k))
        .map(|(k, v)| format!("    {}: {}", js_str(k), js_str(v)))
        .collect();
    let mut o = format!(
        "fetch({}, {{\n  method: {},\n  headers: {{\n{}\n  }}",
        js_str(&f.url()),
        js_str(&f.method),
        hs.join(",\n")
    );
    if !f.request.body.is_empty() {
        o.push_str(&format!(",\n  body: {}", js_str(&f.request.text())));
    }
    o.push_str("\n});");
    o
}

pub fn python(f: &Flow) -> String {
    let hs: Vec<String> = f
        .request
        .headers
        .iter()
        .filter(|(k, _)| !skip_hdr(k))
        .map(|(k, v)| format!("    {}: {},", js_str(k), js_str(v)))
        .collect();
    let mut o = format!(
        "import requests\n\nres = requests.request(\n    {},\n    {},\n    headers={{\n{}\n    }}",
        js_str(&f.method),
        js_str(&f.url()),
        hs.join("\n").replace("\n    ", "\n        ")
    );
    if !f.request.body.is_empty() {
        o.push_str(&format!(",\n    data={}", js_str(&f.request.text())));
    }
    o.push_str(",\n)\nprint(res.status_code, res.text)");
    o
}

pub fn httpie(f: &Flow) -> String {
    let mut o = format!("http {} {}", f.method, sh_quote(&f.url()));
    for (k, v) in f.request.headers.iter().filter(|(k, _)| !skip_hdr(k)) {
        o.push_str(&format!(" {}", sh_quote(&format!("{k}:{v}"))));
    }
    if !f.request.body.is_empty() {
        o = format!("printf %s {} | {o}", sh_quote(&f.request.text()));
    }
    o
}

pub fn raw_http(f: &Flow) -> String {
    let mut o = format!("{} {} {}\nHost: {}\n", f.method, f.path, f.version, f.host);
    for (k, v) in f
        .request
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("host"))
    {
        o.push_str(&format!("{k}: {v}\n"));
    }
    o.push('\n');
    o.push_str(&f.request.text());
    o
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn csv(flows: &[&Flow]) -> String {
    let mut o = String::from("id,method,status,host,path,app,duration_ms,size,tag,comment\n");
    for f in flows {
        let row = [
            f.id.to_string(),
            f.method.clone(),
            f.status.map(|s| s.to_string()).unwrap_or_default(),
            f.host.clone(),
            f.path.clone(),
            f.app_label(),
            f.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
            f.res_size().to_string(),
            f.tag.clone(),
            f.comment.clone(),
        ];
        o.push_str(
            &row.iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        o.push('\n');
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flow::Message;

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
            pinned: false,
            comment: String::new(),
            color: None,
            is_ws: false,
            ws: vec![],
            error: None,
        }
    }

    #[test]
    fn csv_escapes_cells() {
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn curl_contains_method_url_header_and_body() {
        let c = curl(&flow());
        for want in ["curl", "POST", "https://api.x.com/v1?a=1", "X-A", "hi"] {
            assert!(c.contains(want), "{want} missing in {c}");
        }
    }

    #[test]
    fn csv_has_header_plus_one_row_per_flow() {
        let f = flow();
        assert_eq!(csv(&[&f, &f]).lines().count(), 3);
    }
}
