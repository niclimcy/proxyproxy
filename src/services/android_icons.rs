//! Package id to app label and icon for Android emulators and devices, through `adb`.
//! Blocking and slow (pulls the APK): call from a background thread, never the proxy path.
//! Everything degrades to `None` when adb is missing or no device has the package.
use crate::domain::app;
use crate::services::android_pkg::{self, Attr, Resources};
use crate::services::{app_icons, zip_read::Zip};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_APK: u64 = 400 << 20;
const MAX_ENTRY: u64 = 64 << 20;

/// Seam for tests: runs `adb <args>` and returns stdout on success.
pub trait Adb {
    fn run(&self, args: &[&str]) -> Option<Vec<u8>>;
}

pub struct SystemAdb(PathBuf);

impl SystemAdb {
    /// `ANDROID_HOME` / `ANDROID_SDK_ROOT` platform-tools first, else `adb` on `PATH`.
    pub fn locate() -> Self {
        let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
        let sdk = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|d| PathBuf::from(d).join("platform-tools").join(exe))
            .find(|p| p.is_file());
        Self(sdk.unwrap_or_else(|| PathBuf::from("adb")))
    }
}

impl Adb for SystemAdb {
    fn run(&self, args: &[&str]) -> Option<Vec<u8>> {
        use std::process::{Command, Stdio};
        let mut c = Command::new(&self.0);
        c.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // no console flash
        }
        let mut child = c.spawn().ok()?;
        // an offline device can hang adb indefinitely
        let limit = Duration::from_secs(if args.contains(&"pull") { 90 } else { 8 });
        let start = Instant::now();
        loop {
            match child.try_wait().ok()? {
                Some(st) => {
                    let mut out = vec![];
                    child.stdout.take()?.take(1 << 20).read_to_end(&mut out).ok()?;
                    return st.success().then_some(out);
                }
                None if start.elapsed() > limit => {
                    child.kill().ok();
                    child.wait().ok();
                    return None;
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct ApkInfo {
    pub label: Option<String>,
    /// PNG bytes at the APK's own size.
    pub icon: Option<Vec<u8>>,
}

/// Where an icon or label was found, for `resolve`.
#[derive(Debug, PartialEq)]
pub struct AndroidApp {
    pub label: String,
    pub icon_key: String,
}

fn serials(adb: &dyn Adb) -> Vec<String> {
    let out = adb.run(&["devices"]).unwrap_or_default();
    let mut v: Vec<String> = String::from_utf8_lossy(&out)
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            let (s, state) = (p.next()?, p.next()?);
            (state == "device").then(|| s.to_string())
        })
        .collect();
    // emulator traffic is the likelier source, so ask those first
    v.sort_by_key(|s| !s.starts_with("emulator-"));
    v
}

/// `(serial, remote base.apk path)` of the first device that has `pkg`.
fn locate(adb: &dyn Adb, pkg: &str) -> Option<(String, String)> {
    for serial in serials(adb) {
        let out = adb.run(&["-s", &serial, "shell", "pm", "path", pkg])?;
        let text = String::from_utf8_lossy(&out);
        let paths: Vec<&str> = text.lines().filter_map(|l| l.trim().strip_prefix("package:")).collect();
        let base = paths.iter().find(|p| p.ends_with("/base.apk")).or(paths.first());
        if let Some(p) = base {
            return Some((serial, (*p).to_string()));
        }
    }
    None
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

/// Label and icon for `pkg`, cached under `dir` by package and the APK's remote path
/// (which changes on every update). `None` when adb or the package is unavailable.
pub fn resolve(adb: &dyn Adb, pkg: &str, dir: &Path) -> Option<AndroidApp> {
    // the id is passed to a remote shell, so it must be a plain package name
    if !app::is_package_id(pkg) {
        return None;
    }
    let (serial, remote) = locate(adb, pkg)?;
    let key = app::icon_key(pkg, fnv(&remote));
    let (png, name) = (dir.join(format!("{key}.png")), dir.join(format!("{key}.name")));
    if let Ok(label) = std::fs::read_to_string(&name) {
        let have = png.exists();
        return Some(AndroidApp { label, icon_key: if have { key } else { String::new() } });
    }
    std::fs::create_dir_all(dir).ok()?;
    let apk = dir.join(format!("{key}.apk.tmp"));
    let pulled = adb.run(&["-s", &serial, "pull", &remote, apk.to_str()?]).is_some();
    let info = pulled
        .then(|| std::fs::File::open(&apk).ok())
        .flatten()
        .filter(|f| f.metadata().is_ok_and(|m| m.len() <= MAX_APK))
        .and_then(|f| from_apk(std::io::BufReader::new(f)));
    std::fs::remove_file(&apk).ok();
    let info = info?;
    let label = info.label.unwrap_or_else(|| pkg.to_string());
    let has_icon = info
        .icon
        .and_then(|b| app_icons::to_icon_png(&b))
        .is_some_and(|b| std::fs::write(&png, b).is_ok());
    std::fs::write(&name, &label).ok()?;
    Some(AndroidApp { label, icon_key: if has_icon { key } else { String::new() } })
}

pub fn from_apk<R: Read + Seek>(r: R) -> Option<ApkInfo> {
    let mut zip = Zip::open(r)?;
    let manifest = zip.find("AndroidManifest.xml").cloned().and_then(|e| zip.read(&e, MAX_ENTRY))?;
    let manifest = android_pkg::parse_manifest(&manifest)?;
    let table = zip.find("resources.arsc").cloned().and_then(|e| zip.read(&e, MAX_ENTRY));
    let res = table.as_deref().and_then(Resources::parse);

    let resolve = |a: &Attr| -> Vec<String> {
        match a {
            Attr::Text(t) => vec![t.clone()],
            Attr::Res(id) => res.as_ref().map(|r| r.values(*id).into_iter().map(|v| v.text).collect()).unwrap_or_default(),
        }
    };
    let label = manifest.label.as_ref().and_then(|a| resolve(a).into_iter().find(|s| !s.is_empty()));
    let mut icon_paths: Vec<String> =
        manifest.icon.as_ref().map(resolve).unwrap_or_default().into_iter().filter(|p| p.ends_with(".png")).collect();
    // adaptive icons point at XML; fall back to whatever launcher PNGs ship in the archive
    let names: Vec<(&str, u64)> = zip.entries.iter().map(|e| (e.name.as_str(), e.size)).collect();
    icon_paths.extend(launcher_pngs(&names).into_iter().map(String::from));
    let icon = icon_paths.iter().find_map(|p| {
        let e = zip.find(p)?.clone();
        zip.read(&e, MAX_ENTRY).filter(|b| b.starts_with(b"\x89PNG"))
    });
    Some(ApkInfo { label, icon })
}

fn dir_density(dir: &str) -> u32 {
    [("xxxhdpi", 640), ("xxhdpi", 480), ("xhdpi", 320), ("hdpi", 240), ("mdpi", 160), ("ldpi", 120)]
        .iter()
        .find(|(k, _)| dir.contains(k))
        .map_or(0, |(_, d)| *d)
}

/// `res/{mipmap,drawable}*/ic_launcher*.png`, best first: full icons before adaptive
/// foregrounds, square before round, then densest, then largest.
fn launcher_pngs<'a>(files: &[(&'a str, u64)]) -> Vec<&'a str> {
    let mut v: Vec<_> = files
        .iter()
        .filter_map(|(path, size)| {
            let rest = path.strip_prefix("res/")?;
            let (dir, file) = rest.split_once('/')?;
            let stem = file.strip_suffix(".png")?;
            let ok = (dir.starts_with("mipmap") || dir.starts_with("drawable")) && stem.starts_with("ic_launcher");
            let bad = ["background", "monochrome"].iter().any(|b| stem.contains(b));
            (ok && !bad).then(|| {
                let rank = (stem.contains("foreground"), stem.contains("round"));
                (rank, std::cmp::Reverse((dir_density(dir), *size)), *path)
            })
        })
        .collect();
    v.sort();
    v.into_iter().map(|(_, _, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::android_pkg::fixtures::{manifest, table};
    use crate::services::zip_read::build;
    use std::cell::RefCell;
    use std::io::Cursor;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut o = Cursor::new(vec![]);
        image::DynamicImage::new_rgba8(w, h).write_to(&mut o, image::ImageFormat::Png).unwrap();
        o.into_inner()
    }

    fn apk(icon_res: bool) -> Vec<u8> {
        let strings = ["manifest", "package", "versionCode", "application", "icon", "label", "com.foo.bar", "Foo App"];
        let icon_attr = if icon_res { (4, 1, 0x7f01_0000) } else { (4, 1, 0x7f01_0001) };
        let man = manifest(
            &strings,
            &[(0, vec![(1, 3, 6), (2, 0x10, 7)]), (3, vec![icon_attr, (5, 1, 0x7f01_0002)])],
        );
        let g = ["res/mipmap-mdpi-v4/ic_launcher.png", "res/mipmap-xxhdpi-v4/ic_launcher.png", "Foo Label", "res/mipmap-anydpi-v26/ic_launcher.xml"];
        let t = table(&g, &[(160, vec![(3, 0), (3, 3), (3, 2)]), (480, vec![(3, 1), (3, 3), (3, 2)])]);
        let (small, big, fg) = (png(4, 4), png(16, 16), png(9, 9));
        build(&[
            ("AndroidManifest.xml", &man, true),
            ("resources.arsc", &t, true),
            ("res/mipmap-mdpi-v4/ic_launcher.png", &small, false),
            ("res/mipmap-xxhdpi-v4/ic_launcher.png", &big, false),
            ("res/mipmap-xxxhdpi-v4/ic_launcher_foreground.png", &fg, false),
        ])
    }

    fn dims(b: &[u8]) -> (u32, u32) {
        let i = image::load_from_memory(b).unwrap();
        (i.width(), i.height())
    }

    #[test]
    fn apk_icon_resolves_through_resources_to_densest_png() {
        let info = from_apk(Cursor::new(apk(true))).unwrap();
        assert_eq!(info.label.as_deref(), Some("Foo Label"));
        assert_eq!(dims(&info.icon.unwrap()), (16, 16));
    }

    #[test]
    fn adaptive_xml_icon_falls_back_to_launcher_png_then_foreground() {
        let info = from_apk(Cursor::new(apk(false))).unwrap();
        // the xml reference is skipped; the full launcher png beats the adaptive foreground
        assert_eq!(dims(&info.icon.unwrap()), (16, 16));
        let only_fg = [("res/mipmap-hdpi/ic_launcher_foreground.png", 5u64), ("res/mipmap-hdpi/ic_launcher_background.png", 9)];
        assert_eq!(launcher_pngs(&only_fg), ["res/mipmap-hdpi/ic_launcher_foreground.png"]);
        let many = [
            ("res/mipmap-hdpi/ic_launcher_round.png", 9),
            ("res/mipmap-mdpi/ic_launcher.png", 1),
            ("res/drawable-xhdpi/ic_launcher.png", 1),
            ("res/raw/ic_launcher.png", 1),
        ];
        assert_eq!(launcher_pngs(&many)[0], "res/drawable-xhdpi/ic_launcher.png");
    }

    #[test]
    fn corrupt_apks_never_panic() {
        let z = apk(true);
        for n in (0..z.len()).step_by(3) {
            let _ = from_apk(Cursor::new(&z[..n]));
        }
        for i in (0..z.len()).step_by(5) {
            let mut bad = z.clone();
            bad[i] ^= 0xFF;
            let _ = from_apk(Cursor::new(bad));
        }
        assert!(from_apk(Cursor::new(build(&[("x", b"y", false)]))).is_none());
    }

    struct Fake {
        apk: Vec<u8>,
        pkg_path: &'static str,
        devices: &'static str,
        calls: RefCell<Vec<String>>,
    }

    impl Adb for Fake {
        fn run(&self, args: &[&str]) -> Option<Vec<u8>> {
            self.calls.borrow_mut().push(args.join(" "));
            match args {
                ["devices"] => Some(self.devices.as_bytes().to_vec()),
                [_, s, "shell", "pm", "path", _] if *s == "emulator-5554" && !self.pkg_path.is_empty() => {
                    Some(format!("package:{}\n", self.pkg_path).into_bytes())
                }
                [_, _, "shell", ..] => Some(vec![]),
                [_, _, "pull", _, local] => std::fs::write(local, &self.apk).ok().map(|_| vec![]),
                _ => None,
            }
        }
    }

    fn fake(pkg_path: &'static str, devices: &'static str) -> Fake {
        Fake { apk: apk(true), pkg_path, devices, calls: RefCell::new(vec![]) }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pxp-android-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn resolve_pulls_once_and_serves_label_and_icon_from_cache() {
        let dir = tmp("resolve");
        let adb = fake("/data/app/~~x/com.foo.bar-1/base.apk", "List of devices attached\nemulator-5554\tdevice\nR58M\toffline\n");
        let a = resolve(&adb, "com.foo.bar", &dir).unwrap();
        assert_eq!(a.label, "Foo Label");
        assert_eq!(dims(&std::fs::read(dir.join(format!("{}.png", a.icon_key))).unwrap()), (16, 16));
        let pulls = |f: &Fake| f.calls.borrow().iter().filter(|c| c.contains("pull")).count();
        assert_eq!(pulls(&adb), 1);
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
        assert_eq!(resolve(&adb, "com.foo.bar", &dir).unwrap(), a);
        assert_eq!(pulls(&adb), 1, "second lookup is cached");
        // an updated install has a new remote path, so it is pulled again
        let adb2 = fake("/data/app/~~y/com.foo.bar-2/base.apk", "List of devices attached\nemulator-5554\tdevice\n");
        assert_ne!(resolve(&adb2, "com.foo.bar", &dir).unwrap().icon_key, a.icon_key);
        assert_eq!(pulls(&adb2), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn degrades_silently_without_devices_package_or_valid_id() {
        let dir = tmp("degrade");
        assert!(resolve(&fake("/x/base.apk", "List of devices attached\n"), "com.foo.bar", &dir).is_none());
        assert!(resolve(&fake("", "List of devices attached\nemulator-5554\tdevice\n"), "com.foo.bar", &dir).is_none());
        let adb = fake("/x/base.apk", "List of devices attached\nemulator-5554\tdevice\n");
        assert!(resolve(&adb, "com.foo;rm -rf /", &dir).is_none());
        assert!(adb.calls.borrow().is_empty(), "invalid ids never reach adb");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unreadable_apk_gives_no_identity() {
        let dir = tmp("bad");
        let mut adb = fake("/x/base.apk", "List of devices attached\nemulator-5554\tdevice\n");
        adb.apk = vec![1, 2, 3];
        assert!(resolve(&adb, "com.foo.bar", &dir).is_none());
        std::fs::remove_dir_all(dir).ok();
    }
}
