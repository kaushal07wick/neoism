//! Behavior tests for the slim multi-mode finder.
//!
//! The finder owns the modal UI; the *search* (file walk, ripgrep,
//! git branch listing, ex-command match) belongs to the native shim,
//! so these tests stub it out. The host contract under test is:
//!
//! - Initial visibility is hidden, with no results.
//! - `show(mode)` flips visibility, records the mode, clears query +
//!   results.
//! - Hidden finder ignores every event.
//! - `Text` and `Backspace` mutate `current_query()`.
//! - `ArrowUp`/`ArrowDown` move + clamp the selection.
//! - `Enter` exposes `pick()` for the host and hides the panel.
//! - `Escape` hides without exposing a pick.
//! - `set_results` resets selection to row 0.
//! - `UiEvent::ServiceReply` with the awaited `RequestId` replaces
//!   the result list; replies with the wrong id are ignored.

use std::path::Path;
use std::time::Duration;

use neoism_ui::event::{
    KeyDescriptor, KeyState, LogicalKey, Modifiers, NamedKey, PhysicalKey, UiEvent,
};
use neoism_ui::panels::{Finder, FinderMode, FinderResult, Panel, PanelContext};
use neoism_ui::services::{
    ClipboardService, ClockService, CommandError, CommandService, DirEntry, FilesService,
    GitService, GitStatus, IoError, Services,
};
use neoism_ui::theme::ChromeTheme;

// ─── Null service stubs ───────────────────────────────────────────────

struct NullFiles;
impl FilesService for NullFiles {
    fn list_dir(&self, _path: &Path) -> Result<Vec<DirEntry>, IoError> {
        Ok(Vec::new())
    }
    fn read_file(&self, _path: &Path) -> Result<Vec<u8>, IoError> {
        Err(IoError::NotFound("test".into()))
    }
    fn write_file(&self, _path: &Path, _bytes: &[u8]) -> Result<(), IoError> {
        Ok(())
    }
    fn stat(&self, _path: &Path) -> Result<DirEntry, IoError> {
        Err(IoError::NotFound("test".into()))
    }
}

struct NullClipboard;
impl ClipboardService for NullClipboard {
    fn read(&self) -> Option<String> {
        None
    }
    fn write(&self, _text: &str) {}
}

struct NullCommands;
impl CommandService for NullCommands {
    fn run(&self, _command: &str) -> Result<(), CommandError> {
        Ok(())
    }
}

struct NullGit;
impl GitService for NullGit {
    fn status(&self, _repo: &Path) -> Result<GitStatus, IoError> {
        Ok(GitStatus {
            branch: None,
            dirty: false,
        })
    }
    fn diff(&self, _repo: &Path, _path: Option<&Path>) -> Result<String, IoError> {
        Ok(String::new())
    }
}

struct FixedClock;
impl ClockService for FixedClock {
    fn now_monotonic(&self) -> Duration {
        Duration::from_millis(0)
    }
}

struct Harness {
    files: NullFiles,
    clipboard: NullClipboard,
    commands: NullCommands,
    git: NullGit,
    clock: FixedClock,
    theme: ChromeTheme,
}

impl Harness {
    fn new() -> Self {
        Self {
            files: NullFiles,
            clipboard: NullClipboard,
            commands: NullCommands,
            git: NullGit,
            clock: FixedClock,
            theme: ChromeTheme::default(),
        }
    }

    fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut PanelContext) -> R,
    {
        let services = Services {
            files: &self.files,
            clipboard: &self.clipboard,
            commands: &self.commands,
            git: &self.git,
            clock: &self.clock,
        };
        let mut ctx = PanelContext {
            services,
            theme: &self.theme,
            time: Duration::from_millis(0),
        };
        f(&mut ctx)
    }
}

// ─── Event helpers ────────────────────────────────────────────────────

fn press(named: NamedKey) -> UiEvent {
    UiEvent::Key(KeyDescriptor {
        physical: PhysicalKey(0),
        logical: LogicalKey::Named(named),
        state: KeyState::Pressed,
        modifiers: Modifiers::empty(),
        repeat: false,
    })
}

fn type_text(s: &str) -> UiEvent {
    UiEvent::Text(s.to_string())
}

fn sample_results() -> Vec<FinderResult> {
    vec![
        FinderResult::new("src/main.rs", "main.rs").with_subtitle("src/"),
        FinderResult::new("src/lib.rs", "lib.rs").with_subtitle("src/"),
        FinderResult::new("Cargo.toml", "Cargo.toml"),
    ]
}

// ─── Tests ────────────────────────────────────────────────────────────

#[test]
fn hidden_finder_ignores_events() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    assert!(!finder.is_visible());

    // Type while hidden -> nothing happens to query.
    harness.run(|ctx| {
        finder.handle_event(&type_text("hello"), ctx);
        finder.handle_event(&press(NamedKey::Enter), ctx);
        finder.handle_event(&press(NamedKey::ArrowDown), ctx);
    });
    assert!(finder.current_query().is_empty());
    assert!(!finder.is_visible());
    assert!(finder.pick().is_none());
}

#[test]
fn show_mode_makes_visible_and_picks_mode() {
    let mut finder = Finder::new();
    assert_eq!(finder.mode(), FinderMode::Files); // ctor default

    finder.show(FinderMode::Grep);
    assert!(finder.is_visible());
    assert_eq!(finder.mode(), FinderMode::Grep);
    assert!(finder.current_query().is_empty());
    assert!(finder.results().is_empty());

    finder.show(FinderMode::GitBranches);
    assert_eq!(finder.mode(), FinderMode::GitBranches);

    finder.show(FinderMode::Commands);
    assert_eq!(finder.mode(), FinderMode::Commands);
}

#[test]
fn show_clears_prior_query_and_results() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Files);
    finder.set_results(sample_results());

    harness.run(|ctx| {
        finder.handle_event(&type_text("main"), ctx);
    });
    assert_eq!(finder.current_query(), "main");
    assert!(!finder.results().is_empty());

    // Reopening (even in the same mode) resets state.
    finder.show(FinderMode::Files);
    assert!(finder.current_query().is_empty());
    assert!(finder.results().is_empty());
    assert_eq!(finder.selected_index(), None);
}

#[test]
fn text_and_backspace_mutate_query() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Grep);

    harness.run(|ctx| {
        finder.handle_event(&type_text("ripg"), ctx);
        finder.handle_event(&type_text("rep"), ctx);
        finder.handle_event(&press(NamedKey::Space), ctx);
        finder.handle_event(&type_text("query"), ctx);
    });
    assert_eq!(finder.current_query(), "ripgrep query");

    harness.run(|ctx| {
        // Pop "query" + the space (six pops).
        for _ in 0..6 {
            finder.handle_event(&press(NamedKey::Backspace), ctx);
        }
    });
    assert_eq!(finder.current_query(), "ripgrep");
}

#[test]
fn arrow_keys_navigate_results() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Files);
    finder.set_results(sample_results());

    // Default selection is row 0.
    assert_eq!(finder.selected_index(), Some(0));
    assert_eq!(finder.pick().unwrap().id, "src/main.rs");

    harness.run(|ctx| {
        // Up at the top clamps.
        finder.handle_event(&press(NamedKey::ArrowUp), ctx);
        assert_eq!(finder.selected_index(), Some(0));

        finder.handle_event(&press(NamedKey::ArrowDown), ctx);
        assert_eq!(finder.pick().unwrap().id, "src/lib.rs");

        finder.handle_event(&press(NamedKey::ArrowDown), ctx);
        assert_eq!(finder.pick().unwrap().id, "Cargo.toml");

        // Pile on Downs to confirm bottom clamp.
        for _ in 0..20 {
            finder.handle_event(&press(NamedKey::ArrowDown), ctx);
        }
        assert_eq!(finder.pick().unwrap().id, "Cargo.toml");
    });
}

#[test]
fn enter_hides_and_keeps_pick_for_host() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Files);
    finder.set_results(sample_results());

    harness.run(|ctx| {
        finder.handle_event(&press(NamedKey::ArrowDown), ctx);
        // Capture the pick *before* Enter, which is what the host
        // does immediately after `handle_event` returns. Conveniently
        // the panel keeps the result list intact through `hide()`
        // until the next `show()` clears it.
        let picked_id = finder.pick().map(|r| r.id.clone());
        finder.handle_event(&press(NamedKey::Enter), ctx);
        assert_eq!(picked_id.as_deref(), Some("src/lib.rs"));
    });
    assert!(!finder.is_visible());
}

#[test]
fn escape_hides() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Grep);

    harness.run(|ctx| {
        finder.handle_event(&type_text("partial query"), ctx);
        assert_eq!(finder.current_query(), "partial query");
        finder.handle_event(&press(NamedKey::Escape), ctx);
    });
    assert!(!finder.is_visible());
}

#[test]
fn set_results_replaces_and_resets_selection() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Files);
    finder.set_results(sample_results());

    harness.run(|ctx| {
        finder.handle_event(&press(NamedKey::ArrowDown), ctx);
        finder.handle_event(&press(NamedKey::ArrowDown), ctx);
    });
    assert_eq!(finder.pick().unwrap().id, "Cargo.toml");

    // Replace results — selection should clamp back to row 0 so the
    // user's next Enter lands on the top match for the new search.
    finder.set_results(vec![
        FinderResult::new("docs/README.md", "README.md"),
        FinderResult::new("docs/CHANGELOG.md", "CHANGELOG.md"),
    ]);
    assert_eq!(finder.selected_index(), Some(0));
    assert_eq!(finder.pick().unwrap().id, "docs/README.md");
}

#[test]
fn service_reply_replaces_results() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Grep);

    // Host saw `IoError::Pending(req_id)` and wired it.
    let req_id = 42_u64;
    finder.set_pending_request(req_id);
    assert_eq!(finder.pending_request(), Some(req_id));

    // Reply with a wrong id is ignored — pending stays set, no
    // results adopted.
    let stray_payload = serde_json::json!([
        { "id": "branch/should-not-appear", "label": "nope" }
    ]);
    harness.run(|ctx| {
        finder.handle_event(
            &UiEvent::ServiceReply {
                request_id: req_id + 1,
                payload: stray_payload,
            },
            ctx,
        );
    });
    assert_eq!(finder.pending_request(), Some(req_id));
    assert!(finder.results().is_empty());

    // Matching reply -> adopt payload, clear pending.
    let payload = serde_json::json!([
        { "id": "branch/main", "label": "main", "subtitle": "HEAD" },
        { "id": "branch/dev", "label": "dev", "subtitle": null }
    ]);
    harness.run(|ctx| {
        finder.handle_event(
            &UiEvent::ServiceReply {
                request_id: req_id,
                payload,
            },
            ctx,
        );
    });
    assert_eq!(finder.pending_request(), None);
    assert_eq!(finder.results().len(), 2);
    assert_eq!(finder.results()[0].id, "branch/main");
    assert_eq!(finder.results()[0].subtitle.as_deref(), Some("HEAD"));
    assert_eq!(finder.results()[1].id, "branch/dev");
    assert_eq!(finder.results()[1].subtitle, None);
    assert_eq!(finder.selected_index(), Some(0));
}

#[test]
fn service_reply_with_malformed_payload_clears_pending() {
    let harness = Harness::new();
    let mut finder = Finder::new();
    finder.show(FinderMode::Files);
    finder.set_pending_request(7);
    finder.set_results(sample_results());

    // Payload isn't a Vec<FinderResult>. Result list stays put;
    // pending clears so the host can issue a new request.
    let bad_payload = serde_json::json!({ "error": "rg crashed" });
    harness.run(|ctx| {
        finder.handle_event(
            &UiEvent::ServiceReply {
                request_id: 7,
                payload: bad_payload,
            },
            ctx,
        );
    });
    assert_eq!(finder.pending_request(), None);
    assert_eq!(finder.results().len(), 3); // unchanged
}

#[test]
fn wants_focus_tracks_visibility() {
    let mut finder = Finder::new();
    assert!(!finder.wants_focus());
    finder.show(FinderMode::Files);
    assert!(finder.wants_focus());
    finder.hide();
    assert!(!finder.wants_focus());
}

#[test]
fn name_is_stable_identifier() {
    let finder = Finder::new();
    assert_eq!(finder.name(), "finder");
}

#[test]
fn mode_badge_strings() {
    // The badge labels are the contract the host renders next to the
    // query input; keep them locked down.
    assert_eq!(FinderMode::Files.badge(), "FILES");
    assert_eq!(FinderMode::Grep.badge(), "GREP");
    assert_eq!(FinderMode::GitBranches.badge(), "BRANCH");
    assert_eq!(FinderMode::Commands.badge(), "CMD");
}
