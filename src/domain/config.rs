use super::keymap::{Binding, RowAction};
use super::rule::{Rule, RuleCollection};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub collections: Vec<RuleCollection>,
    /// Zero in configs from before it existed; `new_collection_id` also looks at the highest id in use.
    #[serde(default)]
    pub next_collection_id: u64,
    // configs from before onboarding existed belong to users who are already set up;
    // first runs have no config file and get `Default` (false)
    #[serde(default = "already_onboarded")]
    pub onboarding_completed: bool,
    #[serde(default)]
    pub onboarding_profile: OnboardingProfile,
    #[serde(default)]
    pub scope_only: bool,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// network-level: route only these hosts through the system proxy (via PAC)
    #[serde(default)]
    pub proxy_only: bool,
    #[serde(default)]
    pub proxy_only_hosts: Vec<String>,
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
    /// Changed row shortcuts; `None` turns one off. Actions not listed keep their default.
    #[serde(default)]
    pub shortcuts: Vec<(RowAction, Option<Binding>)>,
    /// Off until the user opts in: MCP can read every captured flow and edit rules.
    #[serde(default)]
    pub mcp_enabled: bool,
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

fn already_onboarded() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_saved_by_an_older_version_loads_with_defaults() {
        let c: Config = serde_json::from_str(
            r#"{"rules":[],"scope_only":true,"scope":["a.com"],"muted":["m.com"]}"#,
        )
        .unwrap();
        assert!(c.exclude.is_empty() && !c.proxy_only && c.proxy_only_hosts.is_empty());
        assert!(
            c.captured("https://a.com/x")
                && !c.captured("https://b.com/x")
                && !c.captured("https://m.com/x")
        );
        assert_eq!(
            c.binding(crate::domain::keymap::RowAction::Replay),
            Some(crate::domain::keymap::Binding {
                key: "R".into(),
                shift: false
            })
        );
    }

    #[test]
    fn existing_config_skips_onboarding_but_first_run_does_not() {
        let cfg: Config = serde_json::from_str(r#"{"rules":[]}"#).unwrap();
        assert!(cfg.onboarding_completed);
        assert!(!Config::default().onboarding_completed);
        assert_eq!(cfg.onboarding_profile, OnboardingProfile::Experienced);
    }

    #[test]
    fn mcp_is_off_by_default_and_for_configs_without_the_field() {
        assert!(!Config::default().mcp_enabled);
        let cfg: Config = serde_json::from_str(r#"{"rules":[]}"#).unwrap();
        assert!(!cfg.mcp_enabled);
    }
}
