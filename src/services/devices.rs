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

#[cfg(target_os = "macos")]
pub fn ios_sim_install_ca(cert: &str) -> Res {
    run(
        "xcrun",
        &["simctl", "keychain", "booted", "add-root-cert", cert],
    )?;
    Ok("CA installed on booted iOS simulator(s)".into())
}

#[cfg(not(target_os = "macos"))]
pub fn ios_sim_install_ca(_: &str) -> Res {
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
