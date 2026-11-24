use super::{Res, pac_url};
use crate::services::devices::run;

fn services() -> Result<Vec<String>, String> {
    Ok(run("networksetup", &["-listallnetworkservices"])?
        .lines()
        .skip(1)
        .filter(|l| !l.starts_with('*'))
        .map(String::from)
        .collect())
}

fn networksetup_field(text: &str, name: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim() == name)
        .map(|(_, v)| v.trim().to_string())
}

// A proxy the user configured themselves (e.g. a corporate one) is not ours.
fn web_proxy_is_ours(getwebproxy: &str, port: u16) -> bool {
    let field = |name| networksetup_field(getwebproxy, name);
    field("Enabled").as_deref() == Some("Yes")
        && field("Server").as_deref() == Some("127.0.0.1")
        && field("Port") == Some(port.to_string())
}

fn pac_is_ours(getautoproxyurl: &str) -> bool {
    networksetup_field_url(getautoproxyurl).as_deref() == Some(pac_url().as_str())
        && networksetup_field(getautoproxyurl, "Enabled").as_deref() == Some("Yes")
}

// the URL value contains ':' so the generic field split would cut it
fn networksetup_field_url(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("URL:"))
        .map(|v| v.trim().to_string())
}

pub fn pac_status() -> Result<bool, String> {
    for service in services()? {
        if pac_is_ours(&run("networksetup", &["-getautoproxyurl", &service])?) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the system web proxy points at this proxy on any network service.
pub fn system_proxy_status(port: u16) -> Result<bool, String> {
    for service in services()? {
        let text = run("networksetup", &["-getwebproxy", &service])?;
        if web_proxy_is_ours(&text, port) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn system_proxy(enable: bool, port: u16) -> Res {
    let p = port.to_string();
    let state = if enable { "on" } else { "off" };
    for s in services()? {
        // PAC and a global proxy are exclusive modes
        if pac_is_ours(&run("networksetup", &["-getautoproxyurl", &s])?) {
            run("networksetup", &["-setautoproxystate", &s, "off"])?;
        }
        if enable {
            run("networksetup", &["-setwebproxy", &s, "127.0.0.1", &p])?;
            run("networksetup", &["-setsecurewebproxy", &s, "127.0.0.1", &p])?;
        }
        run("networksetup", &["-setwebproxystate", &s, state])?;
        run("networksetup", &["-setsecurewebproxystate", &s, state])?;
    }
    Ok(format!("System proxy {state}"))
}

pub fn set_pac(url: &str, port: u16) -> Result<(), String> {
    for s in services()? {
        // never touch a web proxy the user configured themselves
        if web_proxy_is_ours(&run("networksetup", &["-getwebproxy", &s])?, port) {
            run("networksetup", &["-setwebproxystate", &s, "off"])?;
            run("networksetup", &["-setsecurewebproxystate", &s, "off"])?;
        }
        run("networksetup", &["-setautoproxyurl", &s, url])?;
        run("networksetup", &["-setautoproxystate", &s, "on"])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_web_proxy_on_our_port_counts_as_ours() {
        let ours = "Enabled: Yes\nServer: 127.0.0.1\nPort: 9090\nAuthenticated Proxy Enabled: 0";
        assert!(web_proxy_is_ours(ours, 9090));
        assert!(!web_proxy_is_ours(ours, 9091));
        assert!(!web_proxy_is_ours(&ours.replace("Yes", "No"), 9090));
        assert!(!web_proxy_is_ours(&ours.replace("127.0.0.1", "proxy.corp"), 9090));
    }

    #[test]
    fn pac_is_ours_needs_our_url_and_enabled() {
        let out = |url: &str, on: &str| format!("URL: {url}\nEnabled: {on}\n");
        assert!(pac_is_ours(&out(&pac_url(), "Yes")));
        assert!(!pac_is_ours(&out(&pac_url(), "No")));
        assert!(!pac_is_ours(&out("http://corp/proxy.pac", "Yes")));
        assert!(!pac_url().contains(' '));
    }
}
