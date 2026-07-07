//! Shared IDE and agent tool install metadata.
//!
//! This module intentionally contains only portable lookup tables and
//! command-argument metadata. Hosts still own process execution,
//! filesystem writes, PATH probing, and platform-specific linking.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreesitterInstallSpec {
    pub lang: &'static str,
    pub display_name: &'static str,
    pub repo: &'static str,
    pub subdir: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentInstallSpec {
    pub id: &'static str,
    pub binary: &'static str,
    pub display_name: &'static str,
    pub manager: &'static str,
    pub method: AgentInstallMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentInstallMethod {
    NpmGlobal { package: &'static str },
    ShellPipe { url: &'static str },
}

pub fn treesitter_install_spec(lang: &str) -> Option<TreesitterInstallSpec> {
    match lang {
        "rust" => Some(TreesitterInstallSpec {
            lang: "rust",
            display_name: "Rust",
            repo: "https://github.com/tree-sitter/tree-sitter-rust",
            subdir: ".",
        }),
        "python" => Some(TreesitterInstallSpec {
            lang: "python",
            display_name: "Python",
            repo: "https://github.com/tree-sitter/tree-sitter-python",
            subdir: ".",
        }),
        "javascript" => Some(TreesitterInstallSpec {
            lang: "javascript",
            display_name: "JavaScript",
            repo: "https://github.com/tree-sitter/tree-sitter-javascript",
            subdir: ".",
        }),
        "typescript" => Some(TreesitterInstallSpec {
            lang: "typescript",
            display_name: "TypeScript",
            repo: "https://github.com/tree-sitter/tree-sitter-typescript",
            subdir: "typescript",
        }),
        "tsx" => Some(TreesitterInstallSpec {
            lang: "tsx",
            display_name: "TSX",
            repo: "https://github.com/tree-sitter/tree-sitter-typescript",
            subdir: "tsx",
        }),
        "go" => Some(TreesitterInstallSpec {
            lang: "go",
            display_name: "Go",
            repo: "https://github.com/tree-sitter/tree-sitter-go",
            subdir: ".",
        }),
        "lua" => Some(TreesitterInstallSpec {
            lang: "lua",
            display_name: "Lua",
            repo: "https://github.com/tree-sitter-grammars/tree-sitter-lua",
            subdir: ".",
        }),
        "json" => Some(TreesitterInstallSpec {
            lang: "json",
            display_name: "JSON",
            repo: "https://github.com/tree-sitter/tree-sitter-json",
            subdir: ".",
        }),
        "toml" => Some(TreesitterInstallSpec {
            lang: "toml",
            display_name: "TOML",
            repo: "https://github.com/tree-sitter-grammars/tree-sitter-toml",
            subdir: ".",
        }),
        "yaml" => Some(TreesitterInstallSpec {
            lang: "yaml",
            display_name: "YAML",
            repo: "https://github.com/tree-sitter-grammars/tree-sitter-yaml",
            subdir: ".",
        }),
        "markdown" => Some(TreesitterInstallSpec {
            lang: "markdown",
            display_name: "Markdown",
            repo: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
            subdir: "tree-sitter-markdown",
        }),
        "nix" => Some(TreesitterInstallSpec {
            lang: "nix",
            display_name: "Nix",
            repo: "https://github.com/nix-community/tree-sitter-nix",
            subdir: ".",
        }),
        _ => None,
    }
}

pub fn agent_install_spec(id: &str) -> Option<AgentInstallSpec> {
    match id {
        "claude" => Some(AgentInstallSpec {
            id: "claude",
            binary: "claude",
            display_name: "Claude Code",
            manager: "npm",
            method: AgentInstallMethod::NpmGlobal {
                package: "@anthropic-ai/claude-code",
            },
        }),
        "codex" => Some(AgentInstallSpec {
            id: "codex",
            binary: "codex",
            display_name: "Codex",
            manager: "npm",
            method: AgentInstallMethod::NpmGlobal {
                package: "@openai/codex",
            },
        }),
        "opencode" => Some(AgentInstallSpec {
            id: "opencode",
            binary: "opencode",
            display_name: "OpenCode",
            manager: "curl | bash",
            method: AgentInstallMethod::ShellPipe {
                url: "https://opencode.ai/install",
            },
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treesitter_specs_include_repo_and_subdir_mapping() {
        let tsx = treesitter_install_spec("tsx").unwrap();
        assert_eq!(
            tsx.repo,
            "https://github.com/tree-sitter/tree-sitter-typescript"
        );
        assert_eq!(tsx.subdir, "tsx");

        let rust = treesitter_install_spec("rust").unwrap();
        assert_eq!(rust.lang, "rust");
        assert_eq!(rust.subdir, ".");
    }

    #[test]
    fn agent_specs_include_install_strategy() {
        let codex = agent_install_spec("codex").unwrap();
        assert_eq!(
            codex.method,
            AgentInstallMethod::NpmGlobal {
                package: "@openai/codex",
            }
        );

        let opencode = agent_install_spec("opencode").unwrap();
        assert_eq!(
            opencode.method,
            AgentInstallMethod::ShellPipe {
                url: "https://opencode.ai/install",
            }
        );
    }
}
