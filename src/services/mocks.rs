use crate::domain::config::Config;
use crate::domain::rule::{Action, MapStorage, Rule};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

const NAME_MAX: usize = 64;
const DEFAULT_NAME: &str = "response.json";

/// Folder for the files proxyproxy keeps on behalf of Map Local rules.
pub fn dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        // a test build must never touch the real data folder; one folder per test process
        if cfg!(test) {
            std::env::temp_dir().join(format!("pxp-mocks-{}", std::process::id()))
        } else {
            crate::proxy::tls::dir().join("mocks")
        }
    })
}

/// The one spelling of a managed name that is ever used on disk: its last component. `None`
/// for names with no usable file name (`""`, `.`, `..`), so nothing can point outside the folder.
pub fn key(name: &str) -> Option<String> {
    Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

pub fn path(name: &str) -> Option<PathBuf> {
    key(name).map(|k| dir().join(k))
}

fn bad_name() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid stored file name")
}

/// Where a Map Local rule's bytes live.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Disk(PathBuf),
    Stored(String),
}

impl Source {
    pub fn read(&self) -> io::Result<Vec<u8>> {
        match self {
            Source::Disk(p) => std::fs::read(p),
            Source::Stored(n) => std::fs::read(path(n).ok_or_else(bad_name)?),
        }
    }

    pub fn write(&self, data: &[u8]) -> io::Result<()> {
        match self {
            Source::Disk(p) => std::fs::write(p, data),
            Source::Stored(n) => write(n, data),
        }
    }

    pub fn metadata(&self) -> Option<std::fs::Metadata> {
        match self {
            Source::Disk(p) => std::fs::metadata(p).ok(),
            Source::Stored(n) => std::fs::metadata(path(n)?).ok(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Source::Disk(p) => p.display().to_string(),
            Source::Stored(n) => display_name(n).to_string(),
        }
    }
}

/// What the user typed as a file name, reduced to something safe to keep on disk.
pub fn clean_name(typed: &str) -> String {
    let base = Path::new(typed.trim())
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let name: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(NAME_MAX)
        .collect();
    if name.trim_matches('.').is_empty() {
        DEFAULT_NAME.into()
    } else {
        name
    }
}

/// `<16 hex>-<name>`: unique per rule, and the extension survives so the content type is
/// guessed like for any file.
pub fn new_name(typed: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let clean = clean_name(typed);
    loop {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let low = u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or_default();
        let id = low ^ n.rotate_left(48) ^ (u64::from(std::process::id()) << 20);
        let name = format!("{id:016x}-{clean}");
        if !path(&name).is_some_and(|p| p.exists()) {
            return name;
        }
    }
}

/// The name a user would recognise: `new_name` without its unique prefix.
pub fn display_name(managed: &str) -> &str {
    managed.split_once('-').map_or(managed, |(_, rest)| rest)
}

/// Written to a uniquely named neighbour, flushed, then renamed: a crash or a second
/// instance never sees half a file.
pub fn write(name: &str, data: &[u8]) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let key = key(name).ok_or_else(bad_name)?;
    std::fs::create_dir_all(dir())?;
    let tmp = dir().join(format!(
        ".{key}.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(data)?;
        f.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&tmp, dir().join(&key)))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

pub fn delete(name: &str) {
    if let Some(p) = path(name) {
        let _ = std::fs::remove_file(p);
    }
}

pub fn names<'a>(rules: impl Iterator<Item = &'a Rule>) -> Vec<String> {
    rules
        .filter_map(|r| r.action.managed_file().map(str::to_string))
        .collect()
}

fn used_keys(cfg: &Config, keep: &[&str]) -> HashSet<String> {
    cfg.rules
        .iter()
        .filter_map(|r| r.action.managed_file())
        .chain(keep.iter().copied())
        .filter_map(key)
        .collect()
}

/// Deletes the candidates that no rule, and nothing in `keep` (the open form, a pending
/// switch), refers to. Callers pass their config guard, so the check and the delete can't
/// interleave with a rule edit; call it only after the config that dropped them is saved.
pub fn sweep(cfg: &Config, keep: &[&str], candidates: &[String]) {
    let used = used_keys(cfg, keep);
    for name in candidates {
        if key(name).is_some_and(|k| !used.contains(&k)) {
            delete(name);
        }
    }
}

/// Startup cleanup of files no rule owns (a crash or quit between steps) and of leftover
/// temp files. Anything newer than `min_age` is left alone: it may belong to a second instance.
pub fn sweep_orphans(folder: &Path, cfg: &Config, min_age: Duration) {
    let used = used_keys(cfg, &[]);
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let temp =
            name.starts_with('.') && Path::new(&name).extension().is_some_and(|e| e == "tmp");
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age >= min_age);
        if old && e.path().is_file() && (temp || !used.contains(&name)) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Shows the file in Finder / Explorer / the file manager.
pub fn reveal(name: &str) -> Result<(), String> {
    let p = path(name).ok_or("invalid stored file name")?;
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(&p);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("explorer");
        c.arg(format!("/select,{}", p.display()));
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(dir());
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open the folder: {e}"))
}

/// Fills `embedded` with each active stored copy, so exported rules are self-contained.
/// A rule on disk keeps nothing of its retained copy: that copy stays with this install.
pub fn embed(rules: &mut [Rule]) {
    for r in rules {
        if let Action::MapLocal {
            storage,
            managed,
            embedded,
            ..
        } = &mut r.action
        {
            if *storage == MapStorage::AppData && !managed.is_empty() {
                *embedded = Source::Stored(managed.clone())
                    .read()
                    .ok()
                    .map(|d| STANDARD.encode(d));
            } else {
                managed.clear();
            }
        }
    }
}

/// Gives an incoming Map Local rule (import, MCP) a managed file of its own, never one named by
/// the sender. `keep` is the file the rule being replaced already owns; it is reused unless
/// `embedded` carries new content.
pub fn adopt(action: &mut Action, keep: Option<&str>) -> Result<(), String> {
    let Action::MapLocal {
        path: hint,
        storage,
        managed,
        embedded,
        ..
    } = action
    else {
        return Ok(());
    };
    let data = embedded
        .take()
        .map(|b| STANDARD.decode(b.trim()))
        .transpose()
        .map_err(|e| format!("invalid embedded content: {e}"))?;
    let name = keep.map(str::to_string);
    *managed = if data.is_none() && *storage == MapStorage::Disk {
        name.unwrap_or_default()
    } else {
        // an exported rule's own name outranks `path`, which is empty for rules made in stored mode
        let file = if managed.is_empty() {
            hint.clone()
        } else {
            display_name(managed).to_string()
        };
        let name = name.unwrap_or_else(|| new_name(&file));
        if data.is_some() || !path(&name).is_some_and(|p| p.exists()) {
            write(&name, &data.unwrap_or_default())
                .map_err(|e| format!("cannot store file: {e}"))?;
        }
        name
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_local(storage: MapStorage, path: &str, managed: &str) -> Action {
        Action::MapLocal {
            path: path.into(),
            status: 200,
            storage,
            managed: managed.into(),
            embedded: None,
        }
    }

    fn rule(id: u64, action: Action) -> Rule {
        Rule {
            id,
            enabled: true,
            name: String::new(),
            pattern: "a.com".into(),
            method: String::new(),
            action,
            collection: None,
            hits: 0,
        }
    }

    fn config_with(actions: Vec<Action>) -> Config {
        Config {
            rules: actions
                .into_iter()
                .zip(1..)
                .map(|(a, i)| rule(i, a))
                .collect(),
            ..Config::default()
        }
    }

    fn stored(name: &str, data: &[u8]) -> String {
        let n = new_name(name);
        write(&n, data).unwrap();
        n
    }

    #[test]
    fn names_are_unique_and_keep_the_extension() {
        let (a, b) = (new_name("response.json"), new_name("response.json"));
        assert_ne!(a, b);
        assert!(a.ends_with("-response.json") && display_name(&a) == "response.json");
        assert_eq!(clean_name("../../etc/pass wd"), "pass_wd");
        assert_eq!(clean_name("  "), DEFAULT_NAME);
    }

    #[test]
    fn names_cannot_point_outside_the_folder() {
        assert_eq!(path("../../x.json"), Some(dir().join("x.json")));
        for bad in ["", ".", "..", "a/.."] {
            assert!(path(bad).is_none(), "{bad:?}");
            assert!(write(bad, b"x").is_err() && Source::Stored(bad.into()).read().is_err());
        }
    }

    #[test]
    fn write_replaces_atomically_and_leaves_no_temp_files() {
        let n = stored("a.bin", &[0, 159, 146, 150]);
        write(&n, b"two").unwrap();
        assert_eq!(Source::Stored(n.clone()).read().unwrap(), b"two");
        let temps = std::fs::read_dir(dir())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!(".{n}."))
            })
            .count();
        assert_eq!(temps, 0);
        delete(&n);
        assert!(Source::Stored(n).read().is_err());
    }

    #[test]
    fn sweep_keeps_files_a_rule_or_the_open_form_still_uses() {
        let (used, held, free) = (stored("u", b"1"), stored("h", b"2"), stored("f", b"3"));
        let cfg = config_with(vec![map_local(MapStorage::Disk, "/x", &used)]);
        sweep(&cfg, &[&held], &[used.clone(), held.clone(), free.clone()]);
        assert!(Source::Stored(used.clone()).read().is_ok());
        assert!(Source::Stored(held.clone()).read().is_ok());
        assert!(Source::Stored(free).read().is_err());
        delete(&used);
        delete(&held);
    }

    #[test]
    fn a_non_canonical_name_in_config_still_protects_its_file() {
        let n = stored("canon.json", b"1");
        let spelled = format!("./{n}");
        let cfg = config_with(vec![map_local(MapStorage::AppData, "", &spelled)]);
        sweep(&cfg, &[], std::slice::from_ref(&n));
        assert!(Source::Stored(n.clone()).read().is_ok());
        delete(&n);
    }

    #[test]
    fn startup_sweep_removes_old_orphans_and_temp_files_only() {
        // its own folder: the sweep must not eat files other tests are using
        let folder = std::env::temp_dir().join(format!("pxp-sweep-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for f in ["used.json", "orphan.json", ".x.1.1.tmp"] {
            std::fs::write(folder.join(f), "x").unwrap();
        }
        let cfg = config_with(vec![map_local(MapStorage::AppData, "", "used.json")]);
        sweep_orphans(&folder, &cfg, Duration::from_secs(3600));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 3);
        sweep_orphans(&folder, &cfg, Duration::ZERO);
        let left: Vec<_> = std::fs::read_dir(&folder).unwrap().flatten().collect();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].file_name(), "used.json");
        std::fs::remove_dir_all(folder).ok();
    }

    #[test]
    fn exported_content_comes_back_under_a_fresh_name() {
        let name = stored("r.json", b"{\"a\":1}");
        let mut rules = [rule(1, map_local(MapStorage::AppData, "", &name))];
        embed(&mut rules);
        let mut action = rules[0].action.clone();
        assert!(matches!(
            &action,
            Action::MapLocal {
                embedded: Some(_),
                ..
            }
        ));

        adopt(&mut action, None).unwrap();
        let fresh = action.managed_file().unwrap().to_string();
        assert_ne!(fresh, name);
        assert!(fresh.ends_with("-r.json"));
        assert_eq!(Source::Stored(fresh.clone()).read().unwrap(), b"{\"a\":1}");
        assert!(matches!(&action, Action::MapLocal { embedded: None, .. }));
        delete(&name);
        delete(&fresh);
    }

    #[test]
    fn export_leaves_a_disk_rules_retained_copy_out() {
        let name = stored("keep.json", b"1");
        let mut rules = [rule(1, map_local(MapStorage::Disk, "/x.json", &name))];
        embed(&mut rules);
        assert!(matches!(
            &rules[0].action,
            Action::MapLocal { embedded: None, managed, .. } if managed.is_empty()
        ));
        delete(&name);
    }

    #[test]
    fn adopt_ignores_a_name_chosen_by_the_sender() {
        let mut action = map_local(MapStorage::AppData, "x.json", "../../victim");
        adopt(&mut action, None).unwrap();
        let name = action.managed_file().unwrap().to_string();
        assert!(name.ends_with("-victim") && !name.contains(".."));
        delete(&name);

        let mut disk = map_local(MapStorage::Disk, "/x.json", "../nope");
        adopt(&mut disk, None).unwrap();
        assert_eq!(disk.managed_file(), None);
    }

    #[test]
    fn adopt_keeps_the_rules_own_file_unless_new_content_comes() {
        let name = stored("x.json", b"kept");
        let mut same = map_local(MapStorage::AppData, "x.json", "");
        adopt(&mut same, Some(&name)).unwrap();
        assert_eq!(same.managed_file(), Some(name.as_str()));
        assert_eq!(Source::Stored(name.clone()).read().unwrap(), b"kept");

        let mut newer = map_local(MapStorage::AppData, "x.json", "");
        if let Action::MapLocal { embedded, .. } = &mut newer {
            *embedded = Some(STANDARD.encode("new"));
        }
        adopt(&mut newer, Some(&name)).unwrap();
        assert_eq!(Source::Stored(name.clone()).read().unwrap(), b"new");
        delete(&name);
    }

    #[test]
    fn bad_embedded_content_is_refused() {
        let mut action = map_local(MapStorage::AppData, "x.json", "");
        if let Action::MapLocal { embedded, .. } = &mut action {
            *embedded = Some("***".into());
        }
        assert!(adopt(&mut action, None).is_err());
    }
}
