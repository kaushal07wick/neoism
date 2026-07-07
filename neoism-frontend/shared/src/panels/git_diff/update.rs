use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use web_time::Instant;

use crate::widgets::diff_card;

use super::state::GitDiffPanel;
use super::types::{PanelHit, ScrollbarKind};
use super::{
    FILE_ROW_HEIGHT, FILE_SCROLL_OFF_ROWS, PANEL_MAX_WIDTH, PANEL_MIN_WIDTH,
    PANEL_OPEN_ANIMATION_LENGTH, REFRESH_DEBOUNCE_MS, RESIZE_HIT_HALF,
    SCROLL_ANIMATION_LENGTH,
};

impl GitDiffPanel {
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// True while the panel reserves its right-edge column. Same as
    /// `is_visible` today; kept as a separate predicate so a future
    /// slide-out animation can reserve the column past the visibility
    /// flip without changing call sites.
    pub fn is_present(&self) -> bool {
        self.visible
    }

    pub fn is_focused(&self) -> bool {
        self.focused && self.visible
    }

    pub fn set_focused(&mut self, f: bool) {
        self.focused = f && self.visible;
    }

    /// Effective panel width in window-logical pixels. Used by the
    /// screen layer's chrome-layout pass to reserve a right margin so
    /// editor/terminal panes don't paint underneath. Honours the
    /// user-resizable `width` field.
    pub fn effective_width(&self, window_w: f32) -> f32 {
        if !self.is_present() {
            return 0.0;
        }
        // Scale + window-relative cap so a tiny window can never have
        // the panel eat more than 80% of the editor area.
        let scaled = self.width * self.scale;
        scaled.min(window_w * 0.8)
    }

    /// Current panel width in logical pixels (pre-scale).
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Adjust the panel's width by `delta` logical pixels. Clamped to
    /// `[PANEL_MIN_WIDTH, PANEL_MAX_WIDTH]`. Mouse-drag resize and
    /// keyboard resize both call this so the chrome layout sees a
    /// single source of truth.
    pub fn resize(&mut self, delta: f32) {
        self.width = (self.width + delta).clamp(PANEL_MIN_WIDTH, PANEL_MAX_WIDTH);
    }

    /// Hit-test for the leading-edge resize gripper. Active only when
    /// the panel is visible — mirrors `is_hovering_file_tree_resize_edge`.
    pub fn is_hovering_resize_edge(&self, mx: f32, my: f32) -> bool {
        if !self.visible || self.panel_rect.w <= 0.0 {
            return false;
        }
        let edge_x = self.panel_rect.x;
        let in_y = my >= self.panel_rect.y && my <= self.panel_rect.y + self.panel_rect.h;
        in_y && (mx - edge_x).abs() <= RESIZE_HIT_HALF
    }

    pub fn select_next(&mut self) {
        let count = self.file_count();
        if count == 0 {
            return;
        }
        if self.selected + 1 >= count {
            // Already on the last file — let the diff card take the
            // keystroke instead so ↓ keeps doing something useful
            // when the user is reading the last file's diff.
            self.scroll_diff_rows(2);
            return;
        }
        let next = self.selected + 1;
        let _ = self.select_file(next);
    }

    pub fn select_prev(&mut self) {
        if self.selected == 0 {
            self.scroll_diff_rows(-2);
            return;
        }
        let prev = self.selected - 1;
        let _ = self.select_file(prev);
    }

    /// Returns the (path, repo_root) of the currently-selected file so
    /// the screen layer can `:edit` it on Enter. `None` if the panel
    /// has no files yet.
    pub fn selected_file_target(&self) -> Option<(PathBuf, PathBuf)> {
        let data = self.data.lock().ok()?;
        let f = data.files.get(self.selected)?;
        let root = data.repo_root.clone()?;
        let abs = root.join(&f.path);
        Some((abs, root))
    }

    pub fn is_animating(&self) -> bool {
        self.file_scroll_spring.position != 0.0
            || self.diff_scroll_spring.position != 0.0
            || self.open_progress() < 1.0
    }

    /// Cursor caret rect (window-logical) so the screen layer can
    /// animate the trail-cursor over to the panel's selected file row
    /// — same path the file_tree uses to make the terminal caret jump
    /// when the user navigates over.
    pub fn selected_cursor_rect(&self) -> Option<[f32; 4]> {
        self.selected_cursor_rect
    }

    pub fn needs_redraw(&self) -> bool {
        if !self.visible {
            return false;
        }
        if self.is_animating() {
            return true;
        }
        if let Ok(data) = self.data.lock() {
            if data.loading {
                return true;
            }
        }
        false
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(0.5, 3.0);
        self.file_scroll_spring.reset();
        self.diff_scroll_spring.reset();
    }

    pub fn open(&mut self, repo_root: Option<PathBuf>, branch: Option<String>) {
        self.visible = true;
        self.focused = true;
        self.open_started_at = Some(Instant::now());
        self.selected = 0;
        self.file_scroll = 0.0;
        self.file_wheel_acc = 0.0;
        self.file_scroll_spring.reset();
        self.diff_scroll = 0.0;
        self.diff_wheel_acc = 0.0;
        self.diff_scroll_spring.reset();
        self.refresh(repo_root, branch);
    }

    pub fn toggle(&mut self, repo_root: Option<PathBuf>, branch: Option<String>) {
        if self.visible {
            self.close();
        } else {
            self.open(repo_root, branch);
        }
    }

    pub fn close(&mut self) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.focused = false;
        self.open_started_at = None;
    }

    pub(super) fn open_progress(&self) -> f32 {
        if !self.visible {
            return 0.0;
        }
        let Some(started) = self.open_started_at else {
            return 1.0;
        };
        let t = (Instant::now()
            .saturating_duration_since(started)
            .as_secs_f32()
            / PANEL_OPEN_ANIMATION_LENGTH.max(0.001))
        .clamp(0.0, 1.0);
        let inv = 1.0 - t;
        1.0 - inv * inv * inv
    }

    pub fn refresh(&mut self, repo_root: Option<PathBuf>, branch: Option<String>) {
        let now = Instant::now();
        let id = {
            let mut data = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let same_repo = data.repo_root.as_deref() == repo_root.as_deref();
            if let Some(last) = data.last_refresh {
                if same_repo
                    && now.saturating_duration_since(last).as_millis()
                        < REFRESH_DEBOUNCE_MS
                    && !data.files.is_empty()
                {
                    data.branch = branch.clone();
                    return;
                }
            }
            data.refresh_id = data.refresh_id.wrapping_add(1);
            data.loading = true;
            data.error = None;
            data.branch = branch;
            data.repo_root = repo_root.clone();
            data.last_refresh = Some(now);
            data.refresh_id
        };

        let Some(root) = repo_root else {
            if let Ok(mut data) = self.data.lock() {
                data.loading = false;
                data.files.clear();
                data.diffs.clear();
                data.error = Some("Not a git repository".to_string());
            }
            return;
        };

        // Web/wasm has no `GitDiffIo` provider installed by default —
        // the daemon pushes data directly into `self.data` instead.
        // Native fork installs an `Arc<dyn GitDiffIo>` so we can shell
        // out to `git status` here on a background thread.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(io) = self.io.clone() else {
                if let Ok(mut data) = self.data.lock() {
                    data.loading = false;
                }
                return;
            };
            let arc = Arc::clone(&self.data);
            std::thread::spawn(move || {
                let files = io.collect_files(&root);
                let first_diff = files
                    .first()
                    .map(|f| (f.path.clone(), super::parse::load_diff(&root, f)));
                let Ok(mut data) = arc.lock() else { return };
                if data.refresh_id != id {
                    return;
                }
                data.loading = false;
                data.files = files;
                data.diffs.clear();
                if let Some((path, diff)) = first_diff {
                    data.diffs.insert(path, diff);
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = root;
            let _ = id;
        }
    }

    /// Host push (web): replace the changed-file list. On wasm there
    /// is no `GitDiffIo` provider — the daemon is the data source and
    /// the host stores results back here, mirroring the native
    /// refresh thread's store-back.
    pub fn host_set_files(&mut self, files: Vec<super::types::FileChange>) {
        let file_count = files.len();
        if let Ok(mut data) = self.data.lock() {
            data.loading = false;
            data.error = None;
            data.files = files;
            data.diffs.clear();
        }
        if self.selected >= file_count {
            self.selected = 0;
            self.file_scroll = 0.0;
        }
    }

    /// Host push (web): the diff body for one file, parsed from raw
    /// `git diff` patch text (hunk `@@` headers included).
    pub fn host_set_diff_text(&mut self, path: &str, patch: &str) {
        let mut lines = Vec::new();
        super::parse::parse_diff_into(patch.as_bytes(), &mut lines);
        if let Ok(mut data) = self.data.lock() {
            data.diffs.insert(path.to_string(), lines);
        }
    }

    /// Host push (web): surface a daemon-side failure in the panel
    /// body instead of spinning on `loading` forever.
    pub fn host_set_error(&mut self, message: String) {
        if let Ok(mut data) = self.data.lock() {
            data.loading = false;
            data.error = Some(message);
        }
    }

    pub fn active_rect(&self) -> Option<[f32; 4]> {
        if self.panel_rect.w <= 0.0 || self.panel_rect.h <= 0.0 {
            return None;
        }
        Some(self.panel_rect.as_array())
    }

    pub fn hit_test(&self, mx: f32, my: f32) -> PanelHit {
        if !self.visible || !self.panel_rect.contains(mx, my) {
            return PanelHit::Outside;
        }
        if self.close_rect.contains(mx, my) {
            return PanelHit::Close;
        }
        for (idx, rect) in &self.file_row_rects {
            if rect.contains(mx, my) {
                return PanelHit::FileRow(*idx);
            }
        }
        PanelHit::Inside
    }

    /// Programmatic select-by-index. Used by `select_next/prev` and
    /// click handlers; lazy-loads the file's diff and springs the
    /// selected row into the file-list viewport.
    pub fn select_file(&mut self, idx: usize) -> bool {
        let (path, repo_root, needs_load) = {
            let data = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            if idx >= data.files.len() {
                return false;
            }
            let f = &data.files[idx];
            let needs_load = !data.diffs.contains_key(&f.path);
            (f.path.clone(), data.repo_root.clone(), needs_load)
        };
        let changed = idx != self.selected;
        self.selected = idx;
        // Reset diff scroll so a freshly-selected file lands at the
        // top of its diff body — otherwise the bottom card would
        // start mid-diff for the new file.
        self.diff_scroll = 0.0;
        self.diff_wheel_acc = 0.0;
        self.diff_scroll_spring.reset();
        self.scroll_selected_into_view();
        if needs_load {
            // Background load of the per-file diff. Native only —
            // wasm relies on the daemon pushing diffs into
            // `self.data.diffs` ahead of time.
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(root) = repo_root {
                let arc = Arc::clone(&self.data);
                let path_for_thread = path.clone();
                std::thread::spawn(move || {
                    let file = {
                        let Ok(d) = arc.lock() else { return };
                        d.files.iter().find(|f| f.path == path_for_thread).cloned()
                    };
                    let Some(file) = file else { return };
                    let diff = super::parse::load_diff(&root, &file);
                    if let Ok(mut d) = arc.lock() {
                        d.diffs.insert(path_for_thread, diff);
                    }
                });
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = (path, repo_root);
            }
        }
        changed
    }

    pub fn scroll_diff_rows(&mut self, rows: i32) {
        if rows == 0 {
            return;
        }
        let line_h = diff_card::LINE_HEIGHT * self.scale;
        if line_h <= 0.0 {
            return;
        }
        let total_lines = self.current_diff_len();
        let visible_h =
            (self.diff_card_rect.h - diff_card::HEADER_HEIGHT * self.scale).max(0.0);
        let visible = ((visible_h / line_h).floor() as usize).max(1);
        let max_top = total_lines.saturating_sub(visible);
        let max_scroll = max_top as f32 * line_h;
        let target = (self.diff_scroll + rows as f32 * line_h).clamp(0.0, max_scroll);
        let drow = target - self.diff_scroll;
        if drow.abs() < 0.5 {
            return;
        }
        let was_idle = self.diff_scroll_spring.position == 0.0;
        self.diff_scroll = target;
        self.diff_scroll_spring.position += drow;
        if was_idle {
            self.last_diff_scroll_frame = Instant::now();
        }
    }

    pub fn scroll_at(&mut self, mx: f32, my: f32, delta: f32) -> bool {
        if !self.visible || !self.panel_rect.contains(mx, my) {
            return false;
        }
        // Diff card sits below the files card — route wheel events to
        // whichever card the mouse is over so the user can scroll the
        // file list and the diff independently.
        if self.diff_card_rect.contains(mx, my) {
            self.scroll_diff_pixels(delta);
        } else if self.files_card_rect.contains(mx, my) {
            self.scroll_files_pixels(delta);
        }
        true
    }

    fn scroll_files_pixels(&mut self, delta: f32) {
        let row_h = FILE_ROW_HEIGHT * self.scale;
        if row_h <= 0.0 || delta == 0.0 {
            return;
        }
        self.file_wheel_acc += delta;
        let mut rows = 0i32;
        while self.file_wheel_acc.abs() >= row_h {
            let sign = self.file_wheel_acc.signum();
            self.file_wheel_acc -= sign * row_h;
            rows += if sign > 0.0 { -1 } else { 1 };
        }
        if rows == 0 {
            return;
        }
        let total = self.file_count();
        let visible = ((self.files_body_rect.h / row_h).floor() as usize).max(1);
        let max_top = total.saturating_sub(visible);
        let max_scroll = max_top as f32 * row_h;
        let target = (self.file_scroll + rows as f32 * row_h).clamp(0.0, max_scroll);
        let drow = target - self.file_scroll;
        if drow == 0.0 {
            return;
        }
        let was_idle = self.file_scroll_spring.position == 0.0;
        self.file_scroll = target;
        self.file_scroll_spring.position += drow;
        if was_idle {
            self.last_file_scroll_frame = Instant::now();
        }
    }

    fn scroll_diff_pixels(&mut self, delta: f32) {
        let line_h = diff_card::LINE_HEIGHT * self.scale;
        if line_h <= 0.0 || delta == 0.0 {
            return;
        }
        self.diff_wheel_acc += delta;
        let mut rows = 0i32;
        while self.diff_wheel_acc.abs() >= line_h {
            let sign = self.diff_wheel_acc.signum();
            self.diff_wheel_acc -= sign * line_h;
            rows += if sign > 0.0 { -1 } else { 1 };
        }
        if rows == 0 {
            return;
        }
        let total_lines = self.current_diff_len();
        let visible_h =
            (self.diff_card_rect.h - diff_card::HEADER_HEIGHT * self.scale).max(0.0);
        let visible = ((visible_h / line_h).floor() as usize).max(1);
        let max_top = total_lines.saturating_sub(visible);
        let max_scroll = max_top as f32 * line_h;
        let target = (self.diff_scroll + rows as f32 * line_h).clamp(0.0, max_scroll);
        let drow = target - self.diff_scroll;
        if drow == 0.0 {
            return;
        }
        let was_idle = self.diff_scroll_spring.position == 0.0;
        self.diff_scroll = target;
        self.diff_scroll_spring.position += drow;
        if was_idle {
            self.last_diff_scroll_frame = Instant::now();
        }
    }

    pub(super) fn scroll_selected_into_view(&mut self) {
        let row_h = FILE_ROW_HEIGHT * self.scale;
        if row_h <= 0.0 || self.files_body_rect.h <= 0.0 {
            return;
        }
        let visible = ((self.files_body_rect.h / row_h).floor() as usize).max(1);
        // Scroll-off: keep `scroll_off` rows of context above and
        // below the cursor, mirroring the file_tree's behaviour. The
        // band shrinks gracefully on tiny viewports so the cursor can
        // still reach the very top/bottom row.
        let scroll_off = FILE_SCROLL_OFF_ROWS.min(visible.saturating_sub(1) / 2);
        let total = self.file_count();
        let last_idx = total.saturating_sub(1);

        // Selected row's logical y inside the scroll space.
        let row_y = self.selected as f32 * row_h;
        let view_top = self.file_scroll;
        let view_bot = view_top + self.files_body_rect.h;

        // Distance from the *padded* viewport edges so the cursor
        // can never touch them unless we're at the actual list bound.
        let pad = scroll_off as f32 * row_h;
        let target = if self.selected <= scroll_off {
            // Near the very top — pin to 0 so the first row stays
            // anchored at the top of the viewport.
            0.0
        } else if last_idx.saturating_sub(self.selected) <= scroll_off {
            // Near the very bottom — pin so the last row sits at
            // the viewport bottom.
            (total as f32 * row_h - self.files_body_rect.h).max(0.0)
        } else if row_y < view_top + pad {
            (row_y - pad).max(0.0)
        } else if row_y + row_h > view_bot - pad {
            (row_y + row_h + pad - self.files_body_rect.h).max(0.0)
        } else {
            self.file_scroll
        };
        let max_top = total.saturating_sub(visible);
        let max_scroll = max_top as f32 * row_h;
        let target = target.clamp(0.0, max_scroll);
        let drow = target - self.file_scroll;
        if drow.abs() < 0.5 {
            return;
        }
        let was_idle = self.file_scroll_spring.position == 0.0;
        self.file_scroll = target;
        self.file_scroll_spring.position += drow;
        if was_idle {
            self.last_file_scroll_frame = Instant::now();
        }
    }

    /// Hit-test for the right-edge scrollbar of either card. Returns
    /// the kind so the screen layer can route a drag to the right
    /// scroll axis.
    pub fn scrollbar_hit(&self, mx: f32, my: f32) -> Option<ScrollbarKind> {
        if !self.visible {
            return None;
        }
        if super::render::hit_scrollbar_thumb(&self.files_scrollbar_thumb_rect, mx, my) {
            return Some(ScrollbarKind::Files);
        }
        if super::render::hit_scrollbar_thumb(&self.diff_scrollbar_thumb_rect, mx, my) {
            return Some(ScrollbarKind::Diff);
        }
        None
    }

    /// Drag a scrollbar thumb to a new vertical position. `mouse_y`
    /// is window-logical. Maps the thumb's track position onto the
    /// underlying scroll range and snaps the spring so the drag feels
    /// 1:1 instead of springing back.
    pub fn drag_scrollbar(&mut self, kind: ScrollbarKind, mouse_y: f32) {
        match kind {
            ScrollbarKind::Files => {
                let row_h = FILE_ROW_HEIGHT * self.scale;
                let total = self.file_count();
                let visible = ((self.files_body_rect.h / row_h).floor() as usize).max(1);
                if total <= visible || self.files_body_rect.h <= 0.0 {
                    return;
                }
                let max_top = total.saturating_sub(visible);
                let max_scroll = max_top as f32 * row_h;
                // Map `mouse_y` linearly across the track height.
                let progress = ((mouse_y - self.files_body_rect.y)
                    / self.files_body_rect.h.max(1.0))
                .clamp(0.0, 1.0);
                let target = (progress * max_scroll).clamp(0.0, max_scroll);
                self.file_scroll = target;
                self.file_scroll_spring.reset();
            }
            ScrollbarKind::Diff => {
                let line_h = diff_card::LINE_HEIGHT * self.scale;
                let total_lines = self.current_diff_len();
                let body_h = (self.diff_card_rect.h
                    - diff_card::HEADER_HEIGHT * self.scale)
                    .max(0.0);
                let visible = ((body_h / line_h).floor() as usize).max(1);
                if total_lines <= visible || body_h <= 0.0 {
                    return;
                }
                let max_top = total_lines.saturating_sub(visible);
                let max_scroll = max_top as f32 * line_h;
                let track_top =
                    self.diff_card_rect.y + diff_card::HEADER_HEIGHT * self.scale;
                let progress = ((mouse_y - track_top) / body_h.max(1.0)).clamp(0.0, 1.0);
                let target = (progress * max_scroll).clamp(0.0, max_scroll);
                self.diff_scroll = target;
                self.diff_scroll_spring.reset();
            }
        }
    }

    pub(super) fn file_count(&self) -> usize {
        self.data.lock().map(|d| d.files.len()).unwrap_or(0)
    }

    pub(super) fn current_diff_len(&self) -> usize {
        self.data
            .lock()
            .map(|d| {
                d.files
                    .get(self.selected)
                    .and_then(|f| d.diffs.get(&f.path))
                    .map(|v| {
                        diff_card::visual_row_count(
                            v,
                            diff_card::body_text_width(self.diff_card_rect.w, self.scale),
                            self.scale,
                        )
                    })
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    pub(super) fn tick_file_scroll(&mut self) -> f32 {
        if self.file_scroll_spring.position == 0.0 {
            self.last_file_scroll_frame = Instant::now();
            return 0.0;
        }
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.last_file_scroll_frame)
            .as_secs_f32()
            .min(0.05);
        self.last_file_scroll_frame = now;
        self.file_scroll_spring.update(dt, SCROLL_ANIMATION_LENGTH);
        self.file_scroll_spring.position
    }

    pub(super) fn tick_diff_scroll(&mut self) -> f32 {
        if self.diff_scroll_spring.position == 0.0 {
            self.last_diff_scroll_frame = Instant::now();
            return 0.0;
        }
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.last_diff_scroll_frame)
            .as_secs_f32()
            .min(0.05);
        self.last_diff_scroll_frame = now;
        self.diff_scroll_spring.update(dt, SCROLL_ANIMATION_LENGTH);
        self.diff_scroll_spring.position
    }
}
