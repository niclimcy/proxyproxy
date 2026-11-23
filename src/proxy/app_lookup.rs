#[cfg(unix)]
pub fn lookup_app(port: u16, own_pid: u32) -> Option<String> {
    let out = std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-Fpc"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (mut pid, mut found) = (0u32, None);
    for l in text.lines() {
        if let Some(p) = l.strip_prefix('p') {
            pid = p.parse().unwrap_or(0);
        } else if let Some(c) = l.strip_prefix('c')
            && pid != own_pid
            && found.is_none()
        {
            found = Some(c.to_string());
        }
    }
    found
}

#[cfg(not(unix))]
pub fn lookup_app(_: u16, _: u32) -> Option<String> {
    None
}
