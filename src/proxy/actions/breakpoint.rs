use super::RuleAction;

pub struct Breakpoint {
    pub request: bool,
    pub response: bool,
}

impl RuleAction for Breakpoint {
    fn tag(&self) -> &'static str {
        "Breakpoint"
    }

    fn breakpoint(&self) -> (bool, bool) {
        (self.request, self.response)
    }
}
