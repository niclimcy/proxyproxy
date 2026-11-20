use super::{BodyDecoder, ModeId, Rendered, TREE};
use crate::domain::flow::Message;
use crate::services::decode::{Ctx, is_json};
use serde_json::Value;

pub struct Tree;

impl BodyDecoder for Tree {
    fn id(&self) -> ModeId {
        TREE
    }

    fn label(&self) -> &'static str {
        "Tree"
    }

    fn applies(&self, m: &Message, _: &Ctx, _: Option<&str>) -> bool {
        is_json(m)
    }

    fn render(&self, m: &Message, _: &Ctx, _: Option<&str>) -> Rendered {
        Rendered::Tree(serde_json::from_slice(&m.body).unwrap_or(Value::Null))
    }

    fn default_rank(&self) -> Option<u8> {
        Some(2)
    }
}
