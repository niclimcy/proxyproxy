use super::{BodyDecoder, ModeId, Rendered};
use crate::domain::flow::Message;
use crate::services::decode::{Ctx, is_form, parse_form};

pub struct Form;

impl BodyDecoder for Form {
    fn id(&self) -> ModeId {
        "form"
    }

    fn label(&self) -> &'static str {
        "Form"
    }

    fn applies(&self, m: &Message, _: &Ctx, _: Option<&str>) -> bool {
        is_form(m)
    }

    fn render(&self, m: &Message, _: &Ctx, _: Option<&str>) -> Rendered {
        Rendered::Form(parse_form(&m.text()))
    }
}
