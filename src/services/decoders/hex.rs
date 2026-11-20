use super::{BodyDecoder, ModeId, Rendered};
use crate::domain::flow::Message;
use crate::services::decode::Ctx;

pub struct Hex;

impl BodyDecoder for Hex {
    fn id(&self) -> ModeId {
        "hex"
    }

    fn label(&self) -> &'static str {
        "Hex"
    }

    fn applies(&self, _: &Message, _: &Ctx, _: Option<&str>) -> bool {
        true
    }

    fn render(&self, _: &Message, _: &Ctx, _: Option<&str>) -> Rendered {
        Rendered::Hex
    }
}
