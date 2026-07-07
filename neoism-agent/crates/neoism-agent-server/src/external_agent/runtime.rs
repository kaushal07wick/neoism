use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalRuntime {
    OpenCode,
    Codex,
    Claude,
}

impl ExternalRuntime {
    pub(crate) fn resolve(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "opencode" | "open-code" => Some(Self::OpenCode),
            "codex" | "openai-codex" => Some(Self::Codex),
            "claude" | "claude-code" | "claude-agent" => Some(Self::Claude),
            _ => None,
        }
    }

    pub(crate) fn agent_name(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::OpenCode => "OpenCode",
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        }
    }

    pub(crate) fn provider_id(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub(crate) fn acp_config(self, cwd: &str) -> AcpServerConfig {
        match self {
            Self::OpenCode => AcpServerConfig::new(
                "opencode",
                "OpenCode",
                "opencode",
                PathBuf::from(cwd),
            )
            .args(["acp", "--cwd", cwd]),
            Self::Codex => package_acp_config(
                "codex",
                "Codex",
                "@zed-industries/codex-acp@latest",
                cwd,
            ),
            Self::Claude => {
                let mut config = package_acp_config(
                    "claude",
                    "Claude",
                    "@agentclientprotocol/claude-agent-acp@latest",
                    cwd,
                );
                if std::env::var_os("CLAUDE_CODE_EXECUTABLE").is_none() {
                    if let Some(path) = command_on_path("claude") {
                        config
                            .env
                            .push(("CLAUDE_CODE_EXECUTABLE".to_string(), path));
                    }
                }
                config
            }
        }
    }
}

pub(crate) fn is_external_agent(name: &str) -> bool {
    ExternalRuntime::resolve(name).is_some()
}

fn package_acp_config(
    id: &'static str,
    name: &'static str,
    package: &'static str,
    cwd: &str,
) -> AcpServerConfig {
    AcpServerConfig::new(id, name, "npx", PathBuf::from(cwd)).args(["--yes", package])
}

fn command_on_path(name: &str) -> Option<String> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let path = dir.join(name);
        if path.is_file() {
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}
