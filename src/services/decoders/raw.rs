use super::{BodyDecoder, ModeId, Rendered};
use crate::domain::flow::Message;
use crate::services::decode::Ctx;

pub struct Raw;

impl BodyDecoder for Raw {
    fn id(&self) -> ModeId {
        "raw"
    }

    fn label(&self) -> &'static str {
        "Raw"
    }

    fn applies(&self, _: &Message, _: &Ctx, _: Option<&str>) -> bool {
        true
    }

    fn render(&self, m: &Message, _: &Ctx, _: Option<&str>) -> Rendered {
        Rendered::Text {
            text: m.text(),
            caption: None,
        }
    }
}
