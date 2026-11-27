use super::config::Config;
use super::rule::Rule;

impl Config {
    pub fn next_rule_id(&self) -> u64 {
        self.rules.iter().map(|r| r.id).max().unwrap_or(0) + 1
    }

    pub fn host_in(list: &[String], host: &str) -> bool {
        list.iter().any(|p| host_entry_matches(p, host))
    }

    /// Scope and exclude entries are hosts (suffix match) or, if they contain `/`, URL patterns.
    pub fn url_in(list: &[String], url: &str) -> bool {
        let host = url_host(url);
        list.iter().any(|e| {
            let e = e.trim();
            if is_url_pattern(e) {
                url_pattern_matches(e, url)
            } else {
                host_entry_matches(e, host)
            }
        })
    }

    /// Scope/exclude/mute verdict, ignoring the pause switch.
    pub fn in_record_scope(&self, url: &str) -> bool {
        if Self::host_in(&self.muted, url_host(url)) || Self::url_in(&self.exclude, url) {
            return false;
        }
        !self.scope_only || self.scope.is_empty() || Self::url_in(&self.scope, url)
    }

    // CONNECT never calls this: tunnels aren't recorded, only the requests inside them,
    // and those arrive with a full URL.
    pub fn captured(&self, url: &str) -> bool {
        !self.capture_paused && self.in_record_scope(url)
    }

    // evaluated per CONNECT, so edits only affect new connections
    pub fn should_decrypt(&self, host: &str) -> bool {
        !Self::host_in(&self.bypass_tls, host)
    }

    // `pick` runs under the config lock, so it lets callers take a cheap handle instead of cloning actions
    pub fn matching<T>(
        &mut self,
        method: &str,
        url: &str,
        mut pick: impl FnMut(&Rule) -> T,
    ) -> Vec<T> {
        let mut out = vec![];
        let off = self.disabled_collections();
        for r in self.rules.iter_mut().filter(|r| rule_active(&off, r)) {
            if rule_matches(r, method, url) {
                r.hits += 1;
                out.push(pick(r));
            }
        }
        out
    }
}

/// A rule applies when it is on and its collection (if it has one) is not in `off`.
/// An id that no longer exists isn't in `off`, so a dangling id never hides a rule.
pub fn rule_active(off: &[u64], r: &Rule) -> bool {
    r.enabled && r.collection.is_none_or(|c| !off.contains(&c))
}

fn host_entry_matches(entry: &str, host: &str) -> bool {
    let (p, h) = (entry.trim().to_lowercase(), host.to_lowercase());
    !p.is_empty()
        && if p.contains('*') {
            glob(p.as_bytes(), h.as_bytes())
        } else {
            h == p || h.ends_with(&format!(".{p}"))
        }
}

fn is_url_pattern(entry: &str) -> bool {
    entry.contains('/')
}

fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => authority,
    }
}

// no `*` => prefix match on a path boundary, so `a.com/v2` covers `a.com/v2/x` but not `a.com/v20`
fn url_pattern_matches(pattern: &str, url: &str) -> bool {
    let pattern = pattern.trim().to_lowercase();
    let wild = pattern.contains('*');
    let p = if pattern.contains("://") || pattern.starts_with('*') {
        pattern
    } else {
        format!("*://{pattern}")
    };
    let u = url.to_lowercase();
    // patterns rarely spell out the port or the query, so also try the URL without them
    let portless = u.replacen(&format!(":{}", port_of(&u)), "", 1);
    let bare = |u: &str| u.split('?').next().unwrap_or(u).to_string();
    let forms = [u.clone(), portless.clone(), bare(&u), bare(&portless)];
    forms.iter().any(|u| {
        let g = |p: &str| glob(p.as_bytes(), u.as_bytes());
        if wild {
            return g(&p);
        }
        let base = p.trim_end_matches('/');
        g(base) || g(&format!("{base}/*"))
    })
}

fn port_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    match authority.rsplit_once(':') {
        Some((_, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => p,
        _ => "\0",
    }
}

pub fn rule_matches(r: &Rule, method: &str, url: &str) -> bool {
    (r.method.trim().is_empty() || r.method.trim().eq_ignore_ascii_case(method))
        && url_matches(&r.pattern, url)
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
    let p = if p.contains("://") || p.starts_with('*') {
        p
    } else {
        format!("*://{p}")
    };
    glob(p.as_bytes(), u.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Shared;
    use crate::domain::rule::{Action, Rule};
    use std::sync::{Arc, Mutex};

    #[test]
    fn globs_and_hosts() {
        assert!(url_matches(
            "api.example.com/users*",
            "https://api.example.com/users/1"
        ));
        assert!(url_matches("example", "https://api.example.com/x"));
        assert!(!url_matches("https://a.com/*", "https://b.com/x"));
        assert!(url_matches("https://a?.com/*", "https://ab.com/x"));
        assert!(Config::host_in(&["example.com".into()], "api.example.com"));
        assert!(!Config::host_in(&["example.com".into()], "notexample.com"));
        let mut c = Config {
            scope_only: true,
            scope: vec!["a.com".into()],
            ..Default::default()
        };
        assert!(c.captured("https://a.com:443/") && !c.captured("https://b.com/"));
        c.muted.push("a.com".into());
        assert!(!c.captured("https://a.com/"));
    }

    #[test]
    fn mute_and_scope_only_affect_recording() {
        let mut c = Config {
            scope_only: true,
            scope: vec!["a.com".into()],
            muted: vec!["a.com".into()],
            ..Default::default()
        };
        c.rules.push(Rule {
            id: 1,
            enabled: true,
            name: String::new(),
            pattern: "b.com".into(),
            method: String::new(),
            action: Action::Block { status: 403 },
            collection: None,
            hits: 0,
        });
        assert!(!c.captured("https://b.com/"));
        // rules stay evaluable; the proxy just skips them for uncaptured hosts
        assert_eq!(c.matching("GET", "https://b.com/x", |r| r.id).len(), 1);
        c.capture_paused = true;
        assert!(!c.captured("https://a.com/"));
    }

    #[test]
    fn rule_edits_apply_immediately() {
        let shared: Shared = Arc::new(Mutex::new(Config::default()));
        assert!(
            shared
                .lock()
                .unwrap()
                .matching("GET", "https://a.com/", |r| r.id)
                .is_empty()
        );
        shared.lock().unwrap().rules.push(Rule {
            id: 1,
            enabled: true,
            name: String::new(),
            pattern: "a.com".into(),
            method: String::new(),
            action: Action::Block { status: 403 },
            collection: None,
            hits: 0,
        });
        assert_eq!(
            shared
                .lock()
                .unwrap()
                .matching("GET", "https://a.com/", |r| r.id)
                .len(),
            1
        );
        shared.lock().unwrap().rules[0].enabled = false;
        assert!(
            shared
                .lock()
                .unwrap()
                .matching("GET", "https://a.com/", |r| r.id)
                .is_empty()
        );
    }

    #[test]
    fn no_decrypt_is_checked_per_connection() {
        let mut c = Config::default();
        assert!(c.should_decrypt("a.com"));
        c.bypass_tls.push("a.com".into());
        assert!(!c.should_decrypt("api.a.com"));
        assert!(c.should_decrypt("b.com"));
    }

    fn rule(id: u64, pattern: &str, method: &str, enabled: bool) -> Rule {
        Rule {
            id,
            enabled,
            name: String::new(),
            pattern: pattern.into(),
            method: method.into(),
            action: Action::Block { status: 403 },
            collection: None,
            hits: 0,
        }
    }

    #[test]
    fn method_filter_is_case_insensitive_and_blank_matches_all() {
        assert!(rule_matches(
            &rule(1, "a.com", "get", true),
            "GET",
            "https://a.com/"
        ));
        assert!(!rule_matches(
            &rule(1, "a.com", "POST", true),
            "GET",
            "https://a.com/"
        ));
        assert!(rule_matches(
            &rule(1, "a.com", " ", true),
            "DELETE",
            "https://a.com/"
        ));
    }

    #[test]
    fn matching_counts_hits_and_skips_disabled() {
        let mut c = Config::default();
        c.rules.push(rule(1, "a.com", "", true));
        c.rules.push(rule(2, "a.com", "", false));
        assert_eq!(c.matching("GET", "https://a.com/", |r| r.id).len(), 1);
        assert_eq!((c.rules[0].hits, c.rules[1].hits), (1, 0));
    }

    #[test]
    fn empty_pattern_never_matches() {
        assert!(!url_matches("  ", "https://a.com/"));
    }

    #[test]
    fn host_in_supports_wildcards_and_ignores_blank_entries() {
        assert!(Config::host_in(
            &["*.example.com".into()],
            "api.example.com"
        ));
        assert!(!Config::host_in(&[String::new(), "  ".into()], "a.com"));
        assert!(Config::host_in(&["A.COM".into()], "b.a.com"));
    }

    #[test]
    fn scope_is_ignored_when_empty_or_disabled() {
        let c = Config {
            scope_only: true,
            ..Default::default()
        };
        assert!(c.captured("https://any.com/"));
        let c = Config {
            scope_only: false,
            scope: vec!["a.com".into()],
            ..Default::default()
        };
        assert!(c.captured("https://b.com/"));
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

    fn cfg(scope: &[&str], exclude: &[&str]) -> Config {
        Config {
            scope_only: true,
            scope: scope.iter().map(ToString::to_string).collect(),
            exclude: exclude.iter().map(ToString::to_string).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn path_scope_matches_full_url() {
        let c = cfg(
            &["api.example.com/v2/*", "https://*.example.com/graphql"],
            &[],
        );
        assert!(c.captured("https://api.example.com/v2/users?x=1"));
        assert!(c.captured("https://api.example.com:8443/v2/users"));
        assert!(!c.captured("https://api.example.com/v1/users"));
        assert!(c.captured("https://a.example.com/graphql"));
        assert!(c.captured("https://a.example.com/graphql?op=1"));
        assert!(!c.captured("http://a.example.com/graphql"));
        assert!(!c.captured("https://a.example.com/graphqlx"));
    }

    #[test]
    fn host_entries_still_match_any_path() {
        let c = cfg(&["example.com", "api.other.com/v2"], &[]);
        assert!(c.captured("https://www.example.com/anything"));
        assert!(c.captured("https://api.other.com/v2"));
        assert!(c.captured("https://api.other.com/v2/x"));
        assert!(!c.captured("https://api.other.com/v20"));
    }

    #[test]
    fn exclude_beats_scope_and_takes_patterns() {
        let c = cfg(
            &["example.com"],
            &["example.com/health*", "ads.example.com"],
        );
        assert!(c.captured("https://example.com/users"));
        assert!(!c.captured("https://example.com/healthz"));
        assert!(!c.captured("https://ads.example.com/x"));
        let c = Config {
            exclude: vec!["a.com/private".into()],
            ..Default::default()
        };
        assert!(c.captured("https://a.com/public") && !c.captured("https://a.com/private/1"));
    }

    #[test]
    fn exclude_applies_without_scope_only() {
        let mut c = cfg(&["a.com"], &["b.com"]);
        c.scope_only = false;
        assert!(!c.captured("https://b.com/") && c.captured("https://c.com/"));
    }
}
