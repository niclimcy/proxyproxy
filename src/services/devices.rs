use std::process::Command;

type Res = Result<String, String>;

fn run(cmd: &str, args: &[&str]) -> Res {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
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
pub fn android_set_proxy(port: u16) -> Res {
    let p = port.to_string();
    let addr = format!("127.0.0.1:{port}");
    each_android(|d| {
        run(
            "adb",
            &["-s", d, "reverse", &format!("tcp:{p}"), &format!("tcp:{p}")],
        )?;
        run(
            "adb",
            &[
                "-s",
                d,
                "shell",
                "settings",
                "put",
                "global",
                "http_proxy",
                &addr,
            ],
        )
    })
    .map(|s| format!("Proxy enabled on {s}"))
}

pub fn android_clear_proxy(port: u16) -> Res {
    each_android(|d| {
        let _ = run(
            "adb",
            &["-s", d, "reverse", "--remove", &format!("tcp:{port}")],
        );
        run(
            "adb",
            &[
                "-s",
                d,
                "shell",
                "settings",
                "put",
                "global",
                "http_proxy",
                ":0",
            ],
        )
    })
    .map(|s| format!("Proxy cleared on {s}"))
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IosSimulatorState {
    Booted,
    Shutdown,
    Other,
}

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
pub fn ios_simulators() -> Result<Vec<IosSimulator>, String> {
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

#[cfg(target_os = "macos")]
pub fn refresh_ios_ca_status(simulators: &mut [IosSimulator], cert: &str) {
    let fingerprint = match run(
        "openssl",
        &["x509", "-in", cert, "-noout", "-fingerprint", "-sha256"],
    ) {
        Ok(text) => text
            .split_once('=')
            .map(|(_, value)| value.replace(':', "").trim().to_ascii_uppercase()),
        Err(_) => None,
    };

    let Some(fingerprint) = fingerprint else {
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
                sim.ca_status = if truststore_contains(&rows, &fingerprint) {
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

#[cfg(not(target_os = "macos"))]
pub fn refresh_ios_ca_status(_: &mut [IosSimulator], _: &str) {}

#[cfg(not(target_os = "macos"))]
pub fn ios_simulators() -> Result<Vec<IosSimulator>, String> {
    Err("iOS simulator requires macOS".into())
}

#[cfg(target_os = "macos")]
fn services() -> Result<Vec<String>, String> {
    Ok(run("networksetup", &["-listallnetworkservices"])?
        .lines()
        .skip(1)
        .filter(|l| !l.starts_with('*'))
        .map(String::from)
        .collect())
}

#[cfg(target_os = "macos")]
pub fn system_proxy_status() -> Result<bool, String> {
    for service in services()? {
        let text = run("networksetup", &["-getwebproxy", &service])?;
        if text.lines().any(|line| line.trim() == "Enabled: Yes") {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(target_os = "macos"))]
pub fn system_proxy_status() -> Result<bool, String> {
    Ok(false)
}

#[cfg(target_os = "macos")]
pub fn system_proxy(enable: bool, port: u16) -> Res {
    let p = port.to_string();
    let state = if enable { "on" } else { "off" };
    for s in services()? {
        if enable {
            run("networksetup", &["-setwebproxy", &s, "127.0.0.1", &p])?;
            run("networksetup", &["-setsecurewebproxy", &s, "127.0.0.1", &p])?;
        }
        run("networksetup", &["-setwebproxystate", &s, state])?;
        run("networksetup", &["-setsecurewebproxystate", &s, state])?;
    }
    Ok(format!("System proxy {state}"))
}

#[cfg(target_os = "linux")]
pub fn system_proxy(enable: bool, port: u16) -> Res {
    let p = port.to_string();
    if enable {
        for k in ["http", "https"] {
            let schema = format!("org.gnome.system.proxy.{k}");
            run("gsettings", &["set", &schema, "host", "127.0.0.1"])?;
            run("gsettings", &["set", &schema, "port", &p])?;
        }
    }
    run(
        "gsettings",
        &[
            "set",
            "org.gnome.system.proxy",
            "mode",
            if enable { "manual" } else { "none" },
        ],
    )?;
    Ok(format!(
        "System proxy {}",
        if enable { "on" } else { "off" }
    ))
}

#[cfg(target_os = "windows")]
pub fn system_proxy(enable: bool, port: u16) -> Res {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    run(
        "reg",
        &[
            "add",
            key,
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            if enable { "1" } else { "0" },
            "/f",
        ],
    )?;
    if enable {
        run(
            "reg",
            &[
                "add",
                key,
                "/v",
                "ProxyServer",
                "/t",
                "REG_SZ",
                "/d",
                &format!("127.0.0.1:{port}"),
                "/f",
            ],
        )?;
    }
    Ok(format!(
        "System proxy {}",
        if enable { "on" } else { "off" }
    ))
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
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn truststore_sha256_match_is_case_insensitive() {
        let sha256 = "D4E842E9A47EB58147DCBAEBC9784FCF5A39C3A642D358754B0DFF7BC8A5B9FB";
        assert!(truststore_contains(
            &format!("other\n{sha256}\n"),
            "d4e842e9a47eb58147dcbaebc9784fcf5a39c3a642d358754b0dff7bc8a5b9fb",
        ));
    }

    #[test]
    fn parses_only_available_booted_sims() {
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
