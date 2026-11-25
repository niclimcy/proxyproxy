use super::config::Config;

/// Which traffic the system proxy sends to proxyproxy.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    All,
    Hosts(Vec<String>),
}

impl Config {
    /// `Hosts` only when the toggle is on and at least one usable entry remains.
    pub fn proxy_route(&self) -> Route {
        let hosts = normalize_hosts(&self.proxy_only_hosts);
        if self.proxy_only && !hosts.is_empty() {
            Route::Hosts(hosts)
        } else {
            Route::All
        }
    }
}

/// Reduces a scope-style entry (`https://api.a.com:8443/v2/*`) to a PAC-expressible host
/// pattern. Anything outside `[a-z0-9._*-]` is rejected so it can't reach the script.
pub fn host_of(entry: &str) -> Option<String> {
    let e = entry.trim().to_ascii_lowercase();
    let e = e.split_once("://").map_or(e.as_str(), |(_, rest)| rest);
    let host = e.split(['/', '?', '#']).next()?;
    let host = match host.rsplit_once(':') {
        Some((h, port)) if port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => host,
    };
    let ok = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'*'));
    ok.then(|| host.to_string())
}

pub fn normalize_hosts(entries: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for h in entries.iter().filter_map(|e| host_of(e)) {
        if !out.contains(&h) {
            out.push(h);
        }
    }
    out
}

/// Whether any entry carries a path that `host_of` drops.
pub fn has_path_entries(entries: &[String]) -> bool {
    entries.iter().any(|e| {
        let e = e.trim();
        let rest = e.split_once("://").map_or(e, |(_, r)| r);
        rest.split_once('/').is_some_and(|(_, p)| !p.is_empty())
    })
}

/// `file:` URL for a PAC path: `file:///C:/Users/a%20b/x.pac` on Windows, `file:///home/x.pac`
/// elsewhere. Takes a string so the Windows shapes are testable on any host.
pub fn file_url(path: &str) -> String {
    let p = path.replace('\\', "/");
    let p = p.strip_prefix("//?/").unwrap_or(&p);
    let enc: String = p
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    if let Some(unc) = enc.strip_prefix("//") {
        format!("file://{unc}")
    } else if enc.starts_with('/') {
        format!("file://{enc}")
    } else {
        format!("file:///{enc}")
    }
}

/// `hosts` must come from `normalize_hosts`. A bare host also covers its subdomains,
/// matching how scope entries behave.
pub fn pac_script(hosts: &[String], port: u16) -> String {
    let mut conds = vec![];
    for h in hosts {
        conds.push(
            if let Some(rest) = h.strip_prefix("*.").filter(|r| !r.contains('*')) {
                format!("dnsDomainIs(host, \".{rest}\")")
            } else if h.contains('*') {
                format!("shExpMatch(host, \"{h}\")")
            } else {
                format!("host == \"{h}\" || dnsDomainIs(host, \".{h}\")")
            },
        );
    }
    let cond = if conds.is_empty() {
        "false".to_string()
    } else {
        conds.join("\n      || ")
    };
    format!(
        "function FindProxyForURL(url, host) {{\n  host = host.toLowerCase();\n  if ({cond}) return \"PROXY 127.0.0.1:{port}\";\n  return \"DIRECT\";\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn reduces_urls_and_patterns_to_hosts() {
        assert_eq!(
            host_of("https://API.a.com:8443/v2/*").as_deref(),
            Some("api.a.com")
        );
        assert_eq!(host_of("a.com/v2/users/*").as_deref(), Some("a.com"));
        assert_eq!(host_of("*.a.com").as_deref(), Some("*.a.com"));
        assert_eq!(host_of("  "), None);
    }

    #[test]
    fn rejects_entries_that_could_inject_script() {
        for bad in ["a.com\"); evil(); (\"", "a\\.com", "a com", "a.com'"] {
            assert_eq!(host_of(bad), None, "{bad}");
        }
        let pac = pac_script(&normalize_hosts(&s(&["x\"||true||\"", "ok.com"])), 1);
        assert!(!pac.contains("true"));
        assert_eq!(pac.matches("return \"PROXY").count(), 1);
    }

    #[test]
    fn normalize_dedupes_and_keeps_order() {
        assert_eq!(
            normalize_hosts(&s(&["B.com/x", "a.com", "b.com", ""])),
            s(&["b.com", "a.com"])
        );
    }

    #[test]
    fn script_matches_host_subdomain_and_wildcards() {
        let pac = pac_script(&s(&["a.com", "*.b.com", "api-*.c.com"]), 9090);
        assert!(pac.contains("function FindProxyForURL(url, host)"));
        assert!(pac.contains("host == \"a.com\" || dnsDomainIs(host, \".a.com\")"));
        assert!(pac.contains("dnsDomainIs(host, \".b.com\")"));
        assert!(pac.contains("shExpMatch(host, \"api-*.c.com\")"));
        assert!(pac.contains("return \"PROXY 127.0.0.1:9090\""));
        assert!(pac.contains("return \"DIRECT\""));
    }

    #[test]
    fn empty_list_never_proxies() {
        assert!(pac_script(&[], 9090).contains("if (false)"));
    }

    #[test]
    fn route_needs_toggle_and_a_usable_host() {
        let mut c = Config {
            proxy_only_hosts: s(&["a.com/x", "bad host"]),
            ..Config::default()
        };
        assert_eq!(c.proxy_route(), Route::All);
        c.proxy_only = true;
        assert_eq!(c.proxy_route(), Route::Hosts(s(&["a.com"])));
        c.proxy_only_hosts = s(&["bad host"]);
        assert_eq!(c.proxy_route(), Route::All);
    }

    #[test]
    fn detects_path_entries() {
        assert!(has_path_entries(&s(&["a.com/v2/*"])));
        assert!(!has_path_entries(&s(&["a.com", "https://b.com/"])));
    }

    #[test]
    fn file_urls_per_platform() {
        assert_eq!(
            file_url("/Users/a b/Library/x.pac"),
            "file:///Users/a%20b/Library/x.pac"
        );
        assert_eq!(
            file_url(r"C:\Users\a b\AppData\Roaming\proxyproxy\x.pac"),
            "file:///C:/Users/a%20b/AppData/Roaming/proxyproxy/x.pac"
        );
        assert_eq!(file_url(r"\\?\C:\x.pac"), "file:///C:/x.pac");
        assert_eq!(file_url(r"\\srv\share\x.pac"), "file://srv/share/x.pac");
        assert_eq!(file_url("/h/é.pac"), "file:///h/%C3%A9.pac");
    }
}
