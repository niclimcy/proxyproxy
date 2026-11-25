use crate::domain::flow::Flow;
use crate::domain::policy::glob;
use regex::Regex;

#[derive(Debug)]
enum Op {
    Has(String),
    Wild(String),
    Re(Regex),
    Eq(String),
    Gt(f64),
    Ge(f64),
    Lt(f64),
    Le(f64),
}

#[derive(Debug)]
struct Term {
    field: String,
    neg: bool,
    op: Op,
}

#[derive(Debug, Default)]
pub struct Query {
    terms: Vec<Term>,
    pub error: Option<String>,
}

pub const FIELDS: &[&str] = &[
    "host", "path", "url", "method", "status", "app", "type", "size", "time", "body", "req", "res",
    "header", "query", "is", "gql", "comment", "tag", "platform",
];

fn tokens(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut q) = (vec![], String::new(), false);
    for c in s.chars() {
        match c {
            '"' => q = !q,
            c if c.is_whitespace() && !q => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_op(v: &str, numeric: bool) -> Result<Op, String> {
    if numeric {
        for (p, mk) in [
            (">=", Op::Ge as fn(f64) -> Op),
            ("<=", Op::Le),
            (">", Op::Gt),
            ("<", Op::Lt),
        ] {
            if let Some(n) = v.strip_prefix(p) {
                return n
                    .trim()
                    .parse()
                    .map(mk)
                    .map_err(|_| format!("bad number '{n}'"));
            }
        }
    }
    if let Some(r) = v.strip_prefix('/').and_then(|r| r.strip_suffix('/')) {
        return Regex::new(&format!("(?i){r}"))
            .map(Op::Re)
            .map_err(|e| e.to_string());
    }
    if let Some(e) = v.strip_prefix('=') {
        return Ok(Op::Eq(e.to_lowercase()));
    }
    if v.contains('*') || v.contains('?') {
        return Ok(Op::Wild(v.to_lowercase()));
    }
    Ok(Op::Has(v.to_lowercase()))
}

impl Query {
    pub fn parse(s: &str) -> Query {
        let mut q = Query::default();
        for tok in tokens(s) {
            let (neg, t) = match tok.strip_prefix('-') {
                Some(r) if !r.is_empty() => (true, r.to_string()),
                _ => (false, tok.clone()),
            };
            let (field, val) = match t.split_once(':') {
                Some((f, v)) if FIELDS.contains(&f.to_lowercase().as_str()) => {
                    (f.to_lowercase(), v.to_string())
                }
                _ => ("any".to_string(), t.clone()),
            };
            let numeric = matches!(field.as_str(), "status" | "size" | "time");
            match parse_op(&val, numeric) {
                Ok(op) => q.terms.push(Term { field, neg, op }),
                Err(e) => q.error = Some(format!("{field}: {e}")),
            }
        }
        q
    }

    pub fn matches(&self, f: &Flow) -> bool {
        self.terms.iter().all(|t| term_matches(t, f) != t.neg)
    }
}

fn text_op(op: &Op, hay: &str) -> bool {
    match op {
        Op::Has(n) => hay.to_lowercase().contains(n),
        Op::Wild(p) => glob(p.as_bytes(), hay.to_lowercase().as_bytes()),
        Op::Re(r) => r.is_match(hay),
        Op::Eq(e) => hay.to_lowercase() == *e,
        _ => false,
    }
}

fn num_op(op: &Op, v: f64) -> bool {
    match op {
        Op::Gt(n) => v > *n,
        Op::Ge(n) => v >= *n,
        Op::Lt(n) => v < *n,
        Op::Le(n) => v <= *n,
        Op::Has(n) | Op::Eq(n) => n.parse::<f64>().is_ok_and(|n| n == v),
        _ => false,
    }
}

fn gql_op(f: &Flow) -> String {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&f.request.body) else {
        return String::new();
    };
    let one = |v: &serde_json::Value| {
        let name = v
            .get("operationName")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let q = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
        format!("{name} {q}")
    };
    match &v {
        serde_json::Value::Array(a) => a.iter().map(one).collect::<Vec<_>>().join(" "),
        _ => one(&v),
    }
}

fn term_matches(t: &Term, f: &Flow) -> bool {
    match t.field.as_str() {
        "host" => text_op(&t.op, f.hostname()),
        "path" => text_op(&t.op, f.path_only()),
        "query" => text_op(&t.op, f.query_string()),
        "url" => text_op(&t.op, &f.url()),
        "method" => text_op(&t.op, &f.method),
        "app" => text_op(&t.op, &f.app_label()),
        "platform" => text_op(&t.op, f.app_id.as_ref().map_or("", |i| i.platform.as_str())),
        "tag" => text_op(&t.op, &f.tag),
        "comment" => text_op(&t.op, &f.comment),
        "status" => f
            .status
            .is_some_and(|s| num_op(&t.op, s as f64) || text_op(&t.op, &s.to_string())),
        "size" => num_op(&t.op, f.res_size() as f64),
        "time" => f.duration_ms.is_some_and(|d| num_op(&t.op, d as f64)),
        "type" => {
            let ct = f
                .response
                .as_ref()
                .map(|r| r.content_type())
                .unwrap_or_default();
            text_op(&t.op, &ct) || text_op(&t.op, ct.rsplit(['/', '+']).next().unwrap_or(""))
        }
        "req" => text_op(&t.op, &f.request.text()),
        "res" | "body" => {
            f.response
                .as_ref()
                .is_some_and(|r| text_op(&t.op, &r.text()))
                || (t.field == "body" && text_op(&t.op, &f.request.text()))
        }
        "header" => {
            let all = |m: &crate::domain::flow::Message| {
                m.headers
                    .iter()
                    .any(|(k, v)| text_op(&t.op, &format!("{k}: {v}")))
            };
            all(&f.request) || f.response.as_ref().is_some_and(all)
        }
        "gql" => text_op(&t.op, &gql_op(f)),
        "is" => match &t.op {
            Op::Has(v) | Op::Eq(v) => match v.as_str() {
                "pinned" => f.pinned,
                "ws" | "websocket" => f.is_ws,
                "error" | "failed" => f.error.is_some() || f.status.is_some_and(|s| s >= 400),
                "mocked" | "modified" => !f.tag.is_empty(),
                "pending" => f.status.is_none() && f.error.is_none(),
                "https" => f.scheme == "https",
                "http" => f.scheme == "http",
                "json" => f
                    .response
                    .as_ref()
                    .is_some_and(|r| r.content_type().contains("json")),
                "commented" => !f.comment.is_empty(),
                "colored" => f.color.is_some(),
                _ => false,
            },
            _ => false,
        },
        _ => {
            text_op(&t.op, &f.url())
                || text_op(&t.op, &f.method)
                || f.status.is_some_and(|s| text_op(&t.op, &s.to_string()))
                || text_op(&t.op, &f.comment)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flow::Message;

    fn flow(method: &str, host: &str, path: &str, status: u16, body: &str) -> Flow {
        Flow {
            id: 1,
            method: method.into(),
            scheme: "https".into(),
            host: host.into(),
            path: path.into(),
            version: "HTTP/1.1".into(),
            started: std::time::SystemTime::now(),
            started_at: std::time::Instant::now(),
            request: Message::default(),
            status: Some(status),
            response: Some(Message {
                headers: vec![("content-type".into(), "application/json".into())],
                body: body.as_bytes().to_vec(),
            }),
            duration_ms: Some(120),
            tag: String::new(),
            app: "curl".into(),
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
    fn query_syntax() {
        let f = flow(
            "POST",
            "api.example.com:443",
            "/v1/users?id=3",
            404,
            r#"{"userId": 7}"#,
        );
        for (q, want) in [
            ("host:api.*", true),
            ("host:=example.com", false),
            ("status:>=400", true),
            ("status:<400", false),
            ("method:post -path:admin", true),
            ("path:admin", false),
            (r#"body:"userId""#, true),
            ("host:/^api[.]/", true),
            ("is:error", true),
            ("is:pinned", false),
            ("time:>100", true),
            ("app:curl type:json", true),
            ("users", true),
        ] {
            assert_eq!(Query::parse(q).matches(&f), want, "{q}");
        }
        assert!(Query::parse("status:>=x").error.is_some());
    }

    fn base() -> Flow {
        Flow {
            id: 1,
            method: "GET".into(),
            scheme: "http".into(),
            host: "a.com:8080".into(),
            path: "/p?x=1".into(),
            version: "HTTP/1.1".into(),
            started: std::time::SystemTime::now(),
            started_at: std::time::Instant::now(),
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

    #[test]
    fn empty_query_matches_everything() {
        assert!(Query::parse("").matches(&base()));
    }

    #[test]
    fn quoted_values_keep_spaces() {
        let mut f = base();
        f.comment = "hello world".into();
        assert!(Query::parse(r#"comment:"hello world""#).matches(&f));
        assert!(!Query::parse(r#"comment:"world hello""#).matches(&f));
    }

    #[test]
    fn host_strips_port_and_query_is_separate() {
        let f = base();
        assert!(Query::parse("host:=a.com").matches(&f));
        assert!(Query::parse("query:x=1").matches(&f));
        assert!(!Query::parse("path:x=1").matches(&f));
    }

    #[test]
    fn pending_and_scheme_flags() {
        let mut f = base();
        assert!(Query::parse("is:pending is:http -is:https").matches(&f));
        f.status = Some(200);
        assert!(!Query::parse("is:pending").matches(&f));
    }

    #[test]
    fn tag_marks_modified_and_errors_flag_failed() {
        let mut f = base();
        assert!(!Query::parse("is:modified").matches(&f));
        f.tag = "Mocked".into();
        assert!(Query::parse("is:mocked tag:mock").matches(&f));
        f.error = Some("x".into());
        assert!(Query::parse("is:failed").matches(&f));
    }

    #[test]
    fn gql_matches_operation_name_in_batches() {
        let mut f = base();
        f.request.body = br#"[{"operationName":"GetUser","query":"{a}"},{"operationName":"Other","query":"{b}"}]"#.to_vec();
        assert!(Query::parse("gql:other").matches(&f));
        assert!(!Query::parse("gql:missing").matches(&f));
    }

    #[test]
    fn bad_regex_reports_error() {
        assert!(Query::parse("host:/(/").error.is_some());
    }
}
