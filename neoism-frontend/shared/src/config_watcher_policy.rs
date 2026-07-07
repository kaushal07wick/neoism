//! Pure predicates for the config-file watcher.
//!
//! Lifted from desktop `terminal/watcher.rs` (originally `notify`-typed
//! helpers). The shared crate stays free of `notify::EventKind`, so we
//! expose a small enum mirror plus a path-set checker. Native callers
//! map `notify::EventKind` → [`ConfigWatcherEventKind`] before asking
//! the policy.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Mirror of `notify::EventKind` reduced to the variants the config
/// watcher cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWatcherEventKind {
    Any,
    Create,
    Modify,
    Remove,
    Other,
    /// Unhandled — should not trigger a reload.
    Unhandled,
}

/// Does this event kind warrant a config reload check?
pub const fn config_watcher_event_relevant(kind: ConfigWatcherEventKind) -> bool {
    matches!(
        kind,
        ConfigWatcherEventKind::Any
            | ConfigWatcherEventKind::Create
            | ConfigWatcherEventKind::Modify
            | ConfigWatcherEventKind::Remove
            | ConfigWatcherEventKind::Other
    )
}

/// Does the changed-paths list look like an edit to the config file?
///
/// - Empty list → assume relevant (some platforms omit paths).
/// - Otherwise → relevant if any entry matches `config_file_path` or
///   any leaf basename equals `config.toml`.
pub fn config_update_paths_match(config_file_path: &Path, paths: &[PathBuf]) -> bool {
    if paths.is_empty() {
        return true;
    }
    paths.iter().any(|path| {
        path == config_file_path || path.file_name() == Some(OsStr::new("config.toml"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn relevant_kinds_match() {
        assert!(config_watcher_event_relevant(ConfigWatcherEventKind::Any));
        assert!(config_watcher_event_relevant(ConfigWatcherEventKind::Create));
        assert!(config_watcher_event_relevant(ConfigWatcherEventKind::Modify));
        assert!(config_watcher_event_relevant(ConfigWatcherEventKind::Remove));
        assert!(config_watcher_event_relevant(ConfigWatcherEventKind::Other));
        assert!(!config_watcher_event_relevant(ConfigWatcherEventKind::Unhandled));
    }

    #[test]
    fn empty_paths_assumed_relevant() {
        assert!(config_update_paths_match(
            &PathBuf::from("/tmp/x/config.toml"),
            &[],
        ));
    }

    #[test]
    fn config_toml_basename_matches() {
        assert!(config_update_paths_match(
            &PathBuf::from("/tmp/x/config.toml"),
            &[PathBuf::from("/elsewhere/config.toml")],
        ));
    }

    #[test]
    fn unrelated_path_does_not_match() {
        assert!(!config_update_paths_match(
            &PathBuf::from("/tmp/x/config.toml"),
            &[PathBuf::from("/tmp/x/terminal-history")],
        ));
    }
}
