use super::{BodyDecoder, ModeId, Rendered};
use crate::domain::flow::Message;
use crate::services::decode::{Ctx, schema_decode};

pub struct Schema;

impl BodyDecoder for Schema {
    fn id(&self) -> ModeId {
        "schema"
    }

    fn label(&self) -> &'static str {
        "Schema"
    }

    fn applies(&self, m: &Message, c: &Ctx, forced: Option<&str>) -> bool {
        schema_decode(m, c, forced).is_some()
    }

    fn render(&self, m: &Message, c: &Ctx, forced: Option<&str>) -> Rendered {
        let d = schema_decode(m, c, forced);
        Rendered::Text { caption: Some(format!("decoded as {}", d.as_ref().map(|d| d.type_name.as_str()).unwrap_or("?"))), text: d.map(|d| d.text).unwrap_or_default() }
    }

    fn default_rank(&self) -> Option<u8> {
        Some(1)
    }

    fn type_picker(&self, _: &Message) -> bool {
        true
    }
}
