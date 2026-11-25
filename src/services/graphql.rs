use crate::domain::flow::Message;
use serde_json::Value;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub kind: String,
    pub name: Option<String>,
    pub query: String,
    pub variables: Option<Value>,
    pub extensions: Option<Value>,
}

impl Op {
    pub fn summary(&self) -> String {
        match &self.name {
            Some(n) => format!("{} {n}", self.kind),
            None => self.kind.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Name(String),
    Str(String),
    Spread,
    P(char),
}

fn tokenize(s: &str) -> Vec<Tok> {
    let c: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    let spread_at = |i: usize| {
        c.get(i) == Some(&'.') && c.get(i + 1) == Some(&'.') && c.get(i + 2) == Some(&'.')
    };
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '#' {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '"' {
            let start = i;
            if c.get(i + 1) == Some(&'"') && c.get(i + 2) == Some(&'"') {
                i += 3;
                while i < c.len()
                    && !(c[i] == '"'
                        && c.get(i + 1) == Some(&'"')
                        && c.get(i + 2) == Some(&'"')
                        && c[i - 1] != '\\')
                {
                    i += 1;
                }
                i = (i + 3).min(c.len());
            } else {
                i += 1;
                while i < c.len() && c[i] != '"' {
                    i += if c[i] == '\\' { 2 } else { 1 };
                }
                i = (i + 1).min(c.len());
            }
            out.push(Tok::Str(c[start..i].iter().collect()));
        } else if spread_at(i) {
            out.push(Tok::Spread);
            i += 3;
        } else if "{}()[]:!=@$|&,".contains(ch) {
            out.push(Tok::P(ch));
            i += 1;
        } else {
            let start = i;
            while i < c.len()
                && !c[i].is_whitespace()
                && !"{}()[]:!=@$|&,\"#".contains(c[i])
                && !spread_at(i)
            {
                i += 1;
            }
            out.push(Tok::Name(c[start..i].iter().collect()));
        }
    }
    out
}

fn scan_ops(query: &str) -> Vec<(String, Option<String>)> {
    let toks = tokenize(query);
    let (mut depth, mut ops) = (0i32, vec![]);
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::P('{' | '(' | '[') => {
                if depth == 0 && *t == Tok::P('{') {
                    ops.push(("query".to_string(), None));
                }
                depth += 1;
            }
            Tok::P('}' | ')' | ']') => depth -= 1,
            Tok::Name(n)
                if depth == 0 && matches!(n.as_str(), "query" | "mutation" | "subscription") =>
            {
                let name = match toks.get(i + 1) {
                    Some(Tok::Name(x)) => Some(x.clone()),
                    _ => None,
                };
                ops.push((n.clone(), name));
            }
            _ => {}
        }
    }
    ops
}

fn looks_graphql(q: &str) -> bool {
    match tokenize(q).first() {
        Some(Tok::P('{')) => true,
        Some(Tok::Name(n)) => matches!(
            n.as_str(),
            "query" | "mutation" | "subscription" | "fragment"
        ),
        _ => false,
    }
}

fn make_op(
    query: &str,
    op_name: Option<&str>,
    variables: Option<Value>,
    extensions: Option<Value>,
) -> Op {
    let ops = scan_ops(query);
    let picked = op_name
        .and_then(|n| ops.iter().find(|(_, x)| x.as_deref() == Some(n)))
        .or(ops.first());
    let (kind, name) = match picked {
        Some((k, n)) => (k.clone(), n.clone().or(op_name.map(String::from))),
        None => ("persisted".to_string(), op_name.map(String::from)),
    };
    Op {
        kind,
        name,
        query: query.to_string(),
        variables,
        extensions,
    }
}

fn from_obj(o: &Value) -> Option<Op> {
    let o = o.as_object()?;
    let q = o.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let ext = o.get("extensions").filter(|e| !e.is_null()).cloned();
    let persisted = q.is_empty()
        && ext
            .as_ref()
            .is_some_and(|e| e.get("persistedQuery").is_some());
    if !persisted && !looks_graphql(q) {
        return None;
    }
    let vars = o.get("variables").filter(|v| !v.is_null()).cloned();
    Some(make_op(
        q,
        o.get("operationName").and_then(|n| n.as_str()),
        vars,
        ext,
    ))
}

pub fn parse_request(method: &str, path: &str, m: &Message) -> Option<Vec<Op>> {
    if method.eq_ignore_ascii_case("GET") {
        let params = crate::services::decode::query_params(path);
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        let json = |k: &str| {
            get(k)
                .and_then(|v| serde_json::from_str::<Value>(v).ok())
                .filter(|v| !v.is_null())
        };
        let mut o = serde_json::Map::new();
        o.insert("query".into(), get("query")?.into());
        if let Some(n) = get("operationName") {
            o.insert("operationName".into(), n.into());
        }
        for k in ["variables", "extensions"] {
            if let Some(v) = json(k) {
                o.insert(k.into(), v);
            }
        }
        return from_obj(&Value::Object(o)).map(|op| vec![op]);
    }
    if m.body.is_empty() {
        return None;
    }
    if m.content_type() == "application/graphql" {
        let q = m.text();
        return looks_graphql(&q).then(|| vec![make_op(&q, None, None, None)]);
    }
    match serde_json::from_slice::<Value>(&m.body).ok()? {
        v @ Value::Object(_) => from_obj(&v).map(|op| vec![op]),
        Value::Array(a) if !a.is_empty() => a.iter().map(from_obj).collect(),
        _ => None,
    }
}

pub fn pretty_query(q: &str) -> String {
    let toks = tokenize(q);
    let mut w = Writer {
        out: String::new(),
        indent: 0,
        stack: vec![],
    };
    let mut prev: Option<&Tok> = None;
    let mut prev2: Option<&Tok> = None;
    for t in &toks {
        w.token(t, prev, prev2);
        prev2 = prev;
        prev = Some(t);
    }
    w.out.trim_end().to_string()
}

struct Writer {
    out: String,
    indent: usize,
    stack: Vec<char>,
}

impl Writer {
    fn inline(&self) -> bool {
        self.stack.iter().any(|c| matches!(c, '(' | '['))
    }

    fn nl(&mut self) {
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        self.out.push_str(&"  ".repeat(self.indent));
    }

    fn raw(t: &Tok) -> String {
        match t {
            Tok::Name(s) | Tok::Str(s) => s.clone(),
            Tok::Spread => "...".into(),
            Tok::P(c) => c.to_string(),
        }
    }

    fn space(&mut self, t: &Tok, prev: Option<&Tok>) {
        let Some(p) = prev else { return };
        let none = matches!(t, Tok::P(':' | ')' | ']' | '!' | ','))
            || matches!(p, Tok::P('(' | '[' | '$' | '@'))
            || (*p == Tok::Spread && !matches!(t, Tok::Name(n) if n == "on"))
            || (matches!(t, Tok::P('(')) && matches!(p, Tok::Name(_)))
            || self.out.ends_with(' ')
            || self.out.ends_with('\n');
        if !none {
            self.out.push(' ');
        }
    }

    fn token(&mut self, t: &Tok, prev: Option<&Tok>, prev2: Option<&Tok>) {
        let inline = self.inline();
        match t {
            Tok::P('{') => {
                self.space(t, prev);
                self.out.push('{');
                self.stack.push('{');
                if !inline {
                    self.indent += 1;
                    self.nl();
                }
            }
            Tok::P('}') => {
                self.stack.pop();
                if inline {
                    self.out.push_str(" }");
                } else {
                    self.indent = self.indent.saturating_sub(1);
                    self.nl();
                    self.out.push('}');
                }
            }
            Tok::P(c @ ('(' | '[')) => {
                self.space(t, prev);
                self.out.push(*c);
                self.stack.push(*c);
            }
            Tok::P(c @ (')' | ']')) => {
                self.stack.pop();
                self.out.push(*c);
            }
            Tok::P(',') if !inline => {}
            _ => {
                if !inline && !self.stack.is_empty() {
                    self.block_item(t, prev, prev2);
                } else if !inline && matches!(prev, Some(Tok::P('}'))) {
                    self.nl();
                    self.out.push('\n');
                } else {
                    self.space(t, prev);
                }
                self.out.push_str(&Self::raw(t));
            }
        }
    }

    fn block_item(&mut self, t: &Tok, prev: Option<&Tok>, prev2: Option<&Tok>) {
        let starts = matches!(t, Tok::Name(_) | Tok::Spread);
        let after_on =
            matches!(prev, Some(Tok::Name(n)) if n == "on") && matches!(prev2, Some(Tok::Spread));
        let ended = matches!(prev, Some(Tok::Name(_) | Tok::P(')' | '}')));
        if matches!(prev, Some(Tok::P('{'))) {
            // '{' already opened a fresh line
        } else if matches!(prev, Some(Tok::P('}'))) || (starts && ended && !after_on) {
            self.nl();
        } else {
            self.space(t, prev);
        }
    }
}

pub fn request_report(ops: &[Op]) -> String {
    let mut s = String::new();
    for (i, op) in ops.iter().enumerate() {
        if i > 0 {
            s.push_str("\n\n");
        }
        if ops.len() > 1 {
            let _ = write!(s, "# [{}/{}] ", i + 1, ops.len());
        } else {
            s.push_str("# ");
        }
        s.push_str(&op.summary());
        s.push('\n');
        if !op.query.is_empty() {
            s.push_str(&pretty_query(&op.query));
            s.push('\n');
        }
        for (label, v) in [("variables", &op.variables), ("extensions", &op.extensions)] {
            if let Some(v) = v {
                let _ = writeln!(
                    s,
                    "\n# {label}\n{}",
                    serde_json::to_string_pretty(v).unwrap_or_default()
                );
            }
        }
    }
    s.trim_end().to_string()
}

fn one_response(v: &Value, s: &mut String) -> bool {
    let Some(o) = v.as_object() else { return false };
    if !o.contains_key("data") && !o.contains_key("errors") {
        return false;
    }
    match o.get("data") {
        Some(Value::Object(d)) => {
            let keys: Vec<&str> = d.keys().map(String::as_str).collect();
            let _ = writeln!(
                s,
                "# data: {}",
                if keys.is_empty() {
                    "{}".to_string()
                } else {
                    keys.join(", ")
                }
            );
        }
        Some(Value::Null) => s.push_str("# data: null\n"),
        Some(_) => s.push_str("# data\n"),
        None => {}
    }
    if let Some(Value::Array(errs)) = o.get("errors") {
        let _ = writeln!(s, "# errors ({})", errs.len());
        for e in errs {
            let msg = e
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("(no message)");
            let mut line = format!("#   - {msg}");
            if let Some(p) = e.get("path").and_then(|p| p.as_array()) {
                let p: Vec<String> = p
                    .iter()
                    .map(|x| x.as_str().map_or_else(|| x.to_string(), String::from))
                    .collect();
                let _ = write!(line, "  (path: {})", p.join("."));
            }
            if let Some(c) = e.pointer("/extensions/code").and_then(|c| c.as_str()) {
                let _ = write!(line, "  [{c}]");
            }
            s.push_str(&line);
            s.push('\n');
        }
    }
    if let Some(d) = o.get("data").filter(|d| !d.is_null()) {
        let _ = writeln!(
            s,
            "\n{}",
            serde_json::to_string_pretty(d).unwrap_or_default()
        );
    }
    true
}

pub fn response_report(m: &Message) -> Option<String> {
    let v: Value = serde_json::from_slice(&m.body).ok()?;
    let mut s = String::new();
    match &v {
        Value::Array(a) if !a.is_empty() => {
            for (i, r) in a.iter().enumerate() {
                let _ = writeln!(s, "# [{}/{}]", i + 1, a.len());
                if !one_response(r, &mut s) {
                    return None;
                }
                s.push('\n');
            }
        }
        _ => {
            if !one_response(&v, &mut s) {
                return None;
            }
        }
    }
    Some(s.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn msg(ct: &str, body: &str) -> Message {
        Message {
            headers: vec![("Content-Type".into(), ct.into())],
            body: Bytes::copy_from_slice(body.as_bytes()),
        }
    }

    #[test]
    fn single_post() {
        let m = msg(
            "application/json",
            r#"{"query":"query GetUser($id: ID!){user(id:$id){name ...F}}","operationName":"GetUser","variables":{"id":"1"}}"#,
        );
        let ops = parse_request("POST", "/graphql", &m).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            (ops[0].kind.as_str(), ops[0].name.as_deref()),
            ("query", Some("GetUser"))
        );
        assert_eq!(ops[0].variables.as_ref().unwrap()["id"], "1");
        let r = request_report(&ops);
        assert!(
            r.contains("user(id: $id) {\n    name\n    ...F\n  }"),
            "{r}"
        );
        assert!(r.contains("# variables"), "{r}");
    }

    #[test]
    fn batch() {
        let m = msg(
            "application/json",
            r#"[{"query":"mutation M { a }"},{"query":"{ b }"}]"#,
        );
        let ops = parse_request("POST", "/g", &m).unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].summary(), "mutation M");
        assert_eq!(ops[1].summary(), "query");
        assert!(request_report(&ops).contains("[2/2]"));
    }

    #[test]
    fn get_request() {
        let p = "/graphql?query=query%20Q%7Ba%7D&operationName=Q&variables=%7B%22x%22%3A1%7D";
        let ops = parse_request("GET", p, &Message::default()).unwrap();
        assert_eq!(ops[0].summary(), "query Q");
        assert_eq!(ops[0].variables.as_ref().unwrap()["x"], 1);
    }

    #[test]
    fn anonymous_and_raw_graphql() {
        let ops = parse_request("POST", "/", &msg("application/graphql", "{ me { id } }")).unwrap();
        assert_eq!((ops[0].kind.as_str(), ops[0].name.clone()), ("query", None));
        assert_eq!(pretty_query("{ me { id } }"), "{\n  me {\n    id\n  }\n}");
        assert_eq!(
            pretty_query(
                "query Q($a: Int = 1, $b: [Int!]!) { f(x: {a: [1, 2]}) @skip(if: true) { ... on T { z } } }"
            ),
            "query Q($a: Int = 1, $b: [Int!]!) {\n  f(x: { a: [1, 2] }) @skip(if: true) {\n    ... on T {\n      z\n    }\n  }\n}"
        );
    }

    #[test]
    fn errors_response() {
        let m = msg(
            "application/json",
            r#"{"data":null,"errors":[{"message":"boom","path":["user",0,"name"],"extensions":{"code":"BAD"}}]}"#,
        );
        let r = response_report(&m).unwrap();
        assert!(
            r.contains("data: null")
                && r.contains("errors (1)")
                && r.contains("boom")
                && r.contains("user.0.name")
                && r.contains("[BAD]"),
            "{r}"
        );
        let ok = msg("application/json", r#"{"data":{"a":1,"b":2}}"#);
        assert!(response_report(&ok).unwrap().contains("data: a, b"));
    }

    #[test]
    fn malformed_and_non_graphql() {
        assert!(parse_request("POST", "/", &msg("application/json", "{not json")).is_none());
        assert!(
            parse_request(
                "POST",
                "/",
                &msg("application/json", r#"{"query":"hello world"}"#)
            )
            .is_none()
        );
        assert!(
            parse_request(
                "POST",
                "/",
                &msg("application/json", r#"{"query":{"match":{}}}"#)
            )
            .is_none()
        );
        assert!(parse_request("POST", "/", &msg("application/json", "[]")).is_none());
        assert!(parse_request("GET", "/x", &Message::default()).is_none());
        assert!(response_report(&msg("application/json", r#"{"foo":1}"#)).is_none());
        assert!(response_report(&msg("application/json", "oops")).is_none());
        assert_eq!(
            pretty_query("{ unterminated \"str"),
            "{\n  unterminated \"str"
        );
    }
}
