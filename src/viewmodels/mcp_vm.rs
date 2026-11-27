use super::intent::{McpCopy, McpIntent};
use super::status::Flash;
use crate::domain::config::Shared;

pub struct McpVm {
    cfg: Shared,
    pub endpoint: String,
    pub claude_command: String,
    pub stdio_command: String,
    copied: [Flash; 3],
}

fn shell_quote(path: &str) -> String {
    if path.contains(char::is_whitespace) {
        format!("\"{path}\"")
    } else {
        path.to_string()
    }
}

impl McpVm {
    /// `exe` is this program's path, so the stdio command works even when it isn't on `PATH`.
    pub fn new(cfg: Shared, port: u16, api_port: u16, exe: Option<&str>) -> Self {
        let endpoint = format!("http://127.0.0.1:{api_port}/mcp");
        Self {
            cfg,
            claude_command: format!("claude mcp add --transport http proxyproxy {endpoint}"),
            stdio_command: format!(
                "{} mcp --port {port}",
                exe.map_or_else(|| "proxyproxy".to_string(), shell_quote)
            ),
            endpoint,
            copied: Default::default(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.cfg.lock().unwrap().mcp_enabled
    }

    pub fn copied(&self, what: McpCopy) -> bool {
        self.copied[what as usize].on()
    }

    pub fn handle(&mut self, intent: McpIntent) {
        match intent {
            McpIntent::SetEnabled(on) => self.cfg.lock().unwrap().mcp_enabled = on,
            McpIntent::Copied(what) => self.copied[what as usize].mark(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use std::sync::{Arc, Mutex};

    fn vm(exe: Option<&str>) -> (McpVm, Shared) {
        let cfg: Shared = Arc::new(Mutex::new(Config::default()));
        (McpVm::new(cfg.clone(), 19090, 19091, exe), cfg)
    }

    #[test]
    fn toggle_updates_config() {
        let (mut vm, cfg) = vm(None);
        assert!(!vm.enabled());
        vm.handle(McpIntent::SetEnabled(true));
        assert!(cfg.lock().unwrap().mcp_enabled && vm.enabled());
        vm.handle(McpIntent::SetEnabled(false));
        assert!(!cfg.lock().unwrap().mcp_enabled);
    }

    #[test]
    fn connection_strings_follow_the_ports_and_quote_the_exe_path() {
        let (vm, _) = vm(None);
        assert_eq!(vm.endpoint, "http://127.0.0.1:19091/mcp");
        assert_eq!(
            vm.claude_command,
            "claude mcp add --transport http proxyproxy http://127.0.0.1:19091/mcp"
        );
        assert_eq!(vm.stdio_command, "proxyproxy mcp --port 19090");
        let (plain, _) = self::vm(Some("/opt/pp"));
        assert_eq!(plain.stdio_command, "/opt/pp mcp --port 19090");
        let (spaced, _) = self::vm(Some("/Applications/Proxy Proxy.app/pp"));
        assert_eq!(
            spaced.stdio_command,
            "\"/Applications/Proxy Proxy.app/pp\" mcp --port 19090"
        );
    }

    #[test]
    fn copy_feedback_is_per_row() {
        let (mut vm, _) = vm(None);
        vm.handle(McpIntent::Copied(McpCopy::Stdio));
        assert!(vm.copied(McpCopy::Stdio) && !vm.copied(McpCopy::Endpoint));
    }
}
