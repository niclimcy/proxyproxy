use super::rule::Rule;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default)]
    pub onboarding_profile: OnboardingProfile,
    #[serde(default)]
    pub scope_only: bool,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub muted: Vec<String>,
    #[serde(default)]
    pub bypass_tls: Vec<String>,
    #[serde(default)]
    pub saved: Vec<(String, String)>,
    #[serde(default)]
    pub hidden_cols: Vec<String>,
    #[serde(default)]
    pub dns_overrides: Vec<(String, String)>,
    #[serde(default)]
    pub upstream: Upstream,
    #[serde(default)]
    pub proto_descs: Vec<String>,
    #[serde(default)]
    pub keylog_path: String,
    #[serde(skip)]
    pub capture_paused: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum OnboardingProfile {
    Beginner,
    #[default]
    Experienced,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum UpstreamMode {
    #[default]
    Off,
    Manual,
    Socks,
    Pac,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Upstream {
    pub mode: UpstreamMode,
    pub url: String,
    pub pac_path: String,
}

pub type Shared = Arc<Mutex<Config>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserializes_without_onboarding_fields() {
        let cfg: Config = serde_json::from_str(r#"{"rules":[]}"#).unwrap();
        assert!(!cfg.onboarding_completed);
        assert_eq!(cfg.onboarding_profile, OnboardingProfile::Experienced);
    }

    #[test]
    fn config_roundtrips_onboarding_profile() {
        let cfg = Config {
            onboarding_completed: true,
            onboarding_profile: OnboardingProfile::Beginner,
            ..Config::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert!(restored.onboarding_completed);
        assert_eq!(restored.onboarding_profile, OnboardingProfile::Beginner);
    }
}
