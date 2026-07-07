//! Behavior tests for the cross-platform `Chrome` assembly.
//!
//! These tests exercise the seam between `Chrome` and the panels it
//! owns:
//!
//! - Construction lands every panel in its documented default state.
//! - `set_layout` distributes a window viewport into non-overlapping
//!   strip / sidebar / terminal rects, and (when overlays are
//!   visible) places modal rects centered inside the viewport.
//! - Event dispatch routes typed events to the focused panel.
//! - Visible modal overlays take priority over the focused background
//!   panel for keyboard-shaped events ("modals swallow the keyboard").
//! - `event_priority_order` exposes a stable resolution ordering the
//!   host can inspect (used by the dispatch path and tested directly
//!   here).
//!
//! The chrome's draw path is not exercised: `sugarloaf::Sugarloaf` has
//! no test double, so `Chrome::draw` is left to the integration build
//! gate.

use std::path::Path;
use std::time::Duration;

use neoism_ui::chrome::{Chrome, PanelKey};
use neoism_ui::event::{
    KeyDescriptor, KeyState, LogicalKey, Modifiers, NamedKey, PhysicalKey, UiEvent,
};
use neoism_ui::layout::Rect;
use neoism_ui::services::{
    ClipboardService, ClockService, CommandError, CommandService, DirEntry, FilesService,
    GitService, GitStatus, IoError, Services,
};

// ─── Null service stubs (same pattern as the per-panel tests) ────────

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
}

impl Harness {
    fn new() -> Self {
        Self {
            files: NullFiles,
            clipboard: NullClipboard,
            commands: NullCommands,
            git: NullGit,
            clock: FixedClock,
        }
    }

    fn services(&self) -> Services<'_> {
        Services {
            files: &self.files,
            clipboard: &self.clipboard,
            commands: &self.commands,
            git: &self.git,
            clock: &self.clock,
        }
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

fn text(s: &str) -> UiEvent {
    UiEvent::Text(s.into())
}

/// Chrome alias used throughout these tests. The unit type `()` is a
/// valid `Send + 'static` choice for the agent label — the tests never
/// populate the buffer-tabs strip so the `AgentLabel + Copy + PartialEq`
/// bound that other `BufferTabs<A>` methods need is irrelevant here.
type TestChrome = Chrome<()>;

// ─── Construction ─────────────────────────────────────────────────────

#[test]
fn chrome_constructs_with_all_panels() {
    let chrome = TestChrome::new();

    // Modals start hidden so the user opens onto a quiet workspace.
    assert!(!chrome.command_palette.is_visible());
    assert!(!chrome.finder.is_visible());
    assert!(!chrome.git_diff.is_visible());
    assert!(!chrome.command_composer.is_visible());

    // FileTree is opt-in — `Chrome::new` does not install one.
    assert!(chrome.file_tree.is_none());

    // BufferTabs starts with no tabs and `visible == false` per the
    // panel's own defaults. We just probe that the field is reachable.
    assert_eq!(chrome.buffer_tabs.tabs.len(), 0);

    // StatusLine ships visible per its own default.
    assert!(chrome.status_line.is_visible());

    // No focus stack entries yet.
    assert!(chrome.focused().is_none());
}

#[test]
fn default_construction_matches_new() {
    let a: TestChrome = Default::default();
    let b = TestChrome::new();
    // Cheap structural check: layout rects start zero-sized in both.
    assert_eq!(a.layout().buffer_tabs, b.layout().buffer_tabs);
    assert_eq!(a.layout().status_line, b.layout().status_line);
    assert_eq!(a.layout().terminal, b.layout().terminal);
}

// ─── Layout ───────────────────────────────────────────────────────────

#[test]
fn chrome_set_layout_distributes_rects() {
    let mut chrome = TestChrome::new();
    let viewport = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    chrome.set_layout(viewport);

    let layout = chrome.layout().clone();

    // Top strip and bottom strip span the full viewport width.
    assert_eq!(layout.buffer_tabs.x, viewport.x);
    assert_eq!(layout.buffer_tabs.w, viewport.w);
    assert_eq!(layout.status_line.x, viewport.x);
    assert_eq!(layout.status_line.w, viewport.w);

    // The two strips do not vertically overlap each other.
    assert!(layout.buffer_tabs.y + layout.buffer_tabs.h <= layout.status_line.y);

    // The terminal rect sits strictly between the two strips
    // vertically.
    assert!(layout.terminal.y >= layout.buffer_tabs.y + layout.buffer_tabs.h);
    assert!(layout.terminal.y + layout.terminal.h <= layout.status_line.y);

    // Without a file tree installed, the terminal eats the whole
    // horizontal slice.
    assert!(layout.file_tree.is_none());
    assert_eq!(layout.terminal.x, viewport.x);
    assert_eq!(layout.terminal.w, viewport.w);

    // Hidden modals leave their rects unassigned so the host can tell
    // "modal off" from "modal sized but not painted".
    assert!(layout.command_palette.is_none());
    assert!(layout.finder.is_none());
    assert!(layout.git_diff.is_none());
    assert!(layout.command_composer.is_none());
}

#[test]
fn chrome_layout_with_file_tree_shrinks_terminal() {
    use neoism_ui::panels::FileTree;
    use std::path::PathBuf;

    let mut chrome = TestChrome::new();
    chrome.install_file_tree(FileTree::new(PathBuf::from("/")));
    let viewport = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    chrome.set_layout(viewport);

    let layout = chrome.layout().clone();
    let ft = layout.file_tree.expect("file tree rect missing");

    // Sidebar pinned to the left, default width honored.
    assert_eq!(ft.x, viewport.x);
    assert_eq!(ft.w, neoism_ui::chrome::DEFAULT_FILE_TREE_WIDTH);

    // Terminal starts at the sidebar's right edge.
    assert_eq!(layout.terminal.x, ft.x + ft.w);
    assert_eq!(layout.terminal.w, viewport.w - ft.w);

    // Sidebar doesn't bleed into either strip vertically.
    assert!(ft.y >= layout.buffer_tabs.y + layout.buffer_tabs.h);
    assert!(ft.y + ft.h <= layout.status_line.y);
}

#[test]
fn chrome_layout_assigns_modal_rects_when_visible() {
    let mut chrome = TestChrome::new();
    chrome.command_palette.show();
    chrome.finder.show(neoism_ui::panels::FinderMode::Files);
    chrome.git_diff.show();
    chrome.command_composer.show();

    let viewport = Rect::new(0.0, 0.0, 1280.0, 800.0);
    chrome.set_layout(viewport);
    let layout = chrome.layout().clone();

    let palette = layout.command_palette.expect("palette rect missing");
    let finder = layout.finder.expect("finder rect missing");
    let diff = layout.git_diff.expect("git_diff rect missing");
    let composer = layout.command_composer.expect("composer rect missing");

    // Modals are horizontally centered inside the viewport.
    let palette_center = palette.x + palette.w * 0.5;
    let finder_center = finder.x + finder.w * 0.5;
    let viewport_center = viewport.x + viewport.w * 0.5;
    assert!((palette_center - viewport_center).abs() < 0.5);
    assert!((finder_center - viewport_center).abs() < 0.5);

    // Modals fit inside the viewport.
    assert!(palette.x >= viewport.x);
    assert!(palette.x + palette.w <= viewport.x + viewport.w);
    assert!(finder.x >= viewport.x);
    assert!(finder.x + finder.w <= viewport.x + viewport.w);

    // Git diff overlays the whole window.
    assert_eq!(diff, viewport);

    // Composer is the full-width sticky bar above the status line.
    assert_eq!(composer.x, viewport.x);
    assert_eq!(composer.w, viewport.w);
    assert!(composer.y + composer.h <= layout.status_line.y + 0.5);
}

// ─── Event dispatch ───────────────────────────────────────────────────

#[test]
fn chrome_dispatches_to_focused_panel() {
    // With the palette open, typing should grow its filter query.
    let mut chrome = TestChrome::new();
    chrome.command_palette.show();
    chrome.focus(PanelKey::CommandPalette);
    chrome.set_layout(Rect::new(0.0, 0.0, 1280.0, 800.0));

    let harness = Harness::new();
    chrome.handle_event(&text("ne"), harness.services(), Duration::ZERO);
    chrome.handle_event(&text("o"), harness.services(), Duration::ZERO);

    assert_eq!(chrome.command_palette.query(), "neo");
}

#[test]
fn modal_palette_takes_priority_over_focus_stack() {
    // FileTree is focused, but the visible palette must still get
    // Escape first and consume it.
    use neoism_ui::panels::FileTree;
    use std::path::PathBuf;

    let mut chrome = TestChrome::new();
    chrome.install_file_tree(FileTree::new(PathBuf::from("/")));
    chrome.focus(PanelKey::FileTree);
    chrome.command_palette.show();
    chrome.set_layout(Rect::new(0.0, 0.0, 1280.0, 800.0));

    assert!(chrome.command_palette.is_visible());
    let harness = Harness::new();
    chrome.handle_event(&press(NamedKey::Escape), harness.services(), Duration::ZERO);

    // Palette consumes Escape → hides itself. FileTree never sees it.
    assert!(!chrome.command_palette.is_visible());
}

#[test]
fn event_priority_visible_modals_first() {
    let mut chrome = TestChrome::new();
    chrome.focus(PanelKey::BufferTabs);

    // Nothing modal visible: focus-stack-top leads.
    let order = chrome.event_priority_order(&press(NamedKey::Enter));
    assert_eq!(order.first().copied(), Some(PanelKey::BufferTabs));

    // Showing the palette pushes it to the front of the dispatch.
    chrome.command_palette.show();
    let order = chrome.event_priority_order(&press(NamedKey::Enter));
    assert_eq!(order.first().copied(), Some(PanelKey::CommandPalette));

    // Adding the finder leaves the palette first (palette beats
    // finder in the modal priority chain).
    chrome.finder.show(neoism_ui::panels::FinderMode::Files);
    let order = chrome.event_priority_order(&press(NamedKey::Enter));
    assert_eq!(order.first().copied(), Some(PanelKey::CommandPalette));
    assert!(order.contains(&PanelKey::Finder));

    // Background panels are always represented somewhere in the
    // order so non-modal events still fan out.
    assert!(order.contains(&PanelKey::StatusLine));
    assert!(order.contains(&PanelKey::BufferTabs));
}

#[test]
fn focus_stack_is_lifo_with_dedup() {
    let mut chrome = TestChrome::new();

    chrome.focus(PanelKey::StatusLine);
    chrome.focus(PanelKey::BufferTabs);
    assert_eq!(chrome.focused(), Some(PanelKey::BufferTabs));

    // Re-focusing the top is a no-op.
    chrome.focus(PanelKey::BufferTabs);
    assert_eq!(chrome.focused(), Some(PanelKey::BufferTabs));

    // Re-focusing a buried key lifts it to the top.
    chrome.focus(PanelKey::StatusLine);
    assert_eq!(chrome.focused(), Some(PanelKey::StatusLine));

    assert_eq!(chrome.pop_focus(), Some(PanelKey::StatusLine));
    assert_eq!(chrome.focused(), Some(PanelKey::BufferTabs));
    assert_eq!(chrome.pop_focus(), Some(PanelKey::BufferTabs));
    assert!(chrome.focused().is_none());
    // Empty pop is None, not a panic.
    assert!(chrome.pop_focus().is_none());
}

#[test]
fn theme_swap_is_observable() {
    use neoism_ui::theme::ChromeTheme;

    let mut chrome = TestChrome::new();
    let before = chrome.theme().clone();
    let mut next = before.clone();
    next.accent = neoism_ui::RgbTriple {
        r: 0xff,
        g: 0x00,
        b: 0x00,
    };
    chrome.set_theme(next.clone());
    assert_eq!(chrome.theme(), &next);
    // Sanity: the dark default is also still constructible.
    let _ = ChromeTheme::dark_default();
}
