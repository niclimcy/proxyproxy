use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Who made a request, resolved lazily from the client process.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppIdentity {
    pub name: String,
    #[serde(default)]
    pub bundle_id: String,
    /// File stem of the cached PNG; empty when no icon could be extracted.
    #[serde(default)]
    pub icon_key: String,
    /// Simulator or emulator the app runs on; empty for host apps.
    #[serde(default)]
    pub platform: String,
    /// Safari's icon, carried by the shared simulator process whose page loads are labelled Safari.
    #[serde(default)]
    pub safari_icon_key: String,
}

pub const PLATFORM_IOS_SIM: &str = "iOS Simulator";
pub const PLATFORM_ANDROID_EMU: &str = "Android Emulator";
pub const PLATFORM_ANDROID_DEVICE: &str = "Android Device";

/// Short-lived sockets are gone before the process lookup runs, but a phone User-Agent
/// from a loopback client can only come from a simulator or emulator.
pub fn platform_from_ua(ua: &str) -> Option<&'static str> {
    if ua.contains("iPhone") || ua.contains("iPad") {
        Some(PLATFORM_IOS_SIM)
    } else if ua.contains("Android") {
        Some(PLATFORM_ANDROID_EMU)
    } else {
        None
    }
}

/// Fields of an `Info.plist` that identify an app and point at its icon.
#[derive(Debug, Default, PartialEq)]
pub struct BundleInfo {
    pub name: String,
    pub bundle_id: String,
    pub icon_files: Vec<String>,
}

/// Outermost `.app`: helper bundles nested in a framework should resolve to the host app.
pub fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in exe.components() {
        out.push(c);
        if c.as_os_str().to_str().is_some_and(|s| s.ends_with(".app")) {
            return Some(out);
        }
    }
    None
}

/// Safari's bundle inside the runtime that `exe` belongs to.
pub fn simulator_safari(exe: &Path) -> Option<PathBuf> {
    let s = exe.to_str()?;
    let root = &s[..s.find("/RuntimeRoot/")? + "/RuntimeRoot".len()];
    Some(PathBuf::from(root).join("Applications/MobileSafari.app"))
}

pub fn is_simulator_path(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.contains("/CoreSimulator/Devices/") || s.contains("/CoreSimulator/Volumes/")
        || s.contains("/RuntimeRoot/")
}

pub fn is_android_emulator(process: &str) -> bool {
    let p = process.rsplit(['/', '\\']).next().unwrap_or(process);
    p.starts_with("qemu-system") || p.starts_with("emulator")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS builds produce this
pub fn parse_info(v: &serde_json::Value, bundle: &Path) -> BundleInfo {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty());
    let stem = bundle
        .file_stem()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut icon_files: Vec<String> = ["CFBundleIconFile", "CFBundleIconName"]
        .iter()
        .filter_map(|k| s(k).map(String::from))
        .collect();
    // iOS keeps its icon names under CFBundleIcons -> CFBundlePrimaryIcon -> CFBundleIconFiles
    let ios = ["CFBundleIcons", "CFBundleIcons~ipad"]
        .iter()
        .filter_map(|k| v.get(k)?.get("CFBundlePrimaryIcon")?.get("CFBundleIconFiles"))
        .filter_map(|a| a.as_array())
        .flatten()
        .filter_map(|x| x.as_str().map(String::from));
    icon_files.extend(ios);
    // asset catalog icon set name, e.g. "AppIcon"
    icon_files.extend(
        ["CFBundleIcons", "CFBundleIcons~ipad"]
            .iter()
            .filter_map(|k| v.get(k)?.get("CFBundlePrimaryIcon")?.get("CFBundleIconName")?.as_str())
            .map(String::from),
    );
    BundleInfo {
        name: s("CFBundleDisplayName")
            .or(s("CFBundleName"))
            .map(String::from)
            .unwrap_or(stem),
        bundle_id: s("CFBundleIdentifier").unwrap_or_default().into(),
        icon_files,
    }
}

/// Changes when the app is updated, so a stale icon is never reused.
pub fn icon_key(id_or_path: &str, mtime_secs: u64) -> String {
    let safe: String = id_or_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    let tail = &safe[safe.len().saturating_sub(80)..];
    format!("{tail}-{mtime_secs}")
}

/// Largest by size wins; the biggest `AppIcon*.png` has the most detail.
pub fn largest_icon(files: &[(String, u64)]) -> Option<&str> {
    files
        .iter()
        .filter(|(n, _)| n.starts_with("AppIcon") && n.ends_with(".png"))
        .max_by_key(|(_, size)| *size)
        .map(|(n, _)| n.as_str())
}

/// A dotted Java-style package name such as `com.example.app`.
pub fn is_package_id(t: &str) -> bool {
    t.split('.').count() >= 3
        && t.split('.').all(|p| {
            p.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// Android apps usually put their package in the User-Agent (`com.example.app/1.2`).
pub fn package_from_user_agent(ua: &str) -> Option<String> {
    ua.split(['/', ' ', '(', ')', ';']).find(|t| is_package_id(t)).map(String::from)
}

/// `X-Requested-With` is the package id on WebView requests, so it wins over the
/// User-Agent. Plain okhttp/HttpURLConnection traffic carries neither.
pub fn package_hint(requested_with: &str, ua: &str) -> Option<String> {
    let rw = requested_with.trim();
    is_package_id(rw).then(|| rw.to_string()).or_else(|| package_from_user_agent(ua))
}

/// `adb` forwards USB devices' traffic, so the local peer is the adb server itself.
pub fn is_adb_bridge(process: &str) -> bool {
    let p = process.rsplit(['/', '\\']).next().unwrap_or(process).to_ascii_lowercase();
    p == "adb" || p == "adb.exe"
}

/// Stable slot so an app keeps its avatar colour across runs.
pub fn avatar_slot(name: &str, slots: usize) -> usize {
    let h = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    h as usize % slots.max(1)
}

pub fn avatar_letter(name: &str) -> char {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('?')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bundle_is_outermost_app() {
        let p = Path::new("/Applications/Google Chrome.app/Contents/Frameworks/X.framework/Helper.app/Contents/MacOS/Helper");
        assert_eq!(app_bundle(p), Some(PathBuf::from("/Applications/Google Chrome.app")));
        assert_eq!(app_bundle(Path::new("/usr/bin/curl")), None);
    }

    #[test]
    fn simulator_paths_detected() {
        let app = "/Users/u/Library/Developer/CoreSimulator/Devices/ABC/data/Containers/Bundle/Application/DEF/Maps.app/Maps";
        assert!(is_simulator_path(Path::new(app)));
        assert_eq!(
            app_bundle(Path::new(app)).unwrap().file_name().unwrap(),
            "Maps.app"
        );
        assert!(is_simulator_path(Path::new("/Library/Developer/CoreSimulator/Volumes/iOS/x/RuntimeRoot/Applications/MobileSafari.app/MobileSafari")));
        assert!(!is_simulator_path(Path::new("/Applications/Safari.app/Contents/MacOS/Safari")));
    }

    #[test]
    fn plist_fields_prefer_display_name() {
        let v = json!({"CFBundleDisplayName": "Maps", "CFBundleName": "M", "CFBundleIdentifier": "com.x.maps", "CFBundleIconFile": "AppIcon"});
        let i = parse_info(&v, Path::new("/a/M.app"));
        assert_eq!((i.name.as_str(), i.bundle_id.as_str()), ("Maps", "com.x.maps"));
        assert_eq!(i.icon_files, ["AppIcon"]);
        let bare = parse_info(&json!({}), Path::new("/a/Foo.app"));
        assert_eq!(bare.name, "Foo");
    }

    #[test]
    fn plist_collects_ios_icon_files() {
        let v = json!({"CFBundleIcons": {"CFBundlePrimaryIcon": {"CFBundleIconFiles": ["AppIcon60x60"]}}});
        assert_eq!(parse_info(&v, Path::new("/a/B.app")).icon_files, ["AppIcon60x60"]);
    }

    #[test]
    fn icon_key_changes_with_mtime_and_is_filename_safe() {
        assert_ne!(icon_key("com.x.a", 1), icon_key("com.x.a", 2));
        let k = icon_key("/Applications/My App.app", 7);
        assert!(!k.contains('/') && !k.contains(' '));
    }

    #[test]
    fn largest_app_icon_picked() {
        let f = vec![
            ("AppIcon60x60@2x.png".to_string(), 100),
            ("AppIcon76x76@2x~ipad.png".to_string(), 300),
            ("Other.png".to_string(), 999),
        ];
        assert_eq!(largest_icon(&f), Some("AppIcon76x76@2x~ipad.png"));
        assert_eq!(largest_icon(&[]), None);
    }

    #[test]
    fn package_hint_and_emulator_detection() {
        assert_eq!(
            package_from_user_agent("com.example.shop/3.1 okhttp/4.9").as_deref(),
            Some("com.example.shop")
        );
        assert_eq!(package_from_user_agent("curl/8.0 Mozilla"), None);
        assert!(is_android_emulator("/Users/u/Library/Android/sdk/emulator/qemu/darwin-aarch64/qemu-system-aarch64"));
        assert!(!is_android_emulator("curl"));
    }

    #[test]
    fn package_hint_prefers_requested_with_and_validates() {
        assert_eq!(package_hint("com.a.b", "com.x.y/1").as_deref(), Some("com.a.b"));
        assert_eq!(package_hint("", "com.x.y/1 okhttp/4.9").as_deref(), Some("com.x.y"));
        assert_eq!(package_hint("not a package", "okhttp/4.9"), None);
        assert!(!is_package_id("com..x") && !is_package_id("com.x") && !is_package_id("Com.x.y"));
        assert!(is_adb_bridge("/usr/bin/adb") && is_adb_bridge("C:\\sdk\\adb.exe") && !is_adb_bridge("adbd2"));
    }

    #[test]
    fn plist_collects_icon_set_name() {
        let v = json!({"CFBundleIcons": {"CFBundlePrimaryIcon": {"CFBundleIconName": "AppIcon", "CFBundleIconFiles": ["AppIcon60x60"]}}});
        assert_eq!(parse_info(&v, Path::new("/a/B.app")).icon_files, ["AppIcon60x60", "AppIcon"]);
    }

    #[test]
    fn avatar_slot_is_in_range_and_letter_falls_back() {
        assert!(avatar_slot("x", 5) < 5);
        assert_eq!(avatar_letter("maps"), 'M');
        assert_eq!(avatar_letter(""), '?');
    }

    #[test]
    fn identity_deserializes_without_optional_fields() {
        let i: AppIdentity = serde_json::from_str(r#"{"name":"A"}"#).unwrap();
        assert_eq!(i.icon_key, "");
    }

    #[test]
    fn phone_user_agents_imply_a_simulator() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15";
        assert_eq!(platform_from_ua(ua), Some(PLATFORM_IOS_SIM));
        assert_eq!(platform_from_ua("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"), None);
    }

    #[test]
    fn safari_bundle_is_found_beside_the_runtime_root_process() {
        let exe = Path::new("/V/iOS.simruntime/Contents/Resources/RuntimeRoot/usr/libexec/x");
        assert_eq!(
            simulator_safari(exe).unwrap(),
            Path::new("/V/iOS.simruntime/Contents/Resources/RuntimeRoot/Applications/MobileSafari.app")
        );
        assert!(simulator_safari(Path::new("/usr/bin/curl")).is_none());
    }
}
