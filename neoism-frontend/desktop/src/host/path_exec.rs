/// Format a path for the composer's cwd chip — `~` collapses HOME,
/// everything else stays absolute. Centralised here so the chip and the
/// block-card cwd label stay visually aligned.
pub(super) fn display_cwd(path: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        if let Ok(rest) = path.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

/// POSIX shell builtins that don't show up in `PATH`. Used by the
/// composer's command-validity classifier so typing `cd ` doesn't paint
/// the word red just because there's no `/bin/cd` binary.
pub(super) const SHELL_BUILTINS: &[&str] = &[
    "cd", "pwd", "echo", "exit", "exec", "source", ".", "set", "unset", "export",
    "alias", "unalias", "jobs", "fg", "bg", "type", "history", "return", "break",
    "continue", "eval", "read", "shift", "true", "false", "let", "local", "declare",
    "readonly", "test", "[", "trap", "wait", "umask", "ulimit", "command", "builtin",
    "enable", "disown", "suspend", "times", "hash", "help", "logout", "printf", "pushd",
    "popd", "dirs",
];

/// Walk `PATH` once and bucket every executable by basename. Cheap on
/// startup (Linux: a few ms for the average PATH); the cache lives for
/// the life of the renderer.
pub(super) fn build_path_executables() -> rustc_hash::FxHashSet<String> {
    let mut out = rustc_hash::FxHashSet::default();
    let Some(path_var) = std::env::var_os("PATH") else {
        return out;
    };
    for dir in std::env::split_paths(&path_var) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            #[cfg(unix)]
            let is_exec = {
                use std::os::unix::fs::PermissionsExt;
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let is_exec = metadata.is_file();
            if !is_exec {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                out.insert(name.to_string());
            }
        }
    }
    out
}
