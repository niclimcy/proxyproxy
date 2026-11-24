//! Parsers for OS command output and file formats. They compile on every host so the
//! fixtures run anywhere, but each OS only calls its own, hence the allow.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// `lsof -Fpc` output: first process that isn't `own_pid`.
pub fn lsof_owner(text: &str, own_pid: u32) -> Option<(String, u32)> {
    let (mut pid, mut found) = (0u32, None);
    for l in text.lines() {
        if let Some(p) = l.strip_prefix('p') {
            pid = p.parse().unwrap_or(0);
        } else if let Some(c) = l.strip_prefix('c')
            && pid != own_pid
            && found.is_none()
        {
            found = Some((c.to_string(), pid));
        }
    }
    found
}

/// Socket inodes whose local port is `port`, from `/proc/net/tcp` or `tcp6`.
pub fn proc_net_inodes(text: &str, port: u16) -> Vec<u64> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let local = u16::from_str_radix(f.get(1)?.rsplit_once(':')?.1, 16).ok()?;
            let inode: u64 = f.get(9)?.parse().ok()?;
            (local == port && inode != 0).then_some(inode)
        })
        .collect()
}

/// `socket:[12345]`, the target of a `/proc/<pid>/fd` link.
pub fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
}

/// `netstat -ano -p TCP`: pid of the first row with that local port. The state column is
/// localized, so only the address and the last column are read.
pub fn netstat_pid(text: &str, port: u16, own_pid: u32) -> Option<u32> {
    text.lines().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 5 || !f[0].eq_ignore_ascii_case("TCP") {
            return None;
        }
        let local: u16 = f[1].rsplit_once(':')?.1.parse().ok()?;
        let pid: u32 = f.last()?.parse().ok()?;
        (local == port && pid != own_pid && pid != 0).then_some(pid)
    })
}

/// `tasklist /FO CSV /NH`: image name without `.exe`.
pub fn tasklist_name(csv: &str) -> Option<String> {
    let first = csv.lines().next()?.trim().strip_prefix('"')?;
    let image = first.split('"').next()?;
    let lower = image.to_ascii_lowercase();
    let stem = if lower.ends_with(".exe") { &image[..image.len() - 4] } else { image };
    (!stem.is_empty()).then(|| stem.to_string())
}

/// `gsettings get` of a string key: `'auto'` -> `auto`.
pub fn gsettings_str(text: &str) -> String {
    text.trim().trim_start_matches("@s ").trim_matches('\'').to_string()
}

/// `gsettings get` of an integer key: `9090` or `uint32 9090`.
pub fn gsettings_port(text: &str) -> Option<u16> {
    text.split_whitespace().last()?.parse().ok()
}

/// `reg query KEY /v NAME` output: the data of `name`.
pub fn reg_value(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (n, rest) = l.trim_start().split_once(char::is_whitespace)?;
        let (ty, data) = rest.trim_start().split_once(char::is_whitespace)?;
        (n.eq_ignore_ascii_case(name) && ty.starts_with("REG_")).then(|| data.trim().to_string())
    })
}

#[derive(Debug, PartialEq)]
pub struct DesktopEntry {
    pub name: String,
    pub icon: String,
    /// File name of the executable in `Exec=`.
    pub exec_bin: String,
}

pub fn parse_desktop(text: &str) -> Option<DesktopEntry> {
    let (mut name, mut icon, mut exec, mut in_entry) = (None, String::new(), None, false);
    for l in text.lines().map(str::trim) {
        if l.starts_with('[') {
            in_entry = l == "[Desktop Entry]";
        } else if in_entry && let Some((k, v)) = l.split_once('=') {
            match k.trim() {
                "Name" => name = Some(v.trim().to_string()),
                "Icon" => icon = v.trim().to_string(),
                "Exec" => exec = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    // `env FOO=1 app %U` launches `app`
    let bin = exec?
        .split_whitespace()
        .map(|t| t.trim_matches('"'))
        .find(|t| *t != "env" && !t.contains('='))?
        .rsplit('/')
        .next()?
        .to_string();
    Some(DesktopEntry { name: name?, icon, exec_bin: bin })
}

/// Where an `Icon=` value may live, best first. Absolute values are used as-is.
pub fn icon_candidates(data_dir: &Path, icon: &str) -> Vec<PathBuf> {
    if Path::new(icon).is_absolute() {
        return vec![PathBuf::from(icon)];
    }
    let mut v: Vec<PathBuf> = ["256x256", "128x128", "512x512", "64x64", "48x48", "32x32"]
        .iter()
        .map(|s| data_dir.join("icons/hicolor").join(s).join("apps").join(format!("{icon}.png")))
        .collect();
    v.push(data_dir.join("pixmaps").join(format!("{icon}.png")));
    v
}

/// Largest PNG-encoded image inside an `.icns` container. Older RLE-only icons are skipped.
pub fn icns_png(data: &[u8]) -> Option<&[u8]> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if data.get(..4)? != b"icns" {
        return None;
    }
    let (mut at, mut best): (usize, Option<&[u8]>) = (8, None);
    while let Some(head) = data.get(at..at + 8) {
        let len = u32::from_be_bytes(head[4..8].try_into().ok()?) as usize;
        if len < 8 {
            break;
        }
        let body = data.get(at + 8..at + len)?;
        if body.starts_with(PNG) && best.is_none_or(|b| body.len() > b.len()) {
            best = Some(body);
        }
        at += len;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsof_skips_own_pid() {
        let t = "p100\ncproxyproxy\np200\ncCurl\n";
        assert_eq!(lsof_owner(t, 100), Some(("Curl".into(), 200)));
        assert_eq!(lsof_owner("", 1), None);
    }

    #[test]
    fn proc_net_tcp_matches_local_port_only() {
        let t = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:C350 0100007F:4A92 01 00000000:00000000 00:00000000 00000000  1000        0 55555 1 0000 100 0 0 10 0\n\
   1: 0100007F:4A92 0100007F:C350 01 00000000:00000000 00:00000000 00000000  1000        0 66666 1 0000 100 0 0 10 0\n";
        assert_eq!(proc_net_inodes(t, 0xC350), vec![55555]);
        assert!(proc_net_inodes(t, 1).is_empty());
        assert_eq!(socket_inode("socket:[55555]"), Some(55555));
        assert_eq!(socket_inode("/dev/null"), None);
    }

    #[test]
    fn netstat_rows() {
        let t = "Active Connections\r\n\r\n  Proto  Local Address          Foreign Address        State           PID\r\n\
  TCP    127.0.0.1:50000        127.0.0.1:19090        ESTABLISHED     4321\r\n\
  TCP    127.0.0.1:19090        127.0.0.1:50000        ESTABLISHED     99\r\n\
  TCP    [::1]:50001            [::1]:19090            ESTABLISHED     777\r\n";
        assert_eq!(netstat_pid(t, 50000, 99), Some(4321));
        assert_eq!(netstat_pid(t, 50001, 99), Some(777));
        assert_eq!(netstat_pid(t, 19090, 99), None);
        assert_eq!(netstat_pid(t, 1, 99), None);
    }

    #[test]
    fn tasklist_csv() {
        assert_eq!(tasklist_name("\"chrome.exe\",\"4321\",\"Console\",\"1\",\"100 K\"\r\n"), Some("chrome".into()));
        assert_eq!(tasklist_name("INFO: No tasks are running which match the specified criteria."), None);
    }

    #[test]
    fn gsettings_values() {
        assert_eq!(gsettings_str("'auto'\n"), "auto");
        assert_eq!(gsettings_str("''"), "");
        assert_eq!(gsettings_str("'file:///home/u/pac.pac'"), "file:///home/u/pac.pac");
        assert_eq!(gsettings_port("9090\n"), Some(9090));
        assert_eq!(gsettings_port("uint32 9090"), Some(9090));
        assert_eq!(gsettings_port("x"), None);
    }

    #[test]
    fn reg_query_values() {
        let t = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\r\n    AutoConfigURL    REG_SZ    file:///C:/Users/a%20b/proxyproxy.pac\r\n    ProxyEnable    REG_DWORD    0x1\r\n";
        assert_eq!(reg_value(t, "AutoConfigURL").as_deref(), Some("file:///C:/Users/a%20b/proxyproxy.pac"));
        assert_eq!(reg_value(t, "ProxyEnable").as_deref(), Some("0x1"));
        assert_eq!(reg_value(t, "ProxyServer"), None);
    }

    #[test]
    fn desktop_entry() {
        let t = "[Desktop Entry]\nName=Firefox\nName[de]=Feuerfuchs\nExec=env MOZ=1 /usr/lib/firefox/firefox %u\nIcon=firefox\n\n[Desktop Action x]\nName=New\nExec=other\n";
        assert_eq!(
            parse_desktop(t),
            Some(DesktopEntry { name: "Firefox".into(), icon: "firefox".into(), exec_bin: "firefox".into() })
        );
        assert_eq!(parse_desktop("[Desktop Entry]\nName=X\n"), None);
    }

    #[test]
    fn icon_lookup_order() {
        let c = icon_candidates(Path::new("/usr/share"), "firefox");
        assert!(c[0].ends_with("hicolor/256x256/apps/firefox.png"));
        assert!(c.last().unwrap().ends_with("pixmaps/firefox.png"));
        let abs = if cfg!(windows) { "C:\\i.png" } else { "/opt/i.png" };
        assert_eq!(icon_candidates(Path::new("/x"), abs), vec![PathBuf::from(abs)]);
    }

    #[test]
    fn icns_picks_largest_png() {
        let png = |n: usize| [b"\x89PNG\r\n\x1a\n".as_slice(), &vec![7u8; n]].concat();
        let entry = |ty: &[u8; 4], body: &[u8]| {
            [ty.as_slice(), &(body.len() as u32 + 8).to_be_bytes(), body].concat()
        };
        let mut file = b"icns\0\0\0\0".to_vec();
        file.extend(entry(b"is32", &[1, 2, 3]));
        file.extend(entry(b"ic07", &png(4)));
        file.extend(entry(b"ic08", &png(40)));
        let got = icns_png(&file).unwrap();
        assert_eq!(got.len(), 48);
        assert_eq!(icns_png(b"nope"), None);
        assert_eq!(icns_png(&file[..20]), None);
    }
}
