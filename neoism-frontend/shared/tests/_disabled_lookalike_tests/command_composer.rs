//! Behavior tests for the slim command composer.
//!
//! Covers (per the migration spec):
//! - `hidden_composer_ignores_events`
//! - `enter_submits_through_command_service`
//! - `escape_hides`
//! - `text_event_inserts_at_cursor`
//! - `arrow_up_browses_history`
//! - `backspace_deletes_byte_before_cursor`
//!
//! Plus a few invariants we want to keep stable as the panel evolves:
//! - Empty-query submit is a no-op-with-hide.
//! - `Pending(req_id)` from the command service is captured.
//! - History entries dedupe on consecutive submits.
//! - Multi-byte UTF-8 backspace deletes the whole codepoint.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use neoism_ui::event::{
    KeyDescriptor, KeyState, LogicalKey, Modifiers, NamedKey, PhysicalKey, UiEvent,
};
use neoism_ui::panels::{CommandComposer, Panel, PanelContext};
use neoism_ui::services::{
    ClipboardService, ClockService, CommandError, CommandService, DirEntry, FilesService,
    GitService, GitStatus, IoError, RequestId, Services,
};
use neoism_ui::theme::ChromeTheme;

// ── Mock services ──────────────────────────────────────────────────

/// Records every `run` invocation. Tests assert against `runs()`.
#[derive(Default)]
struct RecordingCommands {
    runs: Mutex<Vec<String>>,
    /// Optional canned return for the next `run` call. Lets a single
    /// test exercise the `Pending(req_id)` path.
    next_result: Mutex<Option<Result<(), CommandError>>>,
}

impl RecordingCommands {
    fn set_next_result(&self, r: Result<(), CommandError>) {
        *self.next_result.lock().unwrap() = Some(r);
    }
}

impl CommandService for RecordingCommands {
    fn run(&self, command: &str) -> Result<(), CommandError> {
        self.runs.lock().unwrap().push(command.to_string());
        if let Some(canned) = self.next_result.lock().unwrap().take() {
            canned
        } else {
            Ok(())
        }
    }
}

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
    commands: RecordingCommands,
    files: NullFiles,
    clipboard: NullClipboard,
    git: NullGit,
    clock: FixedClock,
    theme: ChromeTheme,
}

impl Harness {
    fn new() -> Self {
        Self {
            commands: RecordingCommands::default(),
            files: NullFiles,
            clipboard: NullClipboard,
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

    fn runs(&self) -> Vec<String> {
        self.commands.runs.lock().unwrap().clone()
    }
}

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

// ── Tests ──────────────────────────────────────────────────────────

#[test]
fn hidden_composer_ignores_events() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    assert!(!composer.is_visible());

    harness.run(|ctx| {
        composer.handle_event(&type_text("ls"), ctx);
        composer.handle_event(&press(NamedKey::Enter), ctx);
        composer.handle_event(&press(NamedKey::Backspace), ctx);
        composer.handle_event(&press(NamedKey::ArrowUp), ctx);
    });

    assert!(composer.query().is_empty());
    assert_eq!(composer.cursor(), 0);
    assert!(harness.runs().is_empty());
    assert!(composer.history().is_empty());
}

#[test]
fn escape_hides() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&type_text("ls -la"), ctx);
        composer.handle_event(&press(NamedKey::Escape), ctx);
    });

    assert!(!composer.is_visible());
    // Escape preserves the in-progress text — re-showing should
    // resurface it untouched.
    assert_eq!(composer.query(), "ls -la");
    assert!(harness.runs().is_empty());
}

#[test]
fn enter_submits_through_command_service() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&type_text("git status"), ctx);
        composer.handle_event(&press(NamedKey::Enter), ctx);
    });

    assert_eq!(harness.runs(), vec!["git status".to_string()]);
    // Submit clears the input and hides.
    assert!(!composer.is_visible());
    assert!(composer.query().is_empty());
    assert_eq!(composer.cursor(), 0);
    // …and pushes to history.
    assert_eq!(composer.history(), &["git status".to_string()]);
}

#[test]
fn text_event_inserts_at_cursor() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&type_text("ls"), ctx);
    });
    assert_eq!(composer.query(), "ls");
    assert_eq!(composer.cursor(), 2);

    // Append more — caret stays at end.
    harness.run(|ctx| {
        composer.handle_event(&type_text(" -la"), ctx);
    });
    assert_eq!(composer.query(), "ls -la");
    assert_eq!(composer.cursor(), 6);

    // Multi-byte codepoints land intact, caret advances by byte len.
    harness.run(|ctx| {
        composer.handle_event(&type_text(" café"), ctx);
    });
    assert_eq!(composer.query(), "ls -la café");
    // " café" = 1 + 3 + 2 = 6 bytes appended.
    assert_eq!(composer.cursor(), 12);
}

#[test]
fn arrow_up_browses_history() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    // Seed three submits.
    for cmd in ["ls", "cd /tmp", "echo hi"] {
        harness.run(|ctx| {
            composer.show();
            composer.handle_event(&type_text(cmd), ctx);
            composer.handle_event(&press(NamedKey::Enter), ctx);
        });
    }
    assert_eq!(
        composer.history(),
        &[
            "ls".to_string(),
            "cd /tmp".to_string(),
            "echo hi".to_string(),
        ]
    );

    composer.show();
    harness.run(|ctx| {
        // Browse backward: newest first.
        composer.handle_event(&press(NamedKey::ArrowUp), ctx);
        assert_eq!(composer.query(), "echo hi");
        assert!(composer.is_browsing_history());

        composer.handle_event(&press(NamedKey::ArrowUp), ctx);
        assert_eq!(composer.query(), "cd /tmp");

        composer.handle_event(&press(NamedKey::ArrowUp), ctx);
        assert_eq!(composer.query(), "ls");

        // Bottomed out — further Up is a no-op.
        composer.handle_event(&press(NamedKey::ArrowUp), ctx);
        assert_eq!(composer.query(), "ls");

        // Walk forward.
        composer.handle_event(&press(NamedKey::ArrowDown), ctx);
        assert_eq!(composer.query(), "cd /tmp");

        // Past the newest — restore draft (empty, since we never typed
        // anything before pressing ArrowUp).
        composer.handle_event(&press(NamedKey::ArrowDown), ctx);
        composer.handle_event(&press(NamedKey::ArrowDown), ctx);
        assert_eq!(composer.query(), "");
        assert!(!composer.is_browsing_history());
    });
}

#[test]
fn arrow_up_preserves_in_flight_draft() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    // Seed history.
    harness.run(|ctx| {
        composer.handle_event(&type_text("ls"), ctx);
        composer.handle_event(&press(NamedKey::Enter), ctx);
    });
    composer.show();

    // Start typing a fresh draft.
    harness.run(|ctx| {
        composer.handle_event(&type_text("echo wip"), ctx);
        assert_eq!(composer.query(), "echo wip");

        // Browse into history — draft gets stashed.
        composer.handle_event(&press(NamedKey::ArrowUp), ctx);
        assert_eq!(composer.query(), "ls");

        // Walk back to the draft.
        composer.handle_event(&press(NamedKey::ArrowDown), ctx);
        assert_eq!(composer.query(), "echo wip");
        assert!(!composer.is_browsing_history());
    });
}

#[test]
fn backspace_deletes_byte_before_cursor() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&type_text("hello"), ctx);
        assert_eq!(composer.query(), "hello");
        assert_eq!(composer.cursor(), 5);

        composer.handle_event(&press(NamedKey::Backspace), ctx);
        assert_eq!(composer.query(), "hell");
        assert_eq!(composer.cursor(), 4);

        composer.handle_event(&press(NamedKey::Backspace), ctx);
        composer.handle_event(&press(NamedKey::Backspace), ctx);
        assert_eq!(composer.query(), "he");
        assert_eq!(composer.cursor(), 2);
    });

    // Backspace at byte 0 is a no-op.
    composer.clear();
    composer.show();
    harness.run(|ctx| {
        composer.handle_event(&press(NamedKey::Backspace), ctx);
    });
    assert_eq!(composer.query(), "");
    assert_eq!(composer.cursor(), 0);
}

#[test]
fn backspace_handles_multibyte_codepoint() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&type_text("café"), ctx);
    });
    // "café" = 5 bytes (c=1, a=1, f=1, é=2).
    assert_eq!(composer.query().len(), 5);
    assert_eq!(composer.cursor(), 5);

    harness.run(|ctx| {
        composer.handle_event(&press(NamedKey::Backspace), ctx);
    });
    // The two-byte `é` should pop atomically.
    assert_eq!(composer.query(), "caf");
    assert_eq!(composer.cursor(), 3);
}

#[test]
fn empty_enter_hides_without_dispatch() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&press(NamedKey::Enter), ctx);
    });

    assert!(!composer.is_visible());
    assert!(harness.runs().is_empty());
    assert!(composer.history().is_empty());
}

#[test]
fn whitespace_only_enter_does_not_push_history() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&type_text("   "), ctx);
        composer.handle_event(&press(NamedKey::Enter), ctx);
    });

    // The query was trimmed-empty, so no dispatch and no history entry.
    assert!(harness.runs().is_empty());
    assert!(composer.history().is_empty());
}

#[test]
fn pending_request_id_is_captured() {
    let harness = Harness::new();
    harness
        .commands
        .set_next_result(Err(CommandError::Pending(42 as RequestId)));

    let mut composer = CommandComposer::new();
    composer.show();

    harness.run(|ctx| {
        composer.handle_event(&type_text("build"), ctx);
        composer.handle_event(&press(NamedKey::Enter), ctx);
    });

    assert_eq!(composer.pending_request(), Some(42));

    // A matching ServiceReply clears the pending marker.
    harness.run(|ctx| {
        composer.show(); // panel hides on submit; show again to receive reply
        composer.handle_event(
            &UiEvent::ServiceReply {
                request_id: 42,
                payload: serde_json::Value::Null,
            },
            ctx,
        );
    });
    assert!(composer.pending_request().is_none());
}

#[test]
fn history_dedupes_consecutive_repeats() {
    let harness = Harness::new();
    let mut composer = CommandComposer::new();

    for _ in 0..3 {
        composer.show();
        harness.run(|ctx| {
            composer.handle_event(&type_text("ls"), ctx);
            composer.handle_event(&press(NamedKey::Enter), ctx);
        });
    }

    // All three submits dispatched, but only one history entry.
    assert_eq!(harness.runs().len(), 3);
    assert_eq!(composer.history(), &["ls".to_string()]);
}

#[test]
fn push_history_seed_from_host() {
    let mut composer = CommandComposer::new();
    composer.push_history("ls".into());
    composer.push_history("cd /tmp".into());
    composer.push_history("".into()); // blanks skipped
    composer.push_history("cd /tmp".into()); // consecutive dup skipped

    assert_eq!(
        composer.history(),
        &["ls".to_string(), "cd /tmp".to_string()]
    );
}

#[test]
fn wants_focus_tracks_visibility() {
    let mut composer = CommandComposer::new();
    assert!(!composer.wants_focus());
    composer.show();
    assert!(composer.wants_focus());
    composer.hide();
    assert!(!composer.wants_focus());
}

#[test]
fn name_is_stable_identifier() {
    let composer = CommandComposer::new();
    assert_eq!(composer.name(), "command_composer");
}

#[test]
fn set_query_replaces_text_and_caret() {
    let mut composer = CommandComposer::new();
    composer.set_query("foo bar");
    assert_eq!(composer.query(), "foo bar");
    assert_eq!(composer.cursor(), 7);
}
