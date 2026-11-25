use super::{Res, pac_url};
use crate::services::devices::run;
use crate::services::os_parse::reg_value;

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

fn query(name: &str) -> Option<String> {
    reg_value(&run("reg", &["query", KEY, "/v", name]).ok()?, name)
}

fn set(name: &str, ty: &str, data: &str) -> Result<(), String> {
    run("reg", &["add", KEY, "/v", name, "/t", ty, "/d", data, "/f"]).map(drop)
}

fn proxy_enable(on: bool) -> Result<(), String> {
    set("ProxyEnable", "REG_DWORD", if on { "1" } else { "0" })
}

fn drop_our_pac() -> Result<(), String> {
    if pac_status()? {
        run("reg", &["delete", KEY, "/v", "AutoConfigURL", "/f"])?;
    }
    Ok(())
}

pub fn system_proxy(enable: bool, port: u16) -> Res {
    drop_our_pac()?;
    proxy_enable(enable)?;
    if enable {
        set("ProxyServer", "REG_SZ", &format!("127.0.0.1:{port}"))?;
    }
    Ok(format!(
        "System proxy {}",
        if enable { "on" } else { "off" }
    ))
}

pub fn system_proxy_status(port: u16) -> Result<bool, String> {
    Ok(query("ProxyEnable").as_deref() == Some("0x1")
        && query("ProxyServer").as_deref() == Some(format!("127.0.0.1:{port}").as_str()))
}

pub fn pac_status() -> Result<bool, String> {
    Ok(query("AutoConfigURL").as_deref() == Some(pac_url().as_str()))
}

// a user-set manual proxy stays as it is; only ours is switched off
pub fn set_pac(url: &str, port: u16) -> Result<(), String> {
    if system_proxy_status(port)? {
        proxy_enable(false)?;
    }
    set("AutoConfigURL", "REG_SZ", url)
}
