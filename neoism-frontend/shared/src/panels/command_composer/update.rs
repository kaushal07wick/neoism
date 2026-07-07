//! Non-rendering `CommandComposer` methods: visibility/scale toggles,
//! row reservation math, completion motion springs, and the public
//! query surface the screen uses (wrap line ranges, popup rect, etc.).

use web_time::Instant;

use sugarloaf::Sugarloaf;

use super::classify::wrap_lines_for_width;
use super::state::CommandComposer;
use super::types::{
    ComposerFrame, COMPLETION_CURSOR_ANIMATION_LENGTH, COMPLETION_ROW_HEIGHT,
    COMPLETION_SCROLL_ANIMATION_LENGTH, COMPLETION_SCROLL_OFF_ROWS, COMPOSER_BASE_HEIGHT,
    COMPOSER_MAX_INPUT_LINES, COMPOSER_WRAP_HARD_LIMIT, DEPTH, FONT_SIZE,
    ORDER_STATUS_JOIN, SCALE_MAX, SCALE_MIN, SHOW_FOOTER_HINT_ROW,
};
use crate::input::InputBuffer;
use crate::primitives::IdeTheme;

impl CommandComposer {
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(SCALE_MIN, SCALE_MAX);
    }

    pub fn set_visible(&mut self, visible: bool) {
        if !visible {
            self.last_frame = ComposerFrame::default();
            self.completion_popup_rect = None;
            self.last_input_wrap = None;
        }
        self.visible = visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn completion_popup_rect(&self) -> Option<[f32; 4]> {
        self.completion_popup_rect
    }

    pub fn input_visual_line_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        let Some(layout) = self.last_input_wrap else {
            return vec![(0, text.len())];
        };
        wrap_lines_for_width(
            text,
            layout.first_width,
            layout.wrapped_width,
            layout.cell_width,
            COMPOSER_WRAP_HARD_LIMIT,
        )
        .into_iter()
        .map(|line| (line.start, line.end))
        .collect()
    }

    /// Logical-pixel height of the composer chassis. The terminal pane
    /// shrinks its renderable height by this number so the composer
    /// doesn't paint over output.
    pub fn scaled_height(&self) -> f32 {
        COMPOSER_BASE_HEIGHT * self.scale
    }

    pub fn estimated_input_line_count(
        &self,
        pane_width_logical: f32,
        cell_width_logical: f32,
        text: &str,
    ) -> usize {
        if text.is_empty() || pane_width_logical <= 0.0 || cell_width_logical <= 0.0 {
            return 1;
        }
        // Prefer the widths the renderer actually MEASURED last frame
        // (`last_input_wrap`). The fixed-offset estimate below can
        // under-count wraps whenever the true text area is narrower
        // than the guess — the reservation then comes up short and the
        // surplus wrapped rows render clipped ("pasting hides stuff").
        if let Some(layout) = self.last_input_wrap {
            return wrap_lines_for_width(
                text,
                layout.first_width,
                layout.wrapped_width,
                layout.cell_width,
                COMPOSER_MAX_INPUT_LINES,
            )
            .len()
            .clamp(1, COMPOSER_MAX_INPUT_LINES);
        }
        let s = self.scale;
        let fixed_first_line_px = 320.0 * s;
        let fixed_wrapped_line_px = 136.0 * s;
        let font_size = FONT_SIZE * s;
        let min_width = cell_width_logical.max(font_size * 0.56) * 8.0;
        let first_width = (pane_width_logical - fixed_first_line_px).max(min_width);
        let wrapped_width = (pane_width_logical - fixed_wrapped_line_px).max(min_width);
        wrap_lines_for_width(
            text,
            first_width,
            wrapped_width,
            cell_width_logical,
            COMPOSER_MAX_INPUT_LINES,
        )
        .len()
        .clamp(1, COMPOSER_MAX_INPUT_LINES)
    }

    /// `pane_rows`: total cell rows of the hosting pane. The composer
    /// grows with content up to `COMPOSER_MAX_INPUT_LINES` but never
    /// takes more than roughly half a short pane — beyond either cap
    /// the input window scrolls internally. Pass 0 when unknown; the
    /// clamp then only guarantees the 3-row minimum.
    pub fn reserved_rows_for_input(
        &self,
        cell_height_logical: f32,
        pane_width_logical: f32,
        cell_width_logical: f32,
        pane_rows: usize,
        text: &str,
    ) -> usize {
        if cell_height_logical <= 0.0 {
            return 1;
        }
        let input_lines =
            self.estimated_input_line_count(pane_width_logical, cell_width_logical, text);
        let extra_rows = if SHOW_FOOTER_HINT_ROW { 2 } else { 1 };
        let rows = input_lines.saturating_add(extra_rows).max(extra_rows + 1);
        rows.min((pane_rows / 2).max(3))
    }

    pub fn terminal_reserved_rows_for_input(
        &self,
        cell_height_logical: f32,
        pane_width_logical: f32,
        cell_width_logical: f32,
        pane_rows: usize,
        text: &str,
    ) -> usize {
        self.reserved_rows_for_input(
            cell_height_logical,
            pane_width_logical,
            cell_width_logical,
            pane_rows,
            text,
        )
    }

    pub fn actual_chassis_height_for_input(
        &self,
        cell_height_logical: f32,
        pane_width_logical: f32,
        cell_width_logical: f32,
        pane_rows: usize,
        text: &str,
    ) -> f32 {
        if cell_height_logical <= 0.0 {
            return self.scaled_height();
        }
        self.reserved_rows_for_input(
            cell_height_logical,
            pane_width_logical,
            cell_width_logical,
            pane_rows,
            text,
        ) as f32
            * cell_height_logical
    }

    pub fn last_frame(&self) -> ComposerFrame {
        self.last_frame
    }

    /// Paint a narrow frame overlay after the status line so the
    /// terminal composer visually contains the status strip below it.
    /// Editor/nvim panes do not call this, keeping their status line
    /// independent.
    pub fn render_status_join(
        &self,
        sugarloaf: &mut Sugarloaf,
        status_y: f32,
        status_h: f32,
        theme: &IdeTheme,
    ) {
        if !self.visible {
            return;
        }
        let [x, y, w, h] = self.last_frame.chassis_rect;
        if w <= 0.0 || h <= 0.0 || status_h <= 0.0 {
            return;
        }

        let s = self.scale;
        let seam_stroke = (1.25 * s).max(1.0);
        let color = theme.f32(theme.surface);
        let seam_y = status_y.max(y);

        sugarloaf.rect(
            None,
            x,
            seam_y,
            w,
            seam_stroke,
            color,
            DEPTH,
            ORDER_STATUS_JOIN,
        );
    }

    /// True when the composer has an in-flight visual animation.
    /// An editable prompt by itself is static; treating it as animation
    /// keeps native/web hosts in a continuous redraw loop while idle.
    #[allow(dead_code)]
    pub fn is_animating(&self, input: &dyn InputBuffer) -> bool {
        self.visible && input.is_prompt_animating()
    }

    pub(super) fn completion_row_height(&self) -> f32 {
        COMPLETION_ROW_HEIGHT * self.scale
    }

    pub(super) fn reset_completion_motion(&mut self) {
        self.last_completion_selected = None;
        self.completion_scroll_offset = 0;
        self.completion_scroll_spring.reset();
        self.completion_cursor_spring.reset();
        self.last_completion_scroll_frame = Instant::now();
        self.last_completion_cursor_frame = Instant::now();
        self.completion_last_scroll_time = None;
    }

    pub(super) fn set_completion_scroll_offset(
        &mut self,
        new_offset: usize,
        count: usize,
        visible: usize,
    ) {
        let max_offset = count.saturating_sub(visible.max(1));
        let new_offset = new_offset.min(max_offset);
        let old_offset = self.completion_scroll_offset;
        if old_offset == new_offset {
            return;
        }

        self.completion_scroll_offset = new_offset;
        let was_idle = self.completion_scroll_spring.position == 0.0;
        let rows = new_offset as i32 - old_offset as i32;
        if rows.unsigned_abs() as usize >= visible.max(1) {
            self.completion_scroll_spring.reset();
            self.last_completion_scroll_frame = Instant::now();
        } else {
            self.completion_scroll_spring.position +=
                rows as f32 * self.completion_row_height();
            if was_idle {
                self.last_completion_scroll_frame = Instant::now();
            }
        }
        self.completion_last_scroll_time = Some(Instant::now());
    }

    pub(super) fn sync_completion_motion(
        &mut self,
        count: usize,
        selected: usize,
        visible: usize,
    ) {
        if count == 0 {
            self.reset_completion_motion();
            return;
        }

        let visible = visible.max(1).min(count);
        let mut jumped_outside_view = false;
        if let Some(previous) = self.last_completion_selected {
            if previous != selected {
                let rows = previous as i32 - selected as i32;
                if rows.unsigned_abs() as usize >= visible {
                    jumped_outside_view = true;
                    self.completion_cursor_spring.reset();
                    self.last_completion_cursor_frame = Instant::now();
                } else {
                    let was_idle = self.completion_cursor_spring.position == 0.0;
                    self.completion_cursor_spring.position +=
                        rows as f32 * self.completion_row_height();
                    if was_idle {
                        self.last_completion_cursor_frame = Instant::now();
                    }
                }
            }
        }
        self.last_completion_selected = Some(selected);

        let scrolloff = COMPLETION_SCROLL_OFF_ROWS.min(visible.saturating_sub(1) / 2);
        if selected < self.completion_scroll_offset.saturating_add(scrolloff) {
            self.set_completion_scroll_offset(
                selected.saturating_sub(scrolloff),
                count,
                visible,
            );
        } else if selected.saturating_add(scrolloff)
            >= self.completion_scroll_offset.saturating_add(visible)
        {
            self.set_completion_scroll_offset(
                selected + scrolloff + 1 - visible,
                count,
                visible,
            );
        }

        let max_offset = count.saturating_sub(visible);
        if self.completion_scroll_offset > max_offset {
            self.set_completion_scroll_offset(max_offset, count, visible);
        }
        if jumped_outside_view {
            self.completion_scroll_spring.reset();
            self.last_completion_scroll_frame = Instant::now();
        }
    }

    pub(super) fn tick_completion_scroll(&mut self) -> f32 {
        if self.completion_scroll_spring.position == 0.0 {
            self.last_completion_scroll_frame = Instant::now();
            return 0.0;
        }
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.last_completion_scroll_frame)
            .as_secs_f32()
            .min(0.05);
        self.last_completion_scroll_frame = now;
        self.completion_scroll_spring
            .update(dt, COMPLETION_SCROLL_ANIMATION_LENGTH);
        self.completion_scroll_spring.position
    }

    pub(super) fn tick_completion_cursor(&mut self) -> f32 {
        if self.completion_cursor_spring.position == 0.0 {
            self.last_completion_cursor_frame = Instant::now();
            return 0.0;
        }
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.last_completion_cursor_frame)
            .as_secs_f32()
            .min(0.05);
        self.last_completion_cursor_frame = now;
        self.completion_cursor_spring
            .update(dt, COMPLETION_CURSOR_ANIMATION_LENGTH);
        self.completion_cursor_spring.position
    }
}
