//! Finds the process on the client side of a loopback connection, and its executable.
use super::os_parse;
use std::path::PathBuf;
#[cfg(not(target_os = "linux"))]
use std::process::Command;

#[cfg(all(any(unix, windows), not(target_os = "linux")))]
fn run(cmd: &mut Command) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // no console flash
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Name and pid of the process that owns the client side of the connection.
/// Blocking, and slow on Windows; call off the proxy path.
#[cfg(target_os = "linux")]
pub fn lookup_app(port: u16, own_pid: u32) -> Option<(String, u32)> {
    use std::fs;
    let inodes: Vec<u64> = ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .filter_map(|f| fs::read_to_string(f).ok())
        .flat_map(|t| os_parse::proc_net_inodes(&t, port))
        .collect();
    if inodes.is_empty() {
        return None;
    }
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        // other users' fd dirs are unreadable; skipping them is expected
        let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        let owns = pid != own_pid
            && fds.flatten().any(|fd| {
                fs::read_link(fd.path())
                    .ok()
                    .and_then(|l| os_parse::socket_inode(&l.to_string_lossy()))
                    .is_some_and(|i| inodes.contains(&i))
            });
        if owns {
            let name = fs::read_to_string(entry.path().join("comm")).ok()?;
            return Some((name.trim().to_string(), pid));
        }
    }
    None
}

#[cfg(all(unix, not(target_os = "linux")))]
pub fn lookup_app(port: u16, own_pid: u32) -> Option<(String, u32)> {
    let text = run(Command::new("lsof").args(["-nP", &format!("-iTCP:{port}"), "-Fpc"]))?;
    os_parse::lsof_owner(&text, own_pid)
}

#[cfg(windows)]
pub fn lookup_app(port: u16, own_pid: u32) -> Option<(String, u32)> {
    let table = run(Command::new("netstat").args(["-ano", "-p", "TCP"]))?;
    let pid = os_parse::netstat_pid(&table, port, own_pid)?;
    let csv =
        run(Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]))?;
    Some((os_parse::tasklist_name(&csv)?, pid))
}

#[cfg(not(any(unix, windows)))]
pub fn lookup_app(_: u16, _: u32) -> Option<(String, u32)> {
    None
}

#[cfg(target_os = "linux")]
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(all(unix, not(target_os = "linux")))]
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    let out = run(Command::new("ps").args(["-o", "comm=", "-p", &pid.to_string()]))?;
    let s = out.trim();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

#[cfg(windows)]
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    let out = run(Command::new("powershell").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!("(Get-Process -Id {pid}).Path"),
    ]))?;
    let s = out.trim();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

#[cfg(not(any(unix, windows)))]
pub fn exe_path(_: u32) -> Option<PathBuf> {
    None
}
