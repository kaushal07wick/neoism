//! Pure context-manager and session policy shared by native and web.
//!
//! The desktop fork's `frontends/neoism/src/context/manager.rs` and
//! `frontends/neoism/src/screen/panes.rs` host the renderer/PTY/Sugarloaf
//! plumbing, but the *decisions* they make — when to refresh a title,
//! whether a path looks like a project root, what to remove when a route
//! exits — are renderer-neutral. They live here so the web frontend can
//! make the same calls and stay byte-for-byte consistent.
//!
//! Renderer-neutral: no Sugarloaf, Taffy, PTY, native-window dependencies.

use std::path::Path;
use web_time::{Duration, Instant};

/// How often the workspace title strip refreshes once at least one
/// context has reported a change. Matches the legacy desktop debounce
/// in `ContextManager::update_titles`.
pub const TITLE_UPDATE_INTERVAL: Duration = Duration::from_secs(2);

/// Decide whether `ContextManager::update_titles` should run this tick.
///
/// `last_update` is `None` when no refresh has happened yet (initial
/// boot) — the legacy code always runs the loop in that case.
/// Otherwise we require [`TITLE_UPDATE_INTERVAL`] to have elapsed.
#[inline]
pub fn title_update_should_run(last_update: Option<Instant>, now: Instant) -> bool {
    title_update_should_run_with_interval(last_update, now, TITLE_UPDATE_INTERVAL)
}

/// Variant of [`title_update_should_run`] that takes the interval
/// explicitly. Used by tests; callers in production should prefer
/// [`title_update_should_run`].
#[inline]
pub fn title_update_should_run_with_interval(
    last_update: Option<Instant>,
    now: Instant,
    interval: Duration,
) -> bool {
    match last_update {
        None => true,
        Some(prev) => now.saturating_duration_since(prev) > interval,
    }
}

/// What [`route_exit_plan`] decided the manager should do when a route
/// finishes (PTY exit, editor exit, tab close).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteExitPlan {
    /// No grid contained the route. The desktop fork closes the window
    /// when this is reported with an empty context list.
    Untracked { contexts_empty: bool },
    /// Remove the entire grid at `grid_index`. The workspace root has
    /// exited (or this grid only had a single context), so dropping
    /// the grid collapses the workspace.
    RemoveGrid { grid_index: usize },
    /// Remove just the route within `grid_index`. The grid still has
    /// other panes; the manager will pick a new focused route from
    /// what remains.
    RemoveRoute { grid_index: usize },
}

/// Input snapshot for [`route_exit_plan`]. Captured once so the policy
/// runs without re-borrowing the manager's contexts.
#[derive(Debug, Clone, Copy)]
pub struct RouteExitInput {
    /// Number of contexts in the manager.
    pub contexts_len: usize,
    /// Index of the grid that hosts `route_id`, or `None` if no grid
    /// claims it.
    pub host_grid_index: Option<usize>,
    /// Whether the host grid's workspace-root route equals `route_id`.
    /// Honored only when `host_grid_index` is `Some`.
    pub is_workspace_root: bool,
    /// Number of contexts in the host grid (before removal). Honored
    /// only when `host_grid_index` is `Some`.
    pub host_grid_len: usize,
}

/// Pure version of `ContextManager::should_close_context_manager`'s
/// branch logic. Given a snapshot of where `route_id` lives, decide
/// whether to drop the grid, drop just the route, or do nothing
/// because the route was already gone.
///
/// The desktop fork's `should_close_context_manager` returns
/// `is_empty()` after applying this plan; this helper only reports the
/// branch — the caller still mutates and decides whether to signal
/// window close based on whether the manager went empty afterwards.
#[inline]
pub fn route_exit_plan(input: RouteExitInput) -> RouteExitPlan {
    let Some(grid_index) = input.host_grid_index else {
        return RouteExitPlan::Untracked {
            contexts_empty: input.contexts_len == 0,
        };
    };

    if input.is_workspace_root || input.host_grid_len <= 1 {
        RouteExitPlan::RemoveGrid { grid_index }
    } else {
        RouteExitPlan::RemoveRoute { grid_index }
    }
}

/// Filesystem markers that flip a directory into "project root" mode in
/// the file tree / search / file-watcher subsystems. Mirrors the legacy
/// `is_project_workspace` heuristic in `screen/panes.rs`.
pub const PROJECT_WORKSPACE_MARKERS: &[&str] = &[
    ".git",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "build.gradle",
    "build.gradle.kts",
    "pom.xml",
    "CMakeLists.txt",
    "Makefile",
    ".vscode",
    ".idea",
    "deno.json",
    "deno.jsonc",
    "pnpm-workspace.yaml",
    "shard.yml",
    "mix.exs",
    "Gemfile",
];

/// "Looks like a project root" — heuristic used to decide whether to
/// install full live filesystem watching and full-fat search, or a
/// lightweight loose mode. The user opening a terminal at `~/` shouldn't
/// pay the cost of recursively watching `~/Library` for FSEvents or
/// letting `rg` walk every macOS cache folder. Any of:
///
/// - any `.git` (file or dir — submodules use a file pointer),
/// - any common project manifest (Cargo, npm, Python, Go, JVM, CMake, etc.),
/// - or an editor-config marker (`.vscode/`, `.idea/`),
///
/// flips us into full mode. Otherwise we treat the directory as a loose
/// browse target. Stats only — no recursion, no canonicalize.
#[inline]
pub fn is_project_workspace(path: &Path) -> bool {
    is_project_workspace_with_markers(path, PROJECT_WORKSPACE_MARKERS)
}

/// Variant of [`is_project_workspace`] that takes the marker set
/// explicitly. Used by tests; production callers should prefer
/// [`is_project_workspace`].
#[inline]
pub fn is_project_workspace_with_markers(path: &Path, markers: &[&str]) -> bool {
    markers.iter().any(|name| path.join(name).exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use web_time::Duration;

    #[test]
    fn title_update_runs_on_first_tick() {
        let now = Instant::now();
        assert!(title_update_should_run(None, now));
    }

    #[test]
    fn title_update_skips_inside_interval() {
        let now = Instant::now();
        let prev = now - Duration::from_millis(500);
        assert!(!title_update_should_run(Some(prev), now));
    }

    #[test]
    fn title_update_runs_after_interval() {
        let now = Instant::now();
        let prev = now - (TITLE_UPDATE_INTERVAL + Duration::from_millis(1));
        assert!(title_update_should_run(Some(prev), now));
    }

    #[test]
    fn title_update_strict_greater_than_legacy_match() {
        // The legacy desktop code uses `elapsed() > interval` (strict),
        // so exactly equal should NOT trigger.
        let interval = Duration::from_secs(2);
        let now = Instant::now();
        let prev = now - interval;
        assert!(!title_update_should_run_with_interval(
            Some(prev),
            now,
            interval
        ));
    }

    #[test]
    fn route_exit_plan_untracked_empty_list() {
        let plan = route_exit_plan(RouteExitInput {
            contexts_len: 0,
            host_grid_index: None,
            is_workspace_root: false,
            host_grid_len: 0,
        });
        assert_eq!(
            plan,
            RouteExitPlan::Untracked {
                contexts_empty: true
            }
        );
    }

    #[test]
    fn route_exit_plan_untracked_nonempty_list() {
        // Route was already removed but other tabs remain — manager
        // should NOT signal a window close.
        let plan = route_exit_plan(RouteExitInput {
            contexts_len: 3,
            host_grid_index: None,
            is_workspace_root: false,
            host_grid_len: 0,
        });
        assert_eq!(
            plan,
            RouteExitPlan::Untracked {
                contexts_empty: false
            }
        );
    }

    #[test]
    fn route_exit_plan_workspace_root_drops_whole_grid() {
        let plan = route_exit_plan(RouteExitInput {
            contexts_len: 2,
            host_grid_index: Some(1),
            is_workspace_root: true,
            host_grid_len: 3,
        });
        assert_eq!(plan, RouteExitPlan::RemoveGrid { grid_index: 1 });
    }

    #[test]
    fn route_exit_plan_single_pane_drops_whole_grid() {
        let plan = route_exit_plan(RouteExitInput {
            contexts_len: 2,
            host_grid_index: Some(0),
            is_workspace_root: false,
            host_grid_len: 1,
        });
        assert_eq!(plan, RouteExitPlan::RemoveGrid { grid_index: 0 });
    }

    #[test]
    fn route_exit_plan_leaf_pane_removes_only_route() {
        let plan = route_exit_plan(RouteExitInput {
            contexts_len: 1,
            host_grid_index: Some(0),
            is_workspace_root: false,
            host_grid_len: 4,
        });
        assert_eq!(plan, RouteExitPlan::RemoveRoute { grid_index: 0 });
    }

    #[test]
    fn route_exit_plan_workspace_root_wins_over_grid_len() {
        // Even with multiple peers, the workspace root exiting must
        // collapse the whole grid (matches the legacy comment about
        // root-terminal ownership).
        let plan = route_exit_plan(RouteExitInput {
            contexts_len: 2,
            host_grid_index: Some(2),
            is_workspace_root: true,
            host_grid_len: 5,
        });
        assert_eq!(plan, RouteExitPlan::RemoveGrid { grid_index: 2 });
    }

    /// Drop-guard scratch directory under [`std::env::temp_dir`]. Avoids
    /// pulling `tempfile` into the shared crate just for these tests —
    /// the markers heuristic is pure but it physically `stat`s paths,
    /// so we need a real directory on disk.
    struct ScratchDir(std::path::PathBuf);
    impl ScratchDir {
        fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "neoism-ui-context-policy-{label}-{}-{}-{}",
                std::process::id(),
                nanos,
                id,
            ));
            std::fs::create_dir_all(&path).expect("mkdir scratch");
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn project_workspace_empty_dir_is_loose() {
        let dir = ScratchDir::new("empty");
        assert!(!is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_with_git_dir_is_project() {
        let dir = ScratchDir::new("git-dir");
        std::fs::create_dir(dir.path().join(".git")).expect("mkdir .git");
        assert!(is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_with_git_file_is_project() {
        // Submodules expose `.git` as a *file* — must still flip the
        // heuristic on.
        let dir = ScratchDir::new("git-file");
        std::fs::write(dir.path().join(".git"), b"gitdir: ../parent/.git/modules/x")
            .expect("write .git");
        assert!(is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_with_cargo_is_project() {
        let dir = ScratchDir::new("cargo");
        std::fs::write(dir.path().join("Cargo.toml"), b"[package]\n").expect("write");
        assert!(is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_with_package_json_is_project() {
        let dir = ScratchDir::new("pkg");
        std::fs::write(dir.path().join("package.json"), b"{}\n").expect("write");
        assert!(is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_with_editor_config_dir_is_project() {
        let dir = ScratchDir::new("vscode");
        std::fs::create_dir(dir.path().join(".vscode")).expect("mkdir");
        assert!(is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_with_random_file_is_loose() {
        let dir = ScratchDir::new("random");
        std::fs::write(dir.path().join("notes.txt"), b"hi\n").expect("write");
        assert!(!is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_with_makefile_is_project() {
        let dir = ScratchDir::new("make");
        std::fs::write(dir.path().join("Makefile"), b"all:\n").expect("write");
        assert!(is_project_workspace(dir.path()));
    }

    #[test]
    fn project_workspace_custom_markers_match() {
        let dir = ScratchDir::new("custom");
        std::fs::write(dir.path().join("MY_MARKER"), b"").expect("write");
        assert!(is_project_workspace_with_markers(
            dir.path(),
            &["MY_MARKER"]
        ));
        assert!(!is_project_workspace_with_markers(dir.path(), &["OTHER"]));
    }
}
