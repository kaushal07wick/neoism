//! Pure filter for git state filesystem watcher events.
//!
//! The native `screen` loop subscribes to `.git/` directory changes via
//! the `notify` crate so the chrome git-branch indicator + diff panels
//! stay live. Most events fired by the OS are noise — lock-file churn,
//! pathless `Any` events, etc. This module owns the decision so web
//! frontends (which receive events through a daemon socket rather than
//! `notify`) can apply the same rules without depending on `notify`.
//!
//! The notify-specific translation lives in the desktop host; the
//! watcher passes us a [`WatcherEventView`] POD with the same shape and
//! we return whether the event is worth a re-scan.
//!
//! ## Path relevance
//!
//! Only paths inside `.git/` that map to refs, the index, or one of the
//! well-known head pointers should retake the indicator. Lock files
//! (`*.lock`) are skipped because they fire ~10x per `git commit`.
//!
//! ## Event-kind relevance
//!
//! `notify` reports a wide vocabulary of events; the ones that
//! reflect content changes are `Create`/`Modify`/`Remove`, plus the
//! Linux `Access(Close(Write))` close-on-write notification — those
//! are the kinds we re-scan for. `Any` and `Other` are also accepted
//! because backends fall back to them when they can't classify the
//! event.
//!
//! See `screen/mod.rs::git_state_event_relevant` (desktop) for the
//! original native body this replaces.

use std::ffi::OsStr;
use std::path::{Component, Path};

/// Filesystem event kind class that the watcher filter cares about.
///
/// `notify::EventKind` is much richer than this enum; the desktop host
/// projects each event onto one of these variants before calling
/// [`event_kind_relevant`]. Web frontends construct the variant
/// directly from the daemon payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatcherEventKindClass {
    /// Backend-reported `Any` — accept by default; we can't tell.
    Any,
    /// File content close-on-write notification (Linux inotify).
    AccessCloseWrite,
    /// Create / modify / remove of any path. We don't distinguish here
    /// because the path-level filter handles refs vs. lockfiles.
    CreateOrModifyOrRemove,
    /// Backend-reported `Other` — accept by default.
    Other,
    /// Any kind we explicitly do not act on (rename without content,
    /// metadata-only events, etc.).
    Ignored,
}

/// Borrowed view of a filesystem watcher event suitable for the pure
/// relevance filter.
///
/// `need_rescan` mirrors `notify::Event::need_rescan()` — when the OS
/// reported a queue overflow we must re-scan even if we have no
/// individual paths.
#[derive(Clone, Copy, Debug)]
pub struct WatcherEventView<'a> {
    pub kind: WatcherEventKindClass,
    pub need_rescan: bool,
    pub paths: &'a [&'a Path],
}

/// True when the event-kind class is one of the variants the git
/// watcher acts on. Mirrors the match in `screen/mod.rs::
/// git_state_event_kind`.
#[inline]
pub fn event_kind_relevant(kind: WatcherEventKindClass) -> bool {
    matches!(
        kind,
        WatcherEventKindClass::Any
            | WatcherEventKindClass::AccessCloseWrite
            | WatcherEventKindClass::CreateOrModifyOrRemove
            | WatcherEventKindClass::Other,
    )
}

/// True when the event should re-fetch the chrome git state.
pub fn event_relevant(event: WatcherEventView<'_>) -> bool {
    event_kind_relevant(event.kind)
        && (event.need_rescan || event.paths.iter().copied().any(path_relevant))
}

/// True when `path` is one of the `.git/` files whose change should
/// trigger a re-scan (refs/, HEAD pointers, the index, packed-refs,
/// the repo config). Lock files (`*.lock`) are skipped.
pub fn path_relevant(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };

    if name.ends_with(".lock") {
        return false;
    }

    matches!(
        name,
        "HEAD"
            | "ORIG_HEAD"
            | "FETCH_HEAD"
            | "MERGE_HEAD"
            | "CHERRY_PICK_HEAD"
            | "REVERT_HEAD"
            | "BISECT_LOG"
            | "index"
            | "packed-refs"
            | "config"
    ) || path.components().any(|component| {
        matches!(component, Component::Normal(part) if part == OsStr::new("refs"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn view<'a>(
        kind: WatcherEventKindClass,
        need_rescan: bool,
        paths: &'a [&'a Path],
    ) -> WatcherEventView<'a> {
        WatcherEventView {
            kind,
            need_rescan,
            paths,
        }
    }

    #[test]
    fn ignores_lock_churn() {
        let p = PathBuf::from("/repo/.git/index.lock");
        let slice: &[&Path] = &[p.as_path()];
        assert!(!event_relevant(view(
            WatcherEventKindClass::CreateOrModifyOrRemove,
            false,
            slice,
        )));
    }

    #[test]
    fn ignores_pathless_churn() {
        let slice: &[&Path] = &[];
        assert!(!event_relevant(view(
            WatcherEventKindClass::CreateOrModifyOrRemove,
            false,
            slice,
        )));
    }

    #[test]
    fn accepts_stable_git_state() {
        for path in [
            PathBuf::from("/repo/.git/index"),
            PathBuf::from("/repo/.git/HEAD"),
            PathBuf::from("/repo/.git/refs/heads/main"),
        ] {
            let slice: &[&Path] = &[path.as_path()];
            assert!(event_relevant(view(
                WatcherEventKindClass::CreateOrModifyOrRemove,
                false,
                slice,
            )));
        }
    }

    #[test]
    fn need_rescan_short_circuits_path_check() {
        let slice: &[&Path] = &[];
        assert!(event_relevant(view(
            WatcherEventKindClass::Any,
            true,
            slice,
        )));
    }

    #[test]
    fn ignored_kind_short_circuits() {
        let p = PathBuf::from("/repo/.git/HEAD");
        let slice: &[&Path] = &[p.as_path()];
        assert!(!event_relevant(view(
            WatcherEventKindClass::Ignored,
            false,
            slice,
        )));
    }
}
