use super::{Res, pac_url};
use crate::services::devices::run;
use crate::services::os_parse::{gsettings_port, gsettings_str};

const SCHEMA: &str = "org.gnome.system.proxy";

fn gs(args: &[&str]) -> Res {
    run("gsettings", args).map_err(|e| {
        format!("{e} (system proxy needs GNOME's gsettings; set it manually or use the env snippet)")
    })
}

fn get(schema: &str, key: &str) -> Result<String, String> {
    gs(&["get", schema, key]).map(|t| gsettings_str(&t))
}

pub fn system_proxy(enable: bool, port: u16) -> Res {
    if enable {
        for k in ["http", "https"] {
            let schema = format!("{SCHEMA}.{k}");
            gs(&["set", &schema, "host", "127.0.0.1"])?;
            gs(&["set", &schema, "port", &port.to_string()])?;
        }
    }
    gs(&["set", SCHEMA, "mode", if enable { "manual" } else { "none" }])?;
    Ok(format!("System proxy {}", if enable { "on" } else { "off" }))
}

pub fn system_proxy_status(port: u16) -> Result<bool, String> {
    if get(SCHEMA, "mode")? != "manual" {
        return Ok(false);
    }
    let http = format!("{SCHEMA}.http");
    Ok(get(&http, "host")? == "127.0.0.1"
        && gsettings_port(&gs(&["get", &http, "port"])?) == Some(port))
}

pub fn pac_status() -> Result<bool, String> {
    Ok(get(SCHEMA, "mode")? == "auto" && get(SCHEMA, "autoconfig-url")? == pac_url())
}

// mode `auto` replaces `manual`, so no separate step is needed to drop our global proxy
pub fn set_pac(url: &str, _: u16) -> Result<(), String> {
    gs(&["set", SCHEMA, "autoconfig-url", url])?;
    gs(&["set", SCHEMA, "mode", "auto"])?;
    Ok(())
}
