use super::intent::{McpCopy, McpIntent};
use super::status::Flash;
use crate::domain::config::Shared;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum McpClient {
    Custom,
    ClaudeCode,
    Codex,
    GeminiCli,
    Antigravity,
}

impl McpClient {
    pub const ALL: [McpClient; 5] = [
        Self::Custom,
        Self::ClaudeCode,
        Self::Codex,
        Self::GeminiCli,
        Self::Antigravity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Custom => "Custom",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::GeminiCli => "Gemini CLI",
            Self::Antigravity => "Antigravity",
        }
    }
}

/// What to show for one client: the text to copy and where it goes.
pub struct Snippet {
    pub text: String,
    pub hint: &'static str,
}

pub struct McpVm {
    cfg: Shared,
    pub endpoint: String,
    snippets: Vec<(McpClient, Snippet)>,
    copied: Option<(McpCopy, Flash)>,
}

fn shell_quote(path: &str) -> String {
    if path.contains(char::is_whitespace) {
        format!("\"{path}\"")
    } else {
        path.to_string()
    }
}

impl McpVm {
    /// `exe` is this program's path, so stdio setups work even when it isn't on `PATH`.
    pub fn new(cfg: Shared, port: u16, api_port: u16, exe: Option<&str>) -> Self {
        let endpoint = format!("http://127.0.0.1:{api_port}/mcp");
        let exe = exe.unwrap_or("proxyproxy");
        let json = serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {
                "proxyproxy": { "command": exe, "args": ["mcp", "--port", port.to_string()] }
            }
        }))
        .expect("a json! value always serializes");
        let http = |cli: &str| format!("{cli} mcp add --transport http proxyproxy {endpoint}");
        let snippets = McpClient::ALL
            .into_iter()
            .map(|c| {
                let (text, hint) = match c {
                    McpClient::Custom => (
                        json.clone(),
                        "Paste into your client's MCP config (mcpServers)",
                    ),
                    McpClient::ClaudeCode => (http("claude"), "Run in a terminal"),
                    McpClient::Codex => (
                        format!(
                            "codex mcp add proxyproxy -- {} mcp --port {port}",
                            shell_quote(exe)
                        ),
                        "Run in a terminal (stdio)",
                    ),
                    McpClient::GeminiCli => (http("gemini"), "Run in a terminal"),
                    McpClient::Antigravity => (
                        json.clone(),
                        "In the agent panel: MCP servers → Manage MCP Servers → View raw config, then paste into mcp_config.json",
                    ),
                };
                (c, Snippet { text, hint })
            })
            .collect();
        Self {
            cfg,
            endpoint,
            snippets,
            copied: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.cfg.lock().unwrap().mcp_enabled
    }

    pub fn snippet(&self, client: McpClient) -> &Snippet {
        &self
            .snippets
            .iter()
            .find(|(c, _)| *c == client)
            .expect("every client has a snippet")
            .1
    }

    pub fn copied(&self, what: McpCopy) -> bool {
        self.copied
            .as_ref()
            .is_some_and(|(w, f)| *w == what && f.on())
    }

    pub fn handle(&mut self, intent: McpIntent) {
        match intent {
            McpIntent::SetEnabled(on) => self.cfg.lock().unwrap().mcp_enabled = on,
            McpIntent::Copied(what) => {
                let mut flash = Flash::default();
                flash.mark();
                self.copied = Some((what, flash));
            }
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

    fn snippet(vm: &McpVm, c: McpClient) -> &str {
        &vm.snippet(c).text
    }

    #[test]
    fn json_snippet_is_valid_and_carries_exe_and_port() {
        let (vm, _) = vm(Some("/Applications/Proxy Proxy.app/pp"));
        for c in [McpClient::Custom, McpClient::Antigravity] {
            let v: serde_json::Value = serde_json::from_str(snippet(&vm, c)).unwrap();
            let server = &v["mcpServers"]["proxyproxy"];
            assert_eq!(server["command"], "/Applications/Proxy Proxy.app/pp");
            assert_eq!(
                server["args"],
                serde_json::json!(["mcp", "--port", "19090"])
            );
        }
        let (bare, _) = self::vm(None);
        assert!(snippet(&bare, McpClient::Custom).contains("\"proxyproxy\""));
    }

    #[test]
    fn cli_commands_use_the_right_endpoint_and_port() {
        let (vm, _) = vm(None);
        assert_eq!(vm.endpoint, "http://127.0.0.1:19091/mcp");
        assert_eq!(
            snippet(&vm, McpClient::ClaudeCode),
            "claude mcp add --transport http proxyproxy http://127.0.0.1:19091/mcp"
        );
        assert_eq!(
            snippet(&vm, McpClient::GeminiCli),
            "gemini mcp add --transport http proxyproxy http://127.0.0.1:19091/mcp"
        );
        assert_eq!(
            snippet(&vm, McpClient::Codex),
            "codex mcp add proxyproxy -- proxyproxy mcp --port 19090"
        );
        let (spaced, _) = self::vm(Some("/Applications/Proxy Proxy.app/pp"));
        assert_eq!(
            snippet(&spaced, McpClient::Codex),
            "codex mcp add proxyproxy -- \"/Applications/Proxy Proxy.app/pp\" mcp --port 19090"
        );
    }

    #[test]
    fn copy_feedback_is_per_snippet() {
        let (mut vm, _) = vm(None);
        let claude = McpCopy::Client(McpClient::ClaudeCode);
        vm.handle(McpIntent::Copied(claude));
        assert!(vm.copied(claude) && !vm.copied(McpCopy::Endpoint));
    }
}
