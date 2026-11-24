use super::sysproxy;
use crate::domain::pac::Route;
use std::process::Command;

type Res = Result<String, String>;

pub(crate) fn run(cmd: &str, args: &[&str]) -> Res {
    let mut c = Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // no console flash
    }
    let out = c.output().map_err(|e| format!("{cmd}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(text)
    } else {
        Err(format!(
            "{cmd} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn adb_devices() -> Result<Vec<String>, String> {
    let out = run("adb", &["devices"])?;
    Ok(out
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            let (serial, state) = (p.next()?, p.next()?);
            (state == "device").then(|| serial.to_string())
        })
        .collect())
}

fn each_android(f: impl Fn(&str) -> Res) -> Res {
    let devs = adb_devices()?;
    if devs.is_empty() {
        return Err("no Android devices/emulators connected".into());
    }
    let mut errs = vec![];
    for d in &devs {
        if let Err(e) = f(d) {
            errs.push(e);
        }
    }
    if errs.is_empty() {
        Ok(format!("{} Android device(s)", devs.len()))
    } else {
        Err(errs.join("; "))
    }
}

// `adb reverse` lets emulators and USB devices reach the host proxy via localhost
fn set_proxy_on(serial: &str, port: u16) -> Res {
    let p = port.to_string();
    run(
        "adb",
        &["-s", serial, "reverse", &format!("tcp:{p}"), &format!("tcp:{p}")],
    )?;
    run(
        "adb",
        &["-s", serial, "shell", "settings", "put", "global", "http_proxy", &proxy_addr(port)],
    )
}

fn clear_proxy_on(serial: &str, port: u16) -> Res {
    let _ = run(
        "adb",
        &["-s", serial, "reverse", "--remove", &format!("tcp:{port}")],
    );
    run(
        "adb",
        &["-s", serial, "shell", "settings", "put", "global", "http_proxy", ":0"],
    )
}

fn proxy_addr(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

pub fn android_set_proxy(port: u16) -> Res {
    each_android(|d| set_proxy_on(d, port)).map(|s| format!("Proxy enabled on {s}"))
}

pub fn android_toggle_proxy(serial: &str, port: u16, enable: bool) -> Res {
    if enable {
        set_proxy_on(serial, port).map(|_| format!("Proxy enabled on {serial}"))
    } else {
        clear_proxy_on(serial, port).map(|_| format!("Proxy cleared on {serial}"))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AndroidDevice {
    pub serial: String,
    pub name: String,
    /// Pointing at our proxy right now.
    pub proxied: bool,
}

// One adb call per device: line 1 is the global proxy setting, line 2 the model.
fn parse_android_state(serial: &str, out: &str, port: u16) -> AndroidDevice {
    let mut lines = out.lines().map(str::trim);
    let proxy = lines.next().unwrap_or("");
    let model = lines.next().unwrap_or("");
    AndroidDevice {
        serial: serial.to_string(),
        name: if model.is_empty() { serial } else { model }.to_string(),
        proxied: proxy == proxy_addr(port),
    }
}

fn android_devices(port: u16) -> Vec<AndroidDevice> {
    adb_devices()
        .unwrap_or_default()
        .iter()
        .map(|serial| {
            let out = run(
                "adb",
                &["-s", serial, "shell", "settings get global http_proxy; getprop ro.product.model"],
            )
            .unwrap_or_default();
            parse_android_state(serial, &out, port)
        })
        .collect()
}

// Android 7+ apps ignore user CAs unless their network security config opts in
pub fn android_install_ca(cert: &str) -> Res {
    each_android(|d| {
        run(
            "adb",
            &["-s", d, "push", cert, "/sdcard/Download/proxyproxy-ca.crt"],
        )?;
        run(
            "adb",
            &[
                "-s",
                d,
                "shell",
                "am",
                "start",
                "-a",
                "android.settings.SECURITY_SETTINGS",
            ],
        )
    })
    .map(|s| {
        format!(
            "CA pushed to Downloads on {s}; install via Settings > Security > Install from storage"
        )
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct IosSimulator {
    pub udid: String,
    pub name: String,
    pub runtime: String,
    pub state: IosSimulatorState,
    pub ca_status: IosCaStatus,
    pub data_path: String,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS builds produce these
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IosSimulatorState {
    Booted,
    Shutdown,
    Other,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS builds produce these
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IosCaStatus {
    Installed,
    Missing,
    Unknown,
}

#[cfg(target_os = "macos")]
fn parse_ios_simulators(json: &str) -> Result<Vec<IosSimulator>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid simctl JSON: {e}"))?;
    let devices = value
        .get("devices")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "simctl returned no devices".to_string())?;

    let mut sims = vec![];
    for (runtime, entries) in devices {
        let runtime_name = runtime
            .strip_prefix("com.apple.CoreSimulator.SimRuntime.")
            .unwrap_or(runtime);
        let Some(entries) = entries.as_array() else {
            continue;
        };
        for entry in entries {
            let is_available = entry
                .get("isAvailable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let state = entry.get("state").and_then(|v| v.as_str()).unwrap_or("");
            let simulator_state = match state {
                "Booted" => IosSimulatorState::Booted,
                "Shutdown" => IosSimulatorState::Shutdown,
                _ => IosSimulatorState::Other,
            };
            let udid = entry.get("udid").and_then(|v| v.as_str()).unwrap_or("");
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let data_path = entry.get("dataPath").and_then(|v| v.as_str()).unwrap_or("");
            if is_available && !udid.is_empty() {
                sims.push(IosSimulator {
                    udid: udid.to_string(),
                    name: name.to_string(),
                    runtime: runtime_name.to_string(),
                    state: simulator_state,
                    ca_status: IosCaStatus::Unknown,
                    data_path: data_path.to_string(),
                });
            }
        }
    }
    sims.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.udid.cmp(&b.udid)));
    Ok(sims)
}

#[cfg(target_os = "macos")]
fn ios_simulators() -> Result<Vec<IosSimulator>, String> {
    let json = run("xcrun", &["simctl", "list", "-j", "devices"])?;
    parse_ios_simulators(&json)
}

#[cfg(target_os = "macos")]
pub fn ios_sim_install_ca(udid: &str, cert: &str) -> Res {
    run(
        "xcrun",
        &["simctl", "keychain", udid, "add-root-cert", cert],
    )?;
    Ok("CA installed".into())
}

#[cfg(target_os = "macos")]
pub fn ios_sim_reboot(udid: &str) -> Res {
    run("xcrun", &["simctl", "reboot", udid])?;
    Ok("Simulator reboot started".into())
}

#[cfg(target_os = "macos")]
fn truststore_contains(rows: &str, sha256: &str) -> bool {
    rows.lines()
        .any(|row| row.trim().eq_ignore_ascii_case(sha256))
}

// The CA is created once at startup, so its fingerprint never changes while we run.
#[cfg(target_os = "macos")]
fn ca_fingerprint(cert: &str) -> Option<&'static str> {
    static FINGERPRINT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    FINGERPRINT
        .get_or_init(|| {
            run(
                "openssl",
                &["x509", "-in", cert, "-noout", "-fingerprint", "-sha256"],
            )
            .ok()?
            .split_once('=')
            .map(|(_, value)| value.replace(':', "").trim().to_ascii_uppercase())
        })
        .as_deref()
}

#[cfg(target_os = "macos")]
fn refresh_ios_ca_status(simulators: &mut [IosSimulator], cert: &str) {
    let Some(fingerprint) = ca_fingerprint(cert) else {
        for sim in simulators {
            sim.ca_status = IosCaStatus::Unknown;
        }
        return;
    };

    for sim in simulators {
        if sim.state != IosSimulatorState::Booted || sim.data_path.is_empty() {
            sim.ca_status = IosCaStatus::Unknown;
            continue;
        }
        let db = std::path::Path::new(&sim.data_path)
            .join("private/var/protected/trustd/private/TrustStore.sqlite3");
        let db = db.to_string_lossy();
        match run(
            "sqlite3",
            &[&db, "select upper(hex(sha256)) from tsettings;"],
        ) {
            Ok(rows) => {
                sim.ca_status = if truststore_contains(&rows, fingerprint) {
                    IosCaStatus::Installed
                } else {
                    IosCaStatus::Missing
                };
            }
            Err(_) => sim.ca_status = IosCaStatus::Unknown,
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ios_sim_install_ca(_: &str, _: &str) -> Res {
    Err("iOS simulator requires macOS".into())
}

#[cfg(not(target_os = "macos"))]
pub fn ios_sim_reboot(_: &str) -> Res {
    Err("iOS simulator requires macOS".into())
}

pub struct DeviceSnapshot {
    pub ios_simulators: Vec<IosSimulator>,
    pub android: Vec<AndroidDevice>,
    /// `None` where we can't read the system proxy setting.
    pub system_proxy_ours: Option<bool>,
    /// `None` where we can't read the auto-proxy (PAC) setting.
    pub pac_ours: Option<bool>,
}

/// Runs several subprocesses; call it off the UI thread.
pub fn snapshot(cert: &str, port: u16) -> DeviceSnapshot {
    #[cfg(target_os = "macos")]
    let ios = {
        let mut sims = ios_simulators().unwrap_or_default();
        refresh_ios_ca_status(&mut sims, cert);
        sims
    };
    #[cfg(not(target_os = "macos"))]
    let ios = {
        let _ = cert;
        vec![]
    };
    DeviceSnapshot {
        ios_simulators: ios,
        android: android_devices(port),
        system_proxy_ours: sysproxy::system_proxy_status(port).ok(),
        pac_ours: sysproxy::pac_status().ok(),
    }
}

/// Routes all traffic, or only the listed hosts, through the proxy. Rerunning it with a
/// new list rewrites the PAC file; connections already open keep their old route.
pub fn apply_route(route: &Route, port: u16) -> Res {
    match route {
        Route::All => sysproxy::system_proxy(true, port),
        Route::Hosts(hosts) => sysproxy::pac_proxy(hosts, port),
    }
}

#[cfg(target_os = "macos")]
pub fn trust_ca_on_host(cert: &str) -> Res {
    let kc = dirs::home_dir()
        .ok_or("no home dir")?
        .join("Library/Keychains/login.keychain-db");
    run(
        "security",
        &[
            "add-trusted-cert",
            "-r",
            "trustRoot",
            "-k",
            &kc.to_string_lossy(),
            cert,
        ],
    )?;
    Ok("CA trusted in login keychain".into())
}

#[cfg(target_os = "windows")]
pub fn trust_ca_on_host(cert: &str) -> Res {
    run("certutil", &["-user", "-addstore", "Root", cert])?;
    Ok("CA trusted in user Root store".into())
}

#[cfg(target_os = "linux")]
pub fn trust_ca_on_host(cert: &str) -> Res {
    Err(format!(
        "copy {cert} into /usr/local/share/ca-certificates/ and run update-ca-certificates"
    ))
}

pub fn env_snippet(port: u16, cert: &str) -> String {
    format!(
        "export HTTP_PROXY=http://127.0.0.1:{port} HTTPS_PROXY=http://127.0.0.1:{port} ALL_PROXY=http://127.0.0.1:{port}\nexport NODE_USE_ENV_PROXY=1 NODE_EXTRA_CA_CERTS={cert} SSL_CERT_FILE={cert} REQUESTS_CA_BUNDLE={cert} CURL_CA_BUNDLE={cert}"
    )
}

#[cfg(target_os = "macos")]
pub fn launch_chrome(port: u16) -> Res {
    let dir = crate::proxy::tls::dir().join("chrome-profile");
    Command::new("open")
        .args(["-na", "Google Chrome", "--args"])
        .arg(format!("--proxy-server=http://127.0.0.1:{port}"))
        .arg(format!("--user-data-dir={}", dir.display()))
        .args(["--no-first-run", "--no-default-browser-check"])
        .spawn()
        .map_err(|e| format!("open: {e}"))?;
    Ok(
        "Launched Chrome with a clean profile routed through proxyproxy (trust the CA first)"
            .into(),
    )
}

#[cfg(not(target_os = "macos"))]
pub fn launch_chrome(port: u16) -> Res {
    let dir = crate::proxy::tls::dir().join("chrome-profile");
    for bin in ["google-chrome", "chromium", "chrome", "chrome.exe"] {
        if Command::new(bin)
            .arg(format!("--proxy-server=http://127.0.0.1:{port}"))
            .arg(format!("--user-data-dir={}", dir.display()))
            .spawn()
            .is_ok()
        {
            return Ok("Launched Chrome with a clean profile routed through proxyproxy".into());
        }
    }
    Err("Chrome not found".into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::parse_android_state;

    #[test]
    fn android_state_reads_proxy_and_model() {
        let d = parse_android_state("emulator-5554", "127.0.0.1:9090\nPixel 8\n", 9090);
        assert!(d.proxied);
        assert_eq!(d.name, "Pixel 8");
        let d = parse_android_state("emulator-5554", "null\n\n", 9090);
        assert!(!d.proxied, "unset proxy reads as null");
        assert_eq!(d.name, "emulator-5554", "falls back to the serial");
        assert!(!parse_android_state("s", "127.0.0.1:1234\nx", 9090).proxied);
    }

    use super::*;

    #[test]
    fn truststore_sha256_match_is_case_insensitive() {
        let sha256 = "D4E842E9A47EB58147DCBAEBC9784FCF5A39C3A642D358754B0DFF7BC8A5B9FB";
        assert!(truststore_contains(
            &format!("other\n{sha256}\n"),
            "d4e842e9a47eb58147dcbaebc9784fcf5a39c3a642d358754b0dff7bc8a5b9fb",
        ));
    }

    #[test]
    fn parses_available_sims_in_any_state() {
        let json = r#"{
          "devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-27-0": [
              {"name":"iPhone A","udid":"UDID-A","state":"Booted","isAvailable":true,"dataPath":"/tmp/A"},
              {"name":"iPhone B","udid":"UDID-B","state":"Shutdown","isAvailable":true,"dataPath":"/tmp/B"},
              {"name":"iPhone C","udid":"UDID-C","state":"Booted","isAvailable":false,"dataPath":"/tmp/C"}
            ]
          }
        }"#;

        assert_eq!(
            parse_ios_simulators(json).unwrap(),
            vec![
                IosSimulator {
                    udid: "UDID-A".into(),
                    name: "iPhone A".into(),
                    runtime: "iOS-27-0".into(),
                    state: IosSimulatorState::Booted,
                    ca_status: IosCaStatus::Unknown,
                    data_path: "/tmp/A".into(),
                },
                IosSimulator {
                    udid: "UDID-B".into(),
                    name: "iPhone B".into(),
                    runtime: "iOS-27-0".into(),
                    state: IosSimulatorState::Shutdown,
                    ca_status: IosCaStatus::Unknown,
                    data_path: "/tmp/B".into(),
                },
            ]
        );
    }
}
