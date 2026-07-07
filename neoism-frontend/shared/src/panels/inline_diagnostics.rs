//! Inline diagnostics painted by Rust chrome on top of editor rows.
//!
//! This intentionally does not use Neovim virtual text/lines or
//! underlines. It mirrors Zed's inline diagnostic lens: first-line
//! message, one item per row, placed after the rendered line end plus
//! padding.

use std::collections::HashMap;

use sugarloaf::text::DrawOpts;
use sugarloaf::Sugarloaf;

use crate::primitives::ide_theme::IdeTheme;

const PAD_X: f32 = 7.0;
const PADDING_CELLS: u32 = 4;
const MIN_LENS_COLUMN: u32 = 0;
const MIN_LENS_WIDTH_PX: f32 = 44.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineDiagnosticSeverity {
    Error,
    Warn,
}

impl InlineDiagnosticSeverity {
    pub fn from_nvim(severity: u8) -> Option<Self> {
        match severity {
            1 => Some(Self::Error),
            2 => Some(Self::Warn),
            _ => None,
        }
    }

    fn color(self, theme: &IdeTheme) -> u32 {
        match self {
            Self::Error => theme.red,
            Self::Warn => theme.yellow,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warn => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineDiagnosticItem {
    /// Output row in the editor grid after applying the same source-line
    /// offset used by the smooth-scroll grid renderer.
    pub row: i32,
    pub severity: InlineDiagnosticSeverity,
    pub message: String,
    /// Occupied text width for this visible editor row, in terminal
    /// columns. The host computes this from the rendered row so the
    /// diagnostic can sit after code when there is room.
    pub text_end_col: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct InlineDiagnosticsLayout {
    /// Physical-pixel left edge of the visible editor grid.
    pub pane_left_px: f32,
    /// Physical-pixel top edge of the first visible editor row.
    pub visible_top_px: f32,
    pub pane_width_px: f32,
    pub pane_height_px: f32,
    pub cell_width_px: f32,
    pub cell_height_px: f32,
    pub columns: u32,
    pub visible_rows: u32,
    /// Same pixel residual used by the grid smooth-scroll uniform.
    pub editor_pixel_offset_y: f32,
    pub scale_factor: f32,
    pub chrome_scale: f32,
}

#[derive(Default)]
pub struct InlineDiagnostics;

impl InlineDiagnostics {
    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &self,
        sugarloaf: &mut Sugarloaf,
        items: &[InlineDiagnosticItem],
        layout: InlineDiagnosticsLayout,
        theme: &IdeTheme,
    ) {
        if items.is_empty()
            || layout.cell_width_px <= 0.0
            || layout.cell_height_px <= 0.0
            || layout.scale_factor <= 0.0
            || layout.visible_rows == 0
            || layout.columns == 0
        {
            return;
        }

        let mut by_row: HashMap<i32, usize> = HashMap::new();
        for (idx, item) in items.iter().enumerate() {
            if !is_visible(item, layout) || item.message.trim().is_empty() {
                continue;
            }
            by_row
                .entry(item.row)
                .and_modify(|current| {
                    let current_item = &items[*current];
                    if item.severity.rank() < current_item.severity.rank() {
                        *current = idx;
                    }
                })
                .or_insert(idx);
        }
        if by_row.is_empty() {
            return;
        }

        let mut sorted: Vec<usize> = by_row.values().copied().collect();
        sorted.sort_by_key(|idx| {
            let item = &items[*idx];
            (item.row, item.severity.rank())
        });

        for idx in sorted {
            let item = &items[idx];
            self.draw_item(sugarloaf, item, layout, theme);
        }
    }

    fn draw_item(
        &self,
        sugarloaf: &mut Sugarloaf,
        item: &InlineDiagnosticItem,
        layout: InlineDiagnosticsLayout,
        theme: &IdeTheme,
    ) {
        let inv = 1.0 / layout.scale_factor;
        let s = layout.chrome_scale.clamp(0.5, 3.0);
        let cell_w = layout.cell_width_px * inv;
        let cell_h = layout.cell_height_px * inv;
        let pane_left = layout.pane_left_px * inv;
        let pane_top = layout.visible_top_px * inv;
        let pane_w = layout.pane_width_px * inv;
        let pane_h = layout.pane_height_px * inv;
        let scroll_y = layout.editor_pixel_offset_y * inv;
        let y = pane_top + item.row as f32 * cell_h + scroll_y;
        if y + cell_h < pane_top || y > pane_top + pane_h {
            return;
        }

        let color = item.severity.color(theme);
        let clip = Some([pane_left, pane_top, pane_w, pane_h]);
        let font_size = (cell_h - 4.0 * s).clamp(9.5 * s, 12.5 * s);
        let text_opts = DrawOpts {
            font_size,
            color: theme.u8(color),
            bold: true,
            clip_rect: clip,
            ..DrawOpts::default()
        };

        let message = item
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if message.is_empty() {
            return;
        }

        let right_pad = 8.0 * s;
        let pane_right = pane_left + pane_w - right_pad;
        let min_lens_width = MIN_LENS_WIDTH_PX * s;
        let x = lens_x(pane_left, cell_w, item.text_end_col, layout.columns);
        let available = (pane_right - x).max(0.0);
        if available < min_lens_width {
            return;
        }

        let text_x = x + PAD_X * s;
        let message_max = (pane_right - PAD_X * s - text_x).max(0.0);
        let message = truncate_to_fit(sugarloaf, &message, message_max, &text_opts);
        if message.is_empty() {
            return;
        }
        let text_y = y + (cell_h - font_size) * 0.5 - 1.0 * s;
        // Severity-colored text ONLY — no chip background and no accent bar.
        // Reads like a Zed/VS Code inline hint (red error text floating after
        // the code) instead of a filled pill.
        sugarloaf
            .overlay_text_mut()
            .draw(text_x, text_y, &message, &text_opts);
    }
}

fn is_visible(item: &InlineDiagnosticItem, layout: InlineDiagnosticsLayout) -> bool {
    item.row >= 0 && item.row < layout.visible_rows as i32
}

fn lens_x(pane_left: f32, cell_w: f32, text_end_col: u32, columns: u32) -> f32 {
    let start_col = text_end_col
        .saturating_add(PADDING_CELLS)
        .max(MIN_LENS_COLUMN)
        .min(columns);
    pane_left + start_col as f32 * cell_w
}

fn truncate_to_fit(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    max_width: f32,
    opts: &DrawOpts,
) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if sugarloaf.overlay_text_mut().measure(text, opts) <= max_width {
        return text.to_string();
    }
    let suffix = "...";
    if sugarloaf.overlay_text_mut().measure(suffix, opts) > max_width {
        return String::new();
    }
    let char_count = text.chars().count();
    let mut low = 0usize;
    let mut high = char_count;
    while low < high {
        let mid = (low + high + 1) / 2;
        let mut candidate: String = text.chars().take(mid).collect();
        candidate.push_str(suffix);
        if sugarloaf.overlay_text_mut().measure(&candidate, opts) <= max_width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let mut out: String = text.chars().take(low).collect();
    out.push_str(suffix);
    out
}
