use super::{BodyDecoder, ModeId, PRETTY, Rendered};
use crate::domain::flow::Message;
use crate::services::decode::{Ctx, pretty_text};

pub struct Pretty;

impl BodyDecoder for Pretty {
    fn id(&self) -> ModeId {
        PRETTY
    }

    fn label(&self) -> &'static str {
        "Pretty"
    }

    fn applies(&self, _: &Message, _: &Ctx, _: Option<&str>) -> bool {
        true
    }

    fn render(&self, m: &Message, _: &Ctx, _: Option<&str>) -> Rendered {
        Rendered::Text { text: pretty_text(m), caption: None }
    }

    fn default_rank(&self) -> Option<u8> {
        Some(99)
    }
}
