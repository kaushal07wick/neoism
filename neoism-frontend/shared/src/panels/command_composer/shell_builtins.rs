//! POSIX shell builtins that don't show up in `PATH` lookups, plus the
//! cwd-display helper used by the composer's cwd chip.
//!
//! The composer's input classifier consults [`SHELL_BUILTINS`] before
//! falling back to a `PATH` scan so that typing `cd `, `pwd`, or
//! `source ` doesn't paint the word red just because there's no
//! corresponding binary on disk. The list is the conservative
//! intersection of bash + zsh + fish builtin sets — extending it is
//! safe (false negatives just over-color a token red), shrinking it
//! is not.
//!
//! Originally lived in `frontends/neoism/src/host/path_exec.rs`. Moved
//! here so the web frontend (which never touches `PATH`) can still
//! classify input identically.

use std::path::Path;

/// Conservative list of POSIX/bash/zsh/fish builtins. Stable order so
/// `contains()` stays branchless on a small string slice.
pub const SHELL_BUILTINS: &[&str] = &[
    "cd", "pwd", "echo", "exit", "exec", "source", ".", "set", "unset", "export",
    "alias", "unalias", "jobs", "fg", "bg", "type", "history", "return", "break",
    "continue", "eval", "read", "shift", "true", "false", "let", "local", "declare",
    "readonly", "test", "[", "trap", "wait", "umask", "ulimit", "command", "builtin",
    "enable", "disown", "suspend", "times", "hash", "help", "logout", "printf", "pushd",
    "popd", "dirs",
];

/// Format a path for the composer's cwd chip — `~` collapses HOME,
/// everything else stays absolute. Centralised here so the chip and
/// the block-card cwd label stay visually aligned across surfaces.
///
/// `home` is borrowed instead of pulled from `std::env::var_os("HOME")`
/// so the web frontend can supply a daemon-reported home dir without
/// touching env vars. Pass `None` to skip the `~` substitution.
pub fn display_cwd_with_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home {
        if let Ok(rest) = path.strip_prefix(home) {
            if rest.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

/// Convenience wrapper that pulls `HOME` from the process environment.
/// The desktop fork uses this — wasm builds should call
/// [`display_cwd_with_home`] directly with the daemon-reported home.
#[cfg(not(target_arch = "wasm32"))]
pub fn display_cwd(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    display_cwd_with_home(path, home.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builtins_contains_core_shell_words() {
        for word in ["cd", "pwd", "echo", "exit", "exec", "source", "."] {
            assert!(SHELL_BUILTINS.contains(&word), "missing {}", word);
        }
    }

    #[test]
    fn display_cwd_collapses_exact_home() {
        let home = PathBuf::from("/home/user");
        assert_eq!(
            display_cwd_with_home(&PathBuf::from("/home/user"), Some(&home)),
            "~"
        );
    }

    #[test]
    fn display_cwd_substitutes_home_prefix() {
        let home = PathBuf::from("/home/user");
        assert_eq!(
            display_cwd_with_home(&PathBuf::from("/home/user/projects"), Some(&home)),
            "~/projects"
        );
    }

    #[test]
    fn display_cwd_keeps_absolute_outside_home() {
        let home = PathBuf::from("/home/user");
        assert_eq!(
            display_cwd_with_home(&PathBuf::from("/etc/passwd"), Some(&home)),
            "/etc/passwd"
        );
    }

    #[test]
    fn display_cwd_without_home_returns_absolute() {
        assert_eq!(
            display_cwd_with_home(&PathBuf::from("/var/log/syslog"), None),
            "/var/log/syslog"
        );
    }
}
