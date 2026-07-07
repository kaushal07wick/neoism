use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use neoism_terminal_core::ansi::CursorShape;

use super::helpers::{parse_blocks, source_from_lines, source_len_from_lines};
use super::types::*;
use super::vim::VimState;

const LARGE_MARKDOWN_FAST_PARSE_LINES: usize = 20_000;
const LARGE_MARKDOWN_FAST_PARSE_BYTES: usize = 2 * 1024 * 1024;

impl MarkdownPane {
    /// Construct a pane from in-memory source text (no filesystem read).
    /// Used by the web/wasm chrome where the daemon ships the file body
    /// over the wire — there is no on-disk path to `MarkdownPane::load`.
    /// `path` may be a stub (e.g. `PathBuf::from(title)`) since the
    /// renderer only needs `blocks` + `lines`.
    pub fn from_source(path: PathBuf, source: &str) -> Self {
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let blocks = if large_markdown_source(source, lines.len()) {
            Vec::new()
        } else {
            parse_blocks(source)
        };
        let title = first_heading_title(source).unwrap_or_else(|| {
            if let Some(MarkdownBlock::Heading { text, .. }) = blocks.first() {
                text.clone()
            } else {
                title
            }
        });
        let saved_baseline = lines.clone();
        Self {
            path,
            title,
            source_len_bytes: source.len(),
            lines,
            blocks,
            source_revision: 1,
            pending_line_edit: None,
            mode: MarkdownMode::Normal,
            cursor_line: 0,
            cursor_col: 0,
            visual_anchor: None,
            mouse_select_anchor: None,
            cursor_rect: None,
            follow_cursor: false,
            goal_visual_col: None,
            scroll_y: 0.0,
            target_scroll_y: 0.0,
            cursor_scroll_remainder: 0.0,
            scroll_viewport_height: 0.0,
            scroll_velocity_px_s: 0.0,
            scroll_velocity_moves_cursor: false,
            remote_cursors: Vec::new(),
            scroll_last_tick_at: None,
            content_height: 0.0,
            block_rects: Vec::new(),
            notebook_image_preview_dimensions: HashMap::new(),
            block_wrap_rows: HashMap::new(),
            block_wrap_hit_stops: HashMap::new(),
            table_rects: Vec::new(),
            table_cell_rects: Vec::new(),
            table_action_rects: Vec::new(),
            task_rects: Vec::new(),
            roster_rects: Vec::new(),
            pending_reveal_line: None,
            outline_rects: Vec::new(),
            table_scrollbar_rects: Vec::new(),
            link_rects: Vec::new(),
            copy_rects: Vec::new(),
            notebook_run_rects: Vec::new(),
            notebook_action_hovered: None,
            table_scroll_x: HashMap::new(),
            task_toggle_animations: HashMap::new(),
            yank_flashes: Vec::new(),
            enter_continuation_lines: HashSet::new(),
            hovered_line: None,
            dragging_line: None,
            dragging_table_scroll: None,
            scrollbar_rect: None,
            dragging_scrollbar: None,
            scrollbar_hovered: false,
            table_action_hovered: false,
            drag_mouse_y: 0.0,
            drag_start_y: 0.0,
            drag_moved: false,
            drag_drop_flash: None,
            pending_block_menu_rect: None,
            vim: VimState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            doc_history_bound: false,
            pending_doc_history: Vec::new(),
            wrap_cache: std::cell::RefCell::new(HashMap::new()),
            code_fence_cache: std::cell::RefCell::new(MarkdownCodeFenceCache::default()),
            link_target_cache: std::cell::RefCell::new(HashMap::new()),
            virtual_render: MarkdownVirtualRenderState::default(),
            saved_baseline,
            error: None,
        }
    }

    /// Replace the pane's source text and re-parse blocks. Cheap to
    /// call every time the host pushes a new content snapshot.
    pub fn set_source(&mut self, source: &str) {
        self.lines = source.lines().map(str::to_string).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.source_len_bytes = source.len();
        self.pending_line_edit = None;
        self.enter_continuation_lines.clear();
        self.link_target_cache.borrow_mut().clear();
        self.clear_notebook_image_preview_dimensions();
        if large_markdown_source(source, self.lines.len()) {
            self.blocks.clear();
        } else {
            self.blocks = parse_blocks(source);
        }
        if let Some(title) = first_heading_title(source) {
            self.title = title;
        } else if let Some(MarkdownBlock::Heading { text, .. }) = self.blocks.first() {
            self.title = text.clone();
        }
        self.source_revision = self.source_revision.saturating_add(1);
        self.clamp_cursor();
        self.saved_baseline = self.lines.clone();
        self.error = None;
    }

    pub fn set_source_preserving_view(&mut self, source: &str) {
        let cursor_line = self.cursor_line;
        let cursor_col = self.cursor_col;
        let mode = self.mode;
        let scroll_y = self.scroll_y;
        let target_scroll_y = self.target_scroll_y;
        let follow_cursor = self.follow_cursor;
        let goal_visual_col = self.goal_visual_col;
        self.set_source(source);
        self.mode = mode;
        self.cursor_line = cursor_line.min(self.lines.len().saturating_sub(1));
        self.cursor_col = cursor_col.min(self.lines[self.cursor_line].len());
        self.scroll_y = scroll_y;
        self.target_scroll_y = target_scroll_y;
        self.follow_cursor = follow_cursor;
        self.goal_visual_col = goal_visual_col;
        self.pending_line_edit = Some(MarkdownPendingLineEdit::Complex);
        self.virtual_render = MarkdownVirtualRenderState::default();
    }

    pub fn load(path: PathBuf) -> Self {
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut pane = Self {
            path,
            title,
            lines: vec![String::new()],
            blocks: Vec::new(),
            source_len_bytes: 0,
            source_revision: 1,
            pending_line_edit: None,
            mode: MarkdownMode::Normal,
            cursor_line: 0,
            cursor_col: 0,
            visual_anchor: None,
            mouse_select_anchor: None,
            cursor_rect: None,
            follow_cursor: false,
            goal_visual_col: None,
            scroll_y: 0.0,
            target_scroll_y: 0.0,
            cursor_scroll_remainder: 0.0,
            scroll_viewport_height: 0.0,
            scroll_velocity_px_s: 0.0,
            scroll_velocity_moves_cursor: false,
            remote_cursors: Vec::new(),
            scroll_last_tick_at: None,
            content_height: 0.0,
            block_rects: Vec::new(),
            notebook_image_preview_dimensions: HashMap::new(),
            block_wrap_rows: HashMap::new(),
            block_wrap_hit_stops: HashMap::new(),
            table_rects: Vec::new(),
            table_cell_rects: Vec::new(),
            table_action_rects: Vec::new(),
            task_rects: Vec::new(),
            roster_rects: Vec::new(),
            pending_reveal_line: None,
            outline_rects: Vec::new(),
            table_scrollbar_rects: Vec::new(),
            link_rects: Vec::new(),
            copy_rects: Vec::new(),
            notebook_run_rects: Vec::new(),
            notebook_action_hovered: None,
            table_scroll_x: HashMap::new(),
            task_toggle_animations: HashMap::new(),
            yank_flashes: Vec::new(),
            enter_continuation_lines: HashSet::new(),
            hovered_line: None,
            dragging_line: None,
            dragging_table_scroll: None,
            scrollbar_rect: None,
            dragging_scrollbar: None,
            scrollbar_hovered: false,
            table_action_hovered: false,
            drag_mouse_y: 0.0,
            drag_start_y: 0.0,
            drag_moved: false,
            drag_drop_flash: None,
            pending_block_menu_rect: None,
            vim: VimState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            doc_history_bound: false,
            pending_doc_history: Vec::new(),
            wrap_cache: std::cell::RefCell::new(HashMap::new()),
            code_fence_cache: std::cell::RefCell::new(MarkdownCodeFenceCache::default()),
            link_target_cache: std::cell::RefCell::new(HashMap::new()),
            virtual_render: MarkdownVirtualRenderState::default(),
            saved_baseline: vec![String::new()],
            error: None,
        };
        pane.reload();
        pane
    }

    pub fn reload(&mut self) {
        match std::fs::read_to_string(&self.path) {
            Ok(source) => {
                self.lines = source.lines().map(str::to_string).collect();
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.source_len_bytes = source.len();
                self.pending_line_edit = None;
                self.enter_continuation_lines.clear();
                self.link_target_cache.borrow_mut().clear();
                if self.should_defer_block_parse() {
                    self.blocks.clear();
                    self.source_revision = self.source_revision.saturating_add(1);
                } else {
                    self.rebuild_blocks();
                }
                if let Some(title) = first_heading_title(&source) {
                    self.title = title;
                } else if let Some(MarkdownBlock::Heading { text, .. }) =
                    self.blocks.first()
                {
                    self.title = text.clone();
                }
                self.clamp_cursor();
                self.saved_baseline = self.lines.clone();
                self.error = None;
            }
            Err(err) => {
                self.blocks.clear();
                self.error = Some(err.to_string());
            }
        }
    }

    pub fn cursor_shape(&self) -> CursorShape {
        match self.mode {
            MarkdownMode::Normal | MarkdownMode::Visual => CursorShape::Block,
            MarkdownMode::Insert => CursorShape::Beam,
        }
    }

    /// A buffer is dirty when its current content differs from the
    /// last-saved baseline — exactly how nvim reports `modified`. This
    /// is recomputed on every call (cheap pointer/length-first `Vec`
    /// compare), so an undo back to the saved text reads clean again
    /// and a redo into a divergent state reads dirty, without any edit
    /// path having to flip a flag.
    pub fn is_dirty(&self) -> bool {
        self.lines != self.saved_baseline
    }

    /// The document was flushed to disk by the daemon (single-writer
    /// save): the doc-level dirty bit clears without this pane having
    /// written anything itself. Re-anchor the saved baseline to the
    /// current content so `is_dirty()` reads clean.
    pub fn mark_saved(&mut self) {
        self.saved_baseline = self.lines.clone();
        self.error = None;
    }

    pub(crate) fn should_defer_block_parse(&self) -> bool {
        self.lines.len() > LARGE_MARKDOWN_FAST_PARSE_LINES
            || self.source_len_bytes > LARGE_MARKDOWN_FAST_PARSE_BYTES
    }

    pub(crate) fn should_use_local_history(&self) -> bool {
        self.should_defer_block_parse()
    }

    pub(crate) fn adjust_source_len(&mut self, delta: isize) {
        if delta >= 0 {
            self.source_len_bytes = self.source_len_bytes.saturating_add(delta as usize);
        } else {
            self.source_len_bytes =
                self.source_len_bytes.saturating_sub(delta.unsigned_abs());
        }
    }

    pub(crate) fn reset_source_len_from_lines(&mut self) {
        self.source_len_bytes = source_len_from_lines(&self.lines);
    }

    pub(crate) fn record_line_insert(&mut self, line: usize, byte_delta: i64) {
        self.pending_line_edit = match self.pending_line_edit {
            None => Some(MarkdownPendingLineEdit::Insert { line, byte_delta }),
            Some(MarkdownPendingLineEdit::Insert {
                line: existing,
                byte_delta: existing_delta,
            }) if existing == line => Some(MarkdownPendingLineEdit::Insert {
                line,
                byte_delta: existing_delta.saturating_add(byte_delta),
            }),
            Some(MarkdownPendingLineEdit::Insert {
                line: existing,
                byte_delta: existing_delta,
            }) if existing.saturating_add(1) == line => {
                Some(MarkdownPendingLineEdit::Insert {
                    line: existing,
                    byte_delta: existing_delta.saturating_add(byte_delta),
                })
            }
            _ => Some(MarkdownPendingLineEdit::Complex),
        };
    }

    pub(crate) fn record_line_delete(&mut self, line: usize, byte_delta: i64) {
        self.pending_line_edit = match self.pending_line_edit {
            None => Some(MarkdownPendingLineEdit::Delete { line, byte_delta }),
            Some(MarkdownPendingLineEdit::Delete {
                line: existing,
                byte_delta: existing_delta,
            }) if existing == line => Some(MarkdownPendingLineEdit::Delete {
                line,
                byte_delta: existing_delta.saturating_add(byte_delta),
            }),
            _ => Some(MarkdownPendingLineEdit::Complex),
        };
    }

    pub(crate) fn extend_pending_line_edit_bytes(&mut self, byte_delta: i64) {
        self.pending_line_edit = match self.pending_line_edit {
            Some(MarkdownPendingLineEdit::Insert {
                line,
                byte_delta: existing,
            }) => Some(MarkdownPendingLineEdit::Insert {
                line,
                byte_delta: existing.saturating_add(byte_delta),
            }),
            Some(MarkdownPendingLineEdit::Delete {
                line,
                byte_delta: existing,
            }) => Some(MarkdownPendingLineEdit::Delete {
                line,
                byte_delta: existing.saturating_add(byte_delta),
            }),
            other => other,
        };
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        let source = source_from_lines(&self.lines);
        match std::fs::write(&self.path, source) {
            Ok(()) => {
                self.saved_baseline = self.lines.clone();
                self.error = None;
                Ok(())
            }
            Err(err) => {
                self.error = Some(err.to_string());
                Err(err)
            }
        }
    }
}

fn large_markdown_source(source: &str, line_count: usize) -> bool {
    line_count > LARGE_MARKDOWN_FAST_PARSE_LINES
        || source.len() > LARGE_MARKDOWN_FAST_PARSE_BYTES
}

fn first_heading_title(source: &str) -> Option<String> {
    source.lines().take(512).find_map(|line| {
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        if !(1..=6).contains(&level)
            || !trimmed.chars().nth(level).is_some_and(|ch| ch == ' ')
        {
            return None;
        }
        let title = trimmed[level..].trim();
        (!title.is_empty()).then(|| title.to_string())
    })
}
