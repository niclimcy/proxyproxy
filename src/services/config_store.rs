use crate::domain::config::Config;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::proxy::tls::dir().join("config.json")
}

pub fn load() -> Config {
    std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(cfg: &Config) {
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path(), s);
    }
}
