use super::RuleAction;
use std::time::Duration;

pub struct Delay {
    pub ms: u64,
    pub kbps: u64,
}

impl RuleAction for Delay {
    fn tag(&self) -> &'static str {
        "Throttled"
    }

    fn runs_on_response(&self) -> bool {
        true
    }

    fn request_delay(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.ms))
    }

    fn response_delay(&self, body_len: usize) -> Option<Duration> {
        // saturating: kbps comes from user input and the MCP API, and must never panic
        (self.kbps > 0).then(|| {
            Duration::from_millis((body_len as u64).saturating_mul(1000) / self.kbps.saturating_mul(1024))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_applies_to_requests_and_bandwidth_cap_to_responses() {
        let d = Delay { ms: 50, kbps: 1 };
        assert_eq!(d.request_delay(), Some(Duration::from_millis(50)));
        assert_eq!(d.response_delay(2048), Some(Duration::from_millis(2000)));
        assert_eq!(Delay { ms: 0, kbps: 0 }.response_delay(2048), None);
        assert_eq!(Delay { ms: 0, kbps: u64::MAX }.response_delay(2048), Some(Duration::ZERO));
    }
}
