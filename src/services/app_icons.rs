use crate::domain::app::{self, AppIdentity, BundleInfo};
use crate::domain::repaint::Repaint;
use crate::services::{android_icons, car, client_proc, os_parse};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

const ICON_PX: u32 = 64;

pub fn icons_dir() -> PathBuf {
    crate::proxy::tls::dir().join("icons")
}

#[cfg(target_os = "macos")]
pub fn read_info(bundle: &Path) -> Option<BundleInfo> {
    // macOS keeps it under Contents/, iOS bundles at the root
    let plist = [bundle.join("Contents/Info.plist"), bundle.join("Info.plist")]
        .into_iter()
        .find(|p| p.exists())?;
    let out = Command::new("plutil")
        .args(["-convert", "json", "-o", "-", "--"])
        .arg(&plist)
        .output()
        .ok()?;
    let v = serde_json::from_slice(&out.status.success().then_some(out.stdout)?).ok()?;
    Some(app::parse_info(&v, bundle))
}

// `.app` bundles only exist on macOS and in simulator paths there
#[cfg(not(target_os = "macos"))]
pub fn read_info(_: &Path) -> Option<BundleInfo> {
    None
}

/// Re-encodes as a PNG no larger than `ICON_PX`, so the cache stays small.
pub(crate) fn to_icon_png(png: &[u8]) -> Option<Vec<u8>> {
    let mut img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?;
    if img.width().max(img.height()) > ICON_PX {
        img = img.thumbnail(ICON_PX, ICON_PX);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// Largest icon in `Assets.car`, trying the plist's icon set names, then `AppIcon`.
fn catalog_icon(bundle: &Path, info: &BundleInfo) -> Option<Vec<u8>> {
    const MAX_CAR: u64 = 128 << 20;
    let path = [bundle.join("Assets.car"), bundle.join("Contents/Resources/Assets.car")]
        .into_iter()
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.len() <= MAX_CAR))?;
    let mut names: Vec<&str> = info.icon_files.iter().map(String::as_str).collect();
    names.push("AppIcon");
    to_icon_png(&car::icon_png(&std::fs::read(path).ok()?, &names)?)
}

fn extract_icon(bundle: &Path, info: &BundleInfo, dest: &Path) -> bool {
    let res = bundle.join("Contents/Resources");
    let src = info.icon_files.iter().find_map(|n| {
        let name = if n.contains('.') { n.clone() } else { format!("{n}.icns") };
        Some(res.join(name)).filter(|p| p.exists())
    });
    let tmp = dest.with_extension("tmp");
    let ok = if let Some(icns) = src {
        std::fs::read(icns)
            .ok()
            .and_then(|d| to_icon_png(os_parse::icns_png(&d)?))
            .is_some_and(|png| std::fs::write(&tmp, png).is_ok())
    } else {
        // the asset catalog first (most iOS apps), then loose AppIcon*.png files
        if catalog_icon(bundle, info).is_some_and(|png| std::fs::write(&tmp, png).is_ok()) {
            return std::fs::rename(&tmp, dest).is_ok();
        }
        let files: Vec<(String, u64)> = std::fs::read_dir(bundle)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.metadata().ok()?.len())))
            .collect();
        app::largest_icon(&files).is_some_and(|n| std::fs::copy(bundle.join(n), &tmp).is_ok())
    };
    ok && std::fs::rename(&tmp, dest).is_ok()
}

/// Identities are looked up once per key; failures are cached too.
fn cached(key: String, make: impl FnOnce() -> Option<AppIdentity>) -> Option<AppIdentity> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<AppIdentity>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok()?.get(&key) {
        return hit.clone();
    }
    let identity = make();
    cache.lock().ok()?.insert(key, identity.clone());
    identity
}

fn mtime_secs(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

fn bundle_identity(bundle: &Path) -> Option<AppIdentity> {
    let mtime = mtime_secs(bundle);
    cached(format!("{}|{mtime}", bundle.display()), || {
        read_info(bundle).map(|info| {
            let path = bundle.to_string_lossy();
            let id = if info.bundle_id.is_empty() { &*path } else { &info.bundle_id };
            let key = app::icon_key(id, mtime);
            let dir = icons_dir();
            let dest = dir.join(format!("{key}.png"));
            let have = dest.exists() || (std::fs::create_dir_all(&dir).is_ok() && extract_icon(bundle, &info, &dest));
            AppIdentity {
                name: info.name,
                bundle_id: info.bundle_id,
                icon_key: if have { key } else { String::new() },
                ..Default::default()
            }
        })
    })
}

/// The `.desktop` entry that launches `exe`, with its icon from the hicolor theme.
#[cfg(target_os = "linux")]
fn desktop_identity(exe: &Path) -> Option<AppIdentity> {
    let bin = exe.file_name()?.to_str()?;
    let mut dirs: Vec<PathBuf> = vec![dirs::data_dir()?];
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let sys = if sys.is_empty() { "/usr/local/share:/usr/share".into() } else { sys };
    dirs.extend(std::env::split_paths(&sys));
    for data in &dirs {
        let Ok(entries) = std::fs::read_dir(data.join("applications")) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            let Some(d) = std::fs::read_to_string(&path).ok().and_then(|t| os_parse::parse_desktop(&t))
            else {
                continue;
            };
            if d.exec_bin != bin {
                continue;
            }
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            let key = app::icon_key(&stem, mtime_secs(exe));
            let dest = icons_dir().join(format!("{key}.png"));
            let have = dest.exists()
                || std::fs::create_dir_all(icons_dir()).is_ok()
                    && dirs
                        .iter()
                        .flat_map(|dd| os_parse::icon_candidates(dd, &d.icon))
                        .filter_map(|p| std::fs::read(p).ok())
                        .find_map(|b| to_icon_png(&b))
                        .is_some_and(|png| std::fs::write(&dest, png).is_ok());
            return Some(AppIdentity {
                name: d.name,
                bundle_id: stem,
                icon_key: if have { key } else { String::new() },
                ..Default::default()
            });
        }
    }
    None
}

/// Where no `.app` bundle exists: a desktop entry on Linux, else the file name alone.
#[cfg(not(target_os = "macos"))]
fn plain_identity(exe: &Path) -> Option<AppIdentity> {
    cached(format!("exe|{}", exe.display()), || {
        #[cfg(target_os = "linux")]
        if let Some(id) = desktop_identity(exe) {
            return Some(id);
        }
        let name = exe.file_stem()?.to_string_lossy().into_owned();
        Some(AppIdentity { name, ..Default::default() })
    })
}

// a bare executable such as curl keeps its process name
#[cfg(target_os = "macos")]
fn plain_identity(_: &Path) -> Option<AppIdentity> {
    None
}

/// Blocking: reads files and may shell out. Call from a background thread, never on the proxy path.
pub fn resolve(pid: u32, process: &str, package_hint: Option<String>) -> Option<AppIdentity> {
    let exe = client_proc::exe_path(pid);
    let exe_str = exe.as_deref().map_or(process.to_string(), |p| p.to_string_lossy().into_owned());
    if app::is_android_emulator(&exe_str) || app::is_adb_bridge(&exe_str) {
        let usb = app::is_adb_bridge(&exe_str);
        let mut id = android_identity(package_hint, usb);
        id.platform = (if usb { app::PLATFORM_ANDROID_DEVICE } else { app::PLATFORM_ANDROID_EMU }).into();
        return Some(id);
    }
    let exe = exe?;
    let sim = app::is_simulator_path(&exe);
    let found = match app::app_bundle(&exe) {
        Some(bundle) => bundle_identity(&bundle),
        // simulator system processes (launchd_sim, daemons) have no app bundle of their own
        None if sim => Some(AppIdentity {
            name: app::PLATFORM_IOS_SIM.into(),
            safari_icon_key: app::simulator_safari(&exe)
                .and_then(|b| bundle_identity(&b))
                .map(|i| i.icon_key)
                .unwrap_or_default(),
            ..Default::default()
        }),
        None => plain_identity(&exe),
    };
    // set after the cached lookup so the cache stays platform-neutral
    found.map(|mut id| {
        if sim {
            id.platform = app::PLATFORM_IOS_SIM.into();
        }
        id
    })
}

/// Package id to label and icon via adb; without adb or the package, the id itself is the name.
fn android_identity(package: Option<String>, usb: bool) -> AppIdentity {
    let Some(pkg) = package else {
        let name = if usb { "Android Device" } else { "Android Emulator" };
        return AppIdentity { name: name.into(), ..Default::default() };
    };
    let found = cached(format!("android|{pkg}"), || {
        let app = android_icons::resolve(&android_icons::SystemAdb::locate(), &pkg, &icons_dir())?;
        Some(AppIdentity { name: app.label, bundle_id: pkg.clone(), icon_key: app.icon_key, ..Default::default() })
    });
    found.unwrap_or(AppIdentity { name: pkg.clone(), bundle_id: pkg, ..Default::default() })
}

enum Slot {
    Loading,
    Ready(Option<Arc<Vec<u8>>>),
}

/// PNG bytes by icon key; memory first, disk read on a worker thread.
#[derive(Clone)]
pub struct IconStore {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    dir: PathBuf,
    repaint: Arc<dyn Repaint>,
}

impl IconStore {
    pub fn new(dir: PathBuf, repaint: Arc<dyn Repaint>) -> Self {
        Self { slots: Default::default(), dir, repaint }
    }

    pub fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        if key.is_empty() {
            return None;
        }
        let mut slots = self.slots.lock().ok()?;
        match slots.get(key) {
            Some(Slot::Ready(b)) => return b.clone(),
            Some(Slot::Loading) => return None,
            None => {}
        }
        slots.insert(key.to_string(), Slot::Loading);
        drop(slots);
        let (this, key) = (self.clone(), key.to_string());
        std::thread::spawn(move || {
            let bytes = std::fs::read(this.dir.join(format!("{key}.png"))).ok().map(Arc::new);
            if let Ok(mut s) = this.slots.lock() {
                s.insert(key, Slot::Ready(bytes));
            }
            this.repaint.request();
        });
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct Quiet;
    impl Repaint for Quiet {
        fn request(&self) {}
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pxp-icons-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn store_loads_from_disk_off_thread_and_caches() {
        let dir = tmp("store");
        std::fs::write(dir.join("k-1.png"), b"png").unwrap();
        let s = IconStore::new(dir.clone(), Arc::new(Quiet));
        assert!(s.get("").is_none());
        let t = Instant::now();
        let got = loop {
            if let Some(b) = s.get("k-1") {
                break b;
            }
            assert!(t.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(&**got, b"png");
        assert!(s.get("missing").is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ios_bundle_icon_is_largest_loose_png() {
        let dir = tmp("ios");
        let bundle = dir.join("Demo.app");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("AppIcon60x60@2x.png"), b"small").unwrap();
        std::fs::write(bundle.join("AppIcon76x76@3x.png"), b"the largest").unwrap();
        let dest = dir.join("out.png");
        assert!(extract_icon(&bundle, &BundleInfo::default(), &dest));
        assert_eq!(std::fs::read(&dest).unwrap(), b"the largest");
        let empty = dir.join("Empty.app");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(!extract_icon(&empty, &BundleInfo::default(), &dir.join("none.png")));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ios_bundle_prefers_asset_catalog_over_loose_png() {
        use crate::services::car::fixtures::{car, csi};
        let dir = tmp("car");
        let bundle = dir.join("Demo.app");
        std::fs::create_dir_all(&bundle).unwrap();
        let mut png = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgba8(120, 120).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let catalog = car("AppIcon-Custom", &[(csi(120, 120, png.get_ref()), false)]);
        std::fs::write(bundle.join("Assets.car"), catalog).unwrap();
        std::fs::write(bundle.join("AppIcon60x60@2x.png"), b"loose").unwrap();
        let info = BundleInfo { icon_files: vec!["AppIcon-Custom".into()], ..Default::default() };
        let dest = dir.join("out.png");
        assert!(extract_icon(&bundle, &info, &dest));
        assert_eq!(image::open(&dest).unwrap().width(), 64);
        // a garbage catalog degrades to the loose png
        std::fs::write(bundle.join("Assets.car"), b"BOMStorejunk").unwrap();
        assert!(extract_icon(&bundle, &info, &dir.join("loose.png")));
        assert_eq!(std::fs::read(dir.join("loose.png")).unwrap(), b"loose");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn android_without_package_keeps_generic_names() {
        assert_eq!(android_identity(None, false).name, "Android Emulator");
        assert_eq!(android_identity(None, true).name, "Android Device");
    }

    #[test]
    fn icns_icon_is_decoded_and_shrunk_without_external_tools() {
        let dir = tmp("icns");
        let bundle = dir.join("Demo.app");
        std::fs::create_dir_all(bundle.join("Contents/Resources")).unwrap();
        let mut png = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgba8(256, 256).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        let icns = [b"icns".as_slice(), &(png.len() as u32 + 16).to_be_bytes(), b"ic08", &(png.len() as u32 + 8).to_be_bytes(), &png].concat();
        std::fs::write(bundle.join("Contents/Resources/Icon.icns"), icns).unwrap();
        let info = BundleInfo { icon_files: vec!["Icon".into()], ..Default::default() };
        let dest = dir.join("out.png");
        assert!(extract_icon(&bundle, &info, &dest));
        let out = image::open(&dest).unwrap();
        assert_eq!((out.width(), out.height()), (64, 64));
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_info_plist_through_plutil() {
        let dir = tmp("plist");
        let bundle = dir.join("Thing.app");
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        std::fs::write(
            bundle.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
            <key>CFBundleName</key><string>Thing</string>
            <key>CFBundleIdentifier</key><string>com.test.thing</string>
            <key>CFBundleIconFile</key><string>Icon</string></dict></plist>"#,
        )
        .unwrap();
        let i = read_info(&bundle).unwrap();
        assert_eq!((i.name.as_str(), i.bundle_id.as_str()), ("Thing", "com.test.thing"));
        assert_eq!(i.icon_files, ["Icon"]);
        std::fs::remove_dir_all(dir).ok();
    }
}
