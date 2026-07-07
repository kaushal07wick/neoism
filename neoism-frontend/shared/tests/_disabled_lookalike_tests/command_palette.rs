//! Behavior tests for the slim command palette.
//!
//! Covers:
//! - Initial visibility (hidden, no commands).
//! - `show` / `hide` lifecycle.
//! - `set_commands` populates the filter.
//! - Substring narrowing via `Text` events.
//! - Backspace shrinks the query and re-runs the filter.
//! - Arrow keys clamp selection at both ends.
//! - `Enter` dispatches the selected command id via `CommandService`
//!   and hides the palette.
//! - `Escape` hides without dispatching.
//! - `Enter` with no filtered rows is a no-op-with-hide (still
//!   closes; nothing dispatched).
//! - Resetting `set_commands` clamps selection back into range.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use neoism_ui::event::{
    KeyDescriptor, KeyState, LogicalKey, Modifiers, NamedKey, PhysicalKey, PointerButton,
    UiEvent,
};
use neoism_ui::layout::{PanelLayout, Rect};
use neoism_ui::panels::{CommandEntry, CommandPalette, Panel, PanelContext};
use neoism_ui::services::{
    ClipboardService, ClockService, CommandError, CommandService, DirEntry, FilesService,
    GitService, GitStatus, IoError, Services,
};
use neoism_ui::theme::ChromeTheme;

/// Records every `run` invocation in order. Tests assert on this
/// vector to confirm dispatching landed.
#[derive(Default)]
struct RecordingCommands {
    runs: Mutex<Vec<String>>,
}

impl CommandService for RecordingCommands {
    fn run(&self, command: &str) -> Result<(), CommandError> {
        self.runs.lock().unwrap().push(command.to_string());
        Ok(())
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

/// Owns the harness services so a single `with_palette` closure can
/// borrow a `PanelContext` against them. `RefCell` only — tests are
/// single-threaded.
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

fn sample_commands() -> Vec<CommandEntry> {
    vec![
        CommandEntry::new("file.open", "Open File").with_keybinding("Ctrl+O"),
        CommandEntry::new("file.save", "Save Document").with_keybinding("Ctrl+S"),
        CommandEntry::new("tab.new", "New Tab").with_keybinding("Ctrl+T"),
        CommandEntry::new("tab.close", "Close Tab").with_keybinding("Ctrl+W"),
        CommandEntry::new("theme.toggle", "Toggle Theme"),
    ]
}

#[test]
fn starts_hidden_with_no_commands() {
    let palette = CommandPalette::new();
    assert!(!palette.is_visible());
    assert_eq!(palette.commands().len(), 0);
    assert_eq!(palette.filtered_len(), 0);
    assert!(palette.selected_command().is_none());
}

#[test]
fn show_then_hide_toggles_visibility() {
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    assert!(!palette.is_visible());

    palette.show();
    assert!(palette.is_visible());
    assert_eq!(palette.filtered_len(), 5);

    palette.hide();
    assert!(!palette.is_visible());
}

#[test]
fn ignores_events_while_hidden() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());

    // Type while hidden -> nothing happens.
    harness.run(|ctx| {
        palette.handle_event(&type_text("save"), ctx);
        palette.handle_event(&press(NamedKey::Enter), ctx);
    });
    assert!(palette.query().is_empty());
    assert_eq!(harness.runs().len(), 0);
}

#[test]
fn text_narrows_filtered_list() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        palette.handle_event(&type_text("tab"), ctx);
    });

    assert_eq!(palette.query(), "tab");
    // "New Tab" + "Close Tab" both match.
    assert_eq!(palette.filtered_len(), 2);
    let chosen = palette.selected_command().expect("selected");
    assert_eq!(chosen.id, "tab.new");
}

#[test]
fn backspace_shrinks_query() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        palette.handle_event(&type_text("tabs"), ctx);
        // "tabs" matches nothing in our catalog.
        assert_eq!(palette.filtered_len(), 0);

        palette.handle_event(&press(NamedKey::Backspace), ctx);
    });
    assert_eq!(palette.query(), "tab");
    assert_eq!(palette.filtered_len(), 2);
}

#[test]
fn arrow_keys_clamp_selection() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        // Up at top is a no-op.
        palette.handle_event(&press(NamedKey::ArrowUp), ctx);
        assert_eq!(palette.selected_command().unwrap().id, "file.open");

        palette.handle_event(&press(NamedKey::ArrowDown), ctx);
        palette.handle_event(&press(NamedKey::ArrowDown), ctx);
        assert_eq!(palette.selected_command().unwrap().id, "tab.new");

        // Pile on Downs to confirm clamp.
        for _ in 0..20 {
            palette.handle_event(&press(NamedKey::ArrowDown), ctx);
        }
        assert_eq!(palette.selected_command().unwrap().id, "theme.toggle");
    });
}

#[test]
fn enter_dispatches_selected_and_hides() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        palette.handle_event(&press(NamedKey::ArrowDown), ctx);
        palette.handle_event(&press(NamedKey::Enter), ctx);
    });

    assert!(!palette.is_visible(), "palette should hide on enter");
    assert_eq!(harness.runs(), vec!["file.save".to_string()]);
}

#[test]
fn enter_with_filtered_match_dispatches_filtered_id() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        palette.handle_event(&type_text("close"), ctx);
        palette.handle_event(&press(NamedKey::Enter), ctx);
    });

    assert_eq!(harness.runs(), vec!["tab.close".to_string()]);
}

#[test]
fn escape_hides_without_dispatch() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        palette.handle_event(&press(NamedKey::ArrowDown), ctx);
        palette.handle_event(&press(NamedKey::Escape), ctx);
    });

    assert!(!palette.is_visible());
    assert!(harness.runs().is_empty());
}

#[test]
fn enter_with_no_matches_does_not_dispatch() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        palette.handle_event(&type_text("xyz"), ctx);
        assert_eq!(palette.filtered_len(), 0);
        palette.handle_event(&press(NamedKey::Enter), ctx);
    });

    // Enter on an empty list still closes the palette (matches the
    // user expectation that hitting enter dismisses an empty modal)
    // but nothing should have been dispatched.
    assert!(!palette.is_visible());
    assert!(harness.runs().is_empty());
}

#[test]
fn set_commands_clamps_selection() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        // Move to the last row.
        for _ in 0..10 {
            palette.handle_event(&press(NamedKey::ArrowDown), ctx);
        }
    });
    assert_eq!(palette.selected_command().unwrap().id, "theme.toggle");

    // Replace the catalog with a shorter one. Selection should clamp
    // rather than panic or point past the end.
    palette.set_commands(vec![
        CommandEntry::new("only.one", "Only One"),
        CommandEntry::new("only.two", "Only Two"),
    ]);
    assert!(palette.selected_command().is_some());
    let sel = palette.selected_command().unwrap();
    assert!(sel.id == "only.one" || sel.id == "only.two");
    // The default reset puts us at row 0.
    assert_eq!(sel.id, "only.one");
}

#[test]
fn space_key_extends_query() {
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    harness.run(|ctx| {
        // Some hosts deliver Space as a NamedKey rather than as
        // `Text(" ")`; the palette should still produce a literal
        // space in the query.
        palette.handle_event(&type_text("new"), ctx);
        palette.handle_event(&press(NamedKey::Space), ctx);
        palette.handle_event(&type_text("tab"), ctx);
    });

    assert_eq!(palette.query(), "new tab");
    assert_eq!(palette.filtered_len(), 1);
    assert_eq!(palette.selected_command().unwrap().id, "tab.new");
}

#[test]
fn wants_focus_tracks_visibility() {
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    assert!(!palette.wants_focus());
    palette.show();
    assert!(palette.wants_focus());
    palette.hide();
    assert!(!palette.wants_focus());
}

#[test]
fn name_is_stable_identifier() {
    let palette = CommandPalette::new();
    assert_eq!(palette.name(), "command_palette");
}

#[test]
fn pointer_before_first_draw_is_ignored() {
    // Sanity: a click that arrives before any draw should not panic
    // and should not dispatch (because there are no row rects yet).
    let harness = Harness::new();
    let mut palette = CommandPalette::new();
    palette.set_commands(sample_commands());
    palette.show();

    let _layout = PanelLayout {
        bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
        scale: 1.0,
    };

    harness.run(|ctx| {
        // Right inside where row 0 would land, but no draw has run.
        palette.handle_event(
            &UiEvent::PointerDown {
                button: PointerButton::Left,
                x: 400.0,
                y: 130.0,
                modifiers: Modifiers::empty(),
                click_count: 1,
            },
            ctx,
        );
    });

    assert!(harness.runs().is_empty());
    assert!(palette.is_visible());
}

// `RefCell` is included to make extending the harness easy in
// follow-up tests; silence the unused-import lint while it is dormant.
#[allow(dead_code)]
fn _hold_refcell() -> RefCell<u8> {
    RefCell::new(0)
}
