use super::config::Config;
use super::rule::{Action, Rule};

impl Config {
    pub fn next_rule_id(&self) -> u64 {
        self.rules.iter().map(|r| r.id).max().unwrap_or(0) + 1
    }

    pub fn host_in(list: &[String], host: &str) -> bool {
        let h = host.to_lowercase();
        list.iter().any(|p| {
            let p = p.trim().to_lowercase();
            !p.is_empty() && if p.contains('*') { glob(p.as_bytes(), h.as_bytes()) } else { h == p || h.ends_with(&format!(".{p}")) }
        })
    }

    pub fn captured(&self, host: &str) -> bool {
        if self.capture_paused {
            return false;
        }
        let h = host.rsplit_once(':').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit())).map(|(h, _)| h).unwrap_or(host);
        if Self::host_in(&self.muted, h) {
            return false;
        }
        !self.scope_only || self.scope.is_empty() || Self::host_in(&self.scope, h)
    }

    // evaluated per CONNECT, so edits only affect new connections
    pub fn should_decrypt(&self, host: &str) -> bool {
        !Self::host_in(&self.bypass_tls, host)
    }

    pub fn matching(&mut self, method: &str, url: &str) -> Vec<Action> {
        let mut out = vec![];
        for r in self.rules.iter_mut().filter(|r| r.enabled) {
            if rule_matches(r, method, url) {
                r.hits += 1;
                out.push(r.action.clone());
            }
        }
        out
    }
}

pub fn rule_matches(r: &Rule, method: &str, url: &str) -> bool {
    (r.method.trim().is_empty() || r.method.trim().eq_ignore_ascii_case(method)) && url_matches(&r.pattern, url)
}

pub fn glob(p: &[u8], t: &[u8]) -> bool {
    let (mut pi, mut ti, mut star) = (0, 0, None);
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && (p[pi] == t[ti] || p[pi] == b'?') {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

// no wildcard => substring match; otherwise a glob over the whole URL
pub fn url_matches(pattern: &str, url: &str) -> bool {
    let (p, u) = (pattern.trim().to_lowercase(), url.to_lowercase());
    if p.is_empty() {
        return false;
    }
    if !p.contains(['*', '?']) {
        return u.contains(&p);
    }
    // scheme-less globs match from the host onward
    let p = if p.contains("://") || p.starts_with('*') { p } else { format!("*://{p}") };
    glob(p.as_bytes(), u.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rule::{Action, Rule};
    use crate::domain::config::Shared;
    use std::sync::{Arc, Mutex};

    #[test]
    fn globs_and_hosts() {
        assert!(url_matches("api.example.com/users*", "https://api.example.com/users/1"));
        assert!(url_matches("example", "https://api.example.com/x"));
        assert!(!url_matches("https://a.com/*", "https://b.com/x"));
        assert!(url_matches("https://a?.com/*", "https://ab.com/x"));
        assert!(Config::host_in(&["example.com".into()], "api.example.com"));
        assert!(!Config::host_in(&["example.com".into()], "notexample.com"));
        let mut c = Config { scope_only: true, scope: vec!["a.com".into()], ..Default::default() };
        assert!(c.captured("a.com:443") && !c.captured("b.com"));
        c.muted.push("a.com".into());
        assert!(!c.captured("a.com"));
    }

    #[test]
    fn mute_and_scope_only_affect_recording() {
        let mut c = Config { scope_only: true, scope: vec!["a.com".into()], muted: vec!["a.com".into()], ..Default::default() };
        c.rules.push(Rule { id: 1, enabled: true, name: String::new(), pattern: "b.com".into(), method: String::new(), action: Action::Block { status: 403 }, hits: 0 });
        assert!(!c.captured("b.com"));
        // rules stay evaluable; the proxy just skips them for uncaptured hosts
        assert_eq!(c.matching("GET", "https://b.com/x").len(), 1);
        c.capture_paused = true;
        assert!(!c.captured("a.com"));
    }

    #[test]
    fn rule_edits_apply_immediately() {
        let shared: Shared = Arc::new(Mutex::new(Config::default()));
        assert!(shared.lock().unwrap().matching("GET", "https://a.com/").is_empty());
        shared.lock().unwrap().rules.push(Rule { id: 1, enabled: true, name: String::new(), pattern: "a.com".into(), method: String::new(), action: Action::Block { status: 403 }, hits: 0 });
        assert_eq!(shared.lock().unwrap().matching("GET", "https://a.com/").len(), 1);
        shared.lock().unwrap().rules[0].enabled = false;
        assert!(shared.lock().unwrap().matching("GET", "https://a.com/").is_empty());
    }

    #[test]
    fn no_decrypt_is_checked_per_connection() {
        let mut c = Config::default();
        assert!(c.should_decrypt("a.com"));
        c.bypass_tls.push("a.com".into());
        assert!(!c.should_decrypt("api.a.com"));
        assert!(c.should_decrypt("b.com"));
    }
}

#[cfg(test)]
mod characterization {
    use super::*;

    fn rule(id: u64, pattern: &str, method: &str, enabled: bool) -> Rule {
        Rule { id, enabled, name: String::new(), pattern: pattern.into(), method: method.into(), action: Action::Block { status: 403 }, hits: 0 }
    }

    #[test]
    fn method_filter_is_case_insensitive_and_blank_matches_all() {
        assert!(rule_matches(&rule(1, "a.com", "get", true), "GET", "https://a.com/"));
        assert!(!rule_matches(&rule(1, "a.com", "POST", true), "GET", "https://a.com/"));
        assert!(rule_matches(&rule(1, "a.com", " ", true), "DELETE", "https://a.com/"));
    }

    #[test]
    fn matching_counts_hits_and_skips_disabled() {
        let mut c = Config::default();
        c.rules.push(rule(1, "a.com", "", true));
        c.rules.push(rule(2, "a.com", "", false));
        assert_eq!(c.matching("GET", "https://a.com/").len(), 1);
        assert_eq!((c.rules[0].hits, c.rules[1].hits), (1, 0));
    }

    #[test]
    fn empty_pattern_never_matches() {
        assert!(!url_matches("  ", "https://a.com/"));
    }

    #[test]
    fn host_in_supports_wildcards_and_ignores_blank_entries() {
        assert!(Config::host_in(&["*.example.com".into()], "api.example.com"));
        assert!(!Config::host_in(&["".into(), "  ".into()], "a.com"));
        assert!(Config::host_in(&["A.COM".into()], "b.a.com"));
    }

    #[test]
    fn scope_is_ignored_when_empty_or_disabled() {
        let c = Config { scope_only: true, ..Default::default() };
        assert!(c.captured("any.com"));
        let c = Config { scope_only: false, scope: vec!["a.com".into()], ..Default::default() };
        assert!(c.captured("b.com"));
    }

    #[test]
    fn next_rule_id_is_max_plus_one() {
        let mut c = Config::default();
        assert_eq!(c.next_rule_id(), 1);
        c.rules.push(rule(7, "a", "", true));
        assert_eq!(c.next_rule_id(), 8);
    }

    #[test]
    fn config_json_roundtrip_drops_runtime_fields() {
        let mut c = Config::default();
        c.rules.push(rule(1, "a", "", true));
        c.rules[0].hits = 5;
        c.capture_paused = true;
        let back: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!((back.rules[0].hits, back.capture_paused), (0, false));
    }
}
