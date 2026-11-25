mod decoded;
mod form;
mod graphql;
mod hex;
mod pretty;
mod raw;
mod schema;
mod tree;

use super::decode::Ctx;
use crate::domain::flow::Message;
use serde_json::Value;

pub type ModeId = &'static str;

pub const PRETTY: ModeId = "pretty";
pub const TREE: ModeId = "tree";

// What a decoder produces; views only know how to draw these.
pub enum Rendered {
    Text {
        text: String,
        caption: Option<String>,
    },
    Form(Vec<(String, String)>),
    Tree(Value),
    Hex,
    Sections(Vec<(String, String)>),
}

// One implementation per body view. Pure: no I/O, no panics.
pub trait BodyDecoder: Sync {
    fn id(&self) -> ModeId;
    fn label(&self) -> &'static str;
    fn applies(&self, m: &Message, c: &Ctx, forced: Option<&str>) -> bool;
    fn render(&self, m: &Message, c: &Ctx, forced: Option<&str>) -> Rendered;

    // lower wins when picking the initial view; None = never the default
    fn default_rank(&self) -> Option<u8> {
        None
    }

    fn type_picker(&self, _m: &Message) -> bool {
        false
    }
}

// Display order. A new decoder is one file plus one line here.
static ALL: &[&dyn BodyDecoder] = &[
    &pretty::Pretty,
    &tree::Tree,
    &form::Form,
    &graphql::GraphQl,
    &schema::Schema,
    &decoded::Decoded,
    &raw::Raw,
    &hex::Hex,
];

pub fn find(id: ModeId) -> Option<&'static dyn BodyDecoder> {
    ALL.iter().copied().find(|d| d.id() == id)
}

pub fn available(m: &Message, c: &Ctx, forced: Option<&str>) -> Vec<&'static dyn BodyDecoder> {
    ALL.iter()
        .copied()
        .filter(|d| d.applies(m, c, forced))
        .collect()
}

pub fn default_for(avail: &[&'static dyn BodyDecoder]) -> ModeId {
    avail
        .iter()
        .filter_map(|d| d.default_rank().map(|r| (r, d.id())))
        .min_by_key(|(r, _)| *r)
        .map_or(PRETTY, |(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn msg(ct: &str, body: &str) -> Message {
        Message {
            headers: vec![("content-type".into(), ct.into())],
            body: Bytes::copy_from_slice(body.as_bytes()),
        }
    }

    fn ids(m: &Message, c: &Ctx) -> Vec<ModeId> {
        available(m, c, None).iter().map(|d| d.id()).collect()
    }

    #[test]
    fn ids_are_unique_and_findable() {
        let mut seen = std::collections::HashSet::new();
        for d in ALL {
            assert!(seen.insert(d.id()), "duplicate {}", d.id());
            assert_eq!(find(d.id()).unwrap().label(), d.label());
        }
        assert!(find("nope").is_none());
    }

    #[test]
    fn plain_text_offers_pretty_raw_hex() {
        assert_eq!(
            ids(&msg("text/plain", "hi"), &Ctx::default()),
            vec!["pretty", "raw", "hex"]
        );
    }

    #[test]
    fn json_adds_tree_and_defaults_to_it() {
        let m = msg("application/json", r#"{"a":1}"#);
        let avail = available(&m, &Ctx::default(), None);
        assert_eq!(
            avail.iter().map(|d| d.id()).collect::<Vec<_>>(),
            vec!["pretty", "tree", "raw", "hex"]
        );
        assert_eq!(default_for(&avail), TREE);
    }

    #[test]
    fn form_body_adds_form_view() {
        assert!(
            ids(
                &msg("application/x-www-form-urlencoded", "a=1"),
                &Ctx::default()
            )
            .contains(&"form")
        );
    }

    #[test]
    fn graphql_beats_tree_as_default() {
        let m = msg("application/json", r#"{"query":"{ a }"}"#);
        let c = Ctx {
            method: "POST",
            path: "/graphql",
            response: false,
            gql_req: false,
        };
        let avail = available(&m, &c, None);
        assert!(avail.iter().any(|d| d.id() == "graphql"));
        assert_eq!(default_for(&avail), "graphql");
    }

    #[test]
    fn jwt_in_body_adds_decoded_view() {
        let m = msg("text/plain", "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig");
        assert!(ids(&m, &Ctx::default()).contains(&"decoded"));
    }

    #[test]
    fn empty_registry_falls_back_to_pretty() {
        assert_eq!(default_for(&[]), PRETTY);
    }

    #[test]
    fn every_available_decoder_renders_without_panicking() {
        let bodies = [
            msg("application/json", r#"{"a":[1,2]}"#),
            msg("application/grpc", "\0\0\0\0\x02\x08\x01"),
            msg("text/html", "<a><b/></a>"),
            msg("", ""),
        ];
        for m in &bodies {
            for d in available(m, &Ctx::default(), None) {
                let _ = d.render(m, &Ctx::default(), None);
            }
        }
    }
}
