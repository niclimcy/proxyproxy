//! System proxy and PAC settings, one backend per OS: macOS `networksetup`, Linux GNOME
//! `gsettings`, Windows registry. Unsupported setups return an `Err` hint, never panic.
use crate::domain::pac::{file_url, pac_script};
use std::path::PathBuf;

type Res = Result<String, String>;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
use other as imp;

pub use imp::{pac_status, system_proxy, system_proxy_status};

fn pac_path() -> PathBuf {
    crate::proxy::tls::dir().join("proxyproxy.pac")
}

fn pac_url() -> String {
    file_url(&pac_path().to_string_lossy())
}

/// Writes the PAC file and points the system's auto-proxy setting at it.
pub fn pac_proxy(hosts: &[String], port: u16) -> Res {
    std::fs::write(pac_path(), pac_script(hosts, port)).map_err(|e| format!("write PAC: {e}"))?;
    imp::set_pac(&pac_url(), port)?;
    Ok(format!("System proxy on for {} host(s) only", hosts.len()))
}
