use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellScan {
    pub(crate) external_dirs: BTreeSet<String>,
    pub(crate) command_patterns: BTreeSet<String>,
    pub(crate) always_patterns: BTreeSet<String>,
}

const CWD_COMMANDS: &[&str] = &["cd", "chdir", "popd", "pushd"];
const FILE_COMMANDS: &[&str] = &[
    "cd", "chdir", "popd", "pushd", "rm", "cp", "mv", "mkdir", "touch", "chmod", "chown",
    "cat",
];

pub(crate) fn scan(command: &str, cwd: &Path, project_root: &Path) -> ShellScan {
    let mut scan = ShellScan::default();
    for segment in command_segments(command) {
        let tokens = shell_words(&segment);
        let Some(name) = tokens.first() else {
            continue;
        };
        let lowered = name.to_ascii_lowercase();

        if FILE_COMMANDS.contains(&lowered.as_str()) {
            for arg in path_args(&tokens) {
                if let Some(path) = resolve_shell_path(arg, cwd) {
                    if !contained_by(&path, project_root) {
                        scan.external_dirs.insert(external_pattern(
                            &path,
                            CWD_COMMANDS.contains(&lowered.as_str()),
                        ));
                    }
                }
            }
        }

        if !CWD_COMMANDS.contains(&lowered.as_str()) {
            scan.command_patterns.insert(segment.clone());
            scan.always_patterns.insert(always_pattern(&tokens));
        }
    }
    scan
}

fn command_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut quote = None;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            current.push(ch);
            escaped = true;
            continue;
        }
        if matches!(quote, Some(q) if q == ch) {
            current.push(ch);
            quote = None;
            continue;
        }
        if quote.is_none() && (ch == '\'' || ch == '"') {
            current.push(ch);
            quote = Some(ch);
            continue;
        }
        if quote.is_none() && (ch == ';' || ch == '\n') {
            push_segment(&mut segments, &mut current);
            continue;
        }
        if quote.is_none() && (ch == '&' || ch == '|') && chars.peek() == Some(&ch) {
            chars.next();
            push_segment(&mut segments, &mut current);
            continue;
        }
        current.push(ch);
    }

    push_segment(&mut segments, &mut current);
    segments
}

fn push_segment(segments: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }
    current.clear();
}

fn shell_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if matches!(quote, Some(q) if q == ch) {
            quote = None;
            continue;
        }
        if quote.is_none() && (ch == '\'' || ch == '"') {
            quote = Some(ch);
            continue;
        }
        if quote.is_none() && ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn path_args(tokens: &[String]) -> impl Iterator<Item = &str> {
    tokens.iter().skip(1).filter_map(|token| {
        if token.starts_with('-') || token.starts_with('+') || token.contains('=') {
            return None;
        }
        if matches!(token.as_str(), ">" | ">>" | "<" | "2>" | "2>>" | "&>") {
            return None;
        }
        Some(token.as_str())
    })
}

fn resolve_shell_path(raw: &str, cwd: &Path) -> Option<PathBuf> {
    let prefix = literal_prefix(raw)?;
    if prefix.contains('$') || prefix.contains('`') || prefix.contains("$(") {
        return None;
    }
    let expanded = expand_home(prefix);
    let path = Path::new(&expanded);
    Some(if path.is_absolute() {
        normalize(path)
    } else {
        normalize(&cwd.join(path))
    })
}

fn literal_prefix(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return None;
    }
    let wildcard = trimmed.find(['*', '?', '[']);
    let prefix = wildcard.map_or(trimmed, |index| &trimmed[..index]);
    if prefix.is_empty() {
        None
    } else {
        Some(prefix.trim_end_matches(['/', '\\']))
    }
}

fn expand_home(raw: &str) -> String {
    if raw == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| raw.to_string());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    raw.to_string()
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn contained_by(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

fn external_pattern(path: &Path, directory_arg: bool) -> String {
    let dir = if directory_arg || path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    format!("{}/*", dir.display())
}

fn always_pattern(tokens: &[String]) -> String {
    let command = tokens.first().map_or("*", String::as_str);
    format!("{command} *")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_multiple_command_patterns_and_always_prefixes() {
        let root = Path::new("/repo");
        let scan = scan("git status && echo done", root, root);
        assert!(scan.command_patterns.contains("git status"));
        assert!(scan.command_patterns.contains("echo done"));
        assert!(scan.always_patterns.contains("git *"));
        assert!(scan.always_patterns.contains("echo *"));
    }

    #[test]
    fn skips_bash_permission_for_cd_only_but_tracks_external_dir() {
        let scan = scan(
            "cd ../outside",
            Path::new("/repo/app"),
            Path::new("/repo/app"),
        );
        assert!(scan.command_patterns.is_empty());
        assert!(scan.external_dirs.contains("/repo/outside/*"));
    }

    #[test]
    fn detects_external_file_command_paths() {
        let scan = scan("cat /etc/hosts", Path::new("/repo"), Path::new("/repo"));
        assert!(scan.external_dirs.contains("/etc/*"));
    }
}
