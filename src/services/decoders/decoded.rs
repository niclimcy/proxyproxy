use super::{BodyDecoder, ModeId, Rendered};
use crate::domain::flow::Message;
use crate::services::decode::{Ctx, decode_jwt, is_protobuf, jwts, protobuf};
use std::fmt::Write as _;

pub struct Decoded;

fn jwt_text(h: &serde_json::Value, p: &serde_json::Value) -> String {
    let mut s = format!(
        "{}\n{}",
        serde_json::to_string_pretty(h).unwrap_or_default(),
        serde_json::to_string_pretty(p).unwrap_or_default()
    );
    if let Some(exp) = p.get("exp").and_then(serde_json::Value::as_i64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
        let _ = if exp < now {
            write!(s, "\nexpired {} min ago", (now - exp) / 60)
        } else {
            write!(s, "\nexpires in {} min", (exp - now) / 60)
        };
    }
    s
}

impl BodyDecoder for Decoded {
    fn id(&self) -> ModeId {
        "decoded"
    }

    fn label(&self) -> &'static str {
        "Decoded"
    }

    fn applies(&self, m: &Message, _: &Ctx, _: Option<&str>) -> bool {
        is_protobuf(m) || !jwts(m).is_empty()
    }

    fn render(&self, m: &Message, _: &Ctx, _: Option<&str>) -> Rendered {
        let mut out = vec![];
        for t in jwts(m) {
            if let Some((h, p)) = decode_jwt(&t) {
                out.push(("JWT".to_string(), jwt_text(&h, &p)));
            }
        }
        if is_protobuf(m) {
            let data = if m.content_type().contains("grpc") && m.body.len() > 5 {
                &m.body[5..]
            } else {
                &m.body[..]
            };
            out.push((
                "Protobuf (schemaless)".to_string(),
                protobuf(data, 0).unwrap_or_else(|| "could not decode as protobuf".into()),
            ));
        }
        Rendered::Sections(out)
    }

    fn type_picker(&self, m: &Message) -> bool {
        is_protobuf(m) && crate::services::schemas::is_loaded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn jwt_section_shows_claims_and_expiry() {
        let m = Message {
            headers: vec![],
            body: Bytes::from_static(b"eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIiwiZXhwIjoxfQ.sig"),
        };
        let Rendered::Sections(s) = Decoded.render(&m, &Ctx::default(), None) else {
            panic!()
        };
        assert_eq!(s[0].0, "JWT");
        assert!(
            s[0].1.contains("\"sub\": \"1\"") && s[0].1.contains("expired"),
            "{}",
            s[0].1
        );
    }

    #[test]
    fn grpc_frame_header_is_skipped() {
        let m = Message {
            headers: vec![("content-type".into(), "application/grpc".into())],
            body: Bytes::from_static(&[0, 0, 0, 0, 2, 0x08, 0x01]),
        };
        let Rendered::Sections(s) = Decoded.render(&m, &Ctx::default(), None) else {
            panic!()
        };
        assert!(s[0].1.contains("1: 1"), "{}", s[0].1);
    }
}
