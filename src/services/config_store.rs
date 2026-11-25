use crate::domain::config::Config;
use std::path::{Path, PathBuf};

pub fn path() -> PathBuf {
    crate::proxy::tls::dir().join("config.json")
}

/// Returns the config and, when the file was unreadable, a note for the status line.
pub fn load(path: &Path) -> (Config, Option<String>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (Config::default(), None);
    };
    match serde_json::from_str(&text) {
        Ok(cfg) => (cfg, None),
        Err(e) => {
            // moved aside so the next save can't overwrite the user's only copy
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let bad = path.with_extension(format!("json.bad-{ts}"));
            let kept = match std::fs::rename(path, &bad) {
                Ok(()) => format!("kept as {}", bad.display()),
                Err(_) => "left in place".into(),
            };
            (
                Config::default(),
                Some(format!(
                    "Settings could not be read ({e}); started with defaults, old file {kept}"
                )),
            )
        }
    }
}

pub fn save(path: &Path, cfg: &Config) {
    let Ok(s) = serde_json::to_string_pretty(cfg) else {
        return;
    };
    // write then rename, so a crash mid-write never leaves a truncated config
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, s).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pxp-cfg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("config.json")
    }

    #[test]
    fn unreadable_config_is_moved_aside_not_overwritten() {
        let p = temp("bad");
        std::fs::write(&p, r#"{"rules":[{"action":{"FutureAction":{}}}]}"#).unwrap();
        let (cfg, note) = load(&p);
        assert!(cfg.rules.is_empty() && note.is_some());
        save(&p, &cfg);
        let backups: Vec<_> = std::fs::read_dir(p.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".bad-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(
            std::fs::read_to_string(backups[0].path())
                .unwrap()
                .contains("FutureAction")
        );
    }

    #[test]
    fn save_then_load_roundtrips_and_missing_file_is_silent() {
        let p = temp("ok");
        assert!(load(&p).1.is_none());
        let mut cfg = Config::default();
        cfg.muted.push("a.com".into());
        save(&p, &cfg);
        let (back, note) = load(&p);
        assert_eq!((back.muted, note), (vec!["a.com".to_string()], None));
    }
}
