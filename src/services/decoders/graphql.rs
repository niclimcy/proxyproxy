use super::{BodyDecoder, ModeId, Rendered};
use crate::domain::flow::Message;
use crate::services::decode::{Ctx, gql_report};

pub struct GraphQl;

impl BodyDecoder for GraphQl {
    fn id(&self) -> ModeId {
        "graphql"
    }

    fn label(&self) -> &'static str {
        "GraphQL"
    }

    fn applies(&self, m: &Message, c: &Ctx, _: Option<&str>) -> bool {
        gql_report(m, c).is_some()
    }

    fn render(&self, m: &Message, c: &Ctx, _: Option<&str>) -> Rendered {
        Rendered::Text { text: gql_report(m, c).unwrap_or_default(), caption: None }
    }

    fn default_rank(&self) -> Option<u8> {
        Some(0)
    }
}
