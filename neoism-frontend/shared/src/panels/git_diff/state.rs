use std::path::Path;
use std::sync::{Arc, Mutex};

use web_time::Instant;

use crate::animation::CriticallyDampedSpring;

use super::types::{FileChange, PanelData, Rect};
use super::PANEL_DEFAULT_WIDTH;

/// Native IO surface the desktop fork plugs in so the shared panel
/// can fetch `git status` without owning a `std::process::Command`
/// dependency (which would lock the shared crate out of wasm). The
/// desktop installs an implementation backed by
/// `frontends/neoism/src/editor/git_diff_panel/io.rs`; the wasm
/// build leaves it `None` and the daemon pushes data directly into
/// the panel's `Arc<Mutex<PanelData>>` instead.
pub trait GitDiffIo: Send + Sync {
    /// Run `git status` + `git diff --numstat` for `repo_root` and
    /// return the changed-file list. Called from a background thread.
    fn collect_files(&self, repo_root: &Path) -> Vec<FileChange>;
}

pub struct GitDiffPanel {
    pub(super) visible: bool,
    pub(super) focused: bool,
    pub(super) scale: f32,
    pub(super) open_started_at: Option<Instant>,
    /// Current panel width in logical pixels. Resizable via mouse drag
    /// on the leading edge or via Alt+Ctrl arrow keys, same UX as
    /// `file_tree::resize`. Persists across hide/show.
    pub(super) width: f32,
    /// Index of the selected file row in the top "Files" card. Up/Down
    /// arrows move this; the bottom diff card always shows whichever
    /// file this points at.
    pub(super) selected: usize,

    /// Spring-damped vertical scroll for the file list (logical px).
    pub(super) file_scroll: f32,
    pub(super) file_scroll_spring: CriticallyDampedSpring,
    pub(super) last_file_scroll_frame: Instant,
    pub(super) file_wheel_acc: f32,

    /// Spring-damped vertical scroll for the diff card body.
    pub(super) diff_scroll: f32,
    pub(super) diff_scroll_spring: CriticallyDampedSpring,
    pub(super) last_diff_scroll_frame: Instant,
    pub(super) diff_wheel_acc: f32,

    pub(super) data: Arc<Mutex<PanelData>>,
    pub(super) panel_rect: Rect,
    pub(super) close_rect: Rect,
    pub(super) files_card_rect: Rect,
    pub(super) files_body_rect: Rect,
    pub(super) diff_card_rect: Rect,
    /// Hit-test rects for each file row — populated by `render`,
    /// consumed by `hit_test` so a click selects a row.
    pub(super) file_row_rects: Vec<(usize, Rect)>,
    /// Files-card scrollbar thumb rect (window-logical). `Rect::ZERO`
    /// when the list fits without scrolling. Used for grab-and-drag.
    pub(super) files_scrollbar_thumb_rect: Rect,
    /// Diff-card scrollbar thumb rect.
    pub(super) diff_scrollbar_thumb_rect: Rect,
    /// Cursor caret rect (window-logical) for the selected row when
    /// the panel has keyboard focus. Drives the trail-cursor animation
    /// in the screen layer, same path as the file_tree's caret jump.
    pub(super) selected_cursor_rect: Option<[f32; 4]>,
    /// Native IO provider injected by the desktop fork. `None` on
    /// wasm (and in the slim-only default), in which case `refresh`
    /// becomes a no-op for the file-list and the host is expected to
    /// populate `data` directly via the daemon's push path.
    pub(super) io: Option<Arc<dyn GitDiffIo>>,
}

impl Default for GitDiffPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl GitDiffPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            focused: false,
            scale: 1.0,
            open_started_at: None,
            width: PANEL_DEFAULT_WIDTH,
            selected: 0,
            file_scroll: 0.0,
            file_scroll_spring: CriticallyDampedSpring::new(),
            last_file_scroll_frame: Instant::now(),
            file_wheel_acc: 0.0,
            diff_scroll: 0.0,
            diff_scroll_spring: CriticallyDampedSpring::new(),
            last_diff_scroll_frame: Instant::now(),
            diff_wheel_acc: 0.0,
            data: Arc::new(Mutex::new(PanelData::default())),
            panel_rect: Rect::ZERO,
            close_rect: Rect::ZERO,
            files_card_rect: Rect::ZERO,
            files_body_rect: Rect::ZERO,
            diff_card_rect: Rect::ZERO,
            file_row_rects: Vec::new(),
            files_scrollbar_thumb_rect: Rect::ZERO,
            diff_scrollbar_thumb_rect: Rect::ZERO,
            selected_cursor_rect: None,
            io: None,
        }
    }

    /// Install a native IO provider. Called once by the desktop fork
    /// after construction so the panel can shell out to `git status`
    /// without the shared crate referencing `std::process::Command`.
    pub fn set_io_provider(&mut self, io: Arc<dyn GitDiffIo>) {
        self.io = Some(io);
    }
}
