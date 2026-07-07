//! Desktop compatibility wrappers for shared terminal-grid hint policy.

use crate::host::Renderer;

pub use neoism_ui::terminal_grid_emit::{row_hints_for, HintTag, RowHint};

#[inline]
pub(super) fn cell_in_row_hints(row_hints: &[RowHint], col: u16) -> Option<HintTag> {
    neoism_ui::terminal_grid_emit::cell_in_row_hints(row_hints, col)
}

#[inline]
pub(super) fn cell_in_hover_underline(row_hints: &[RowHint], col: u16) -> bool {
    neoism_ui::terminal_grid_emit::cell_in_hover_underline(row_hints, col)
}

#[inline]
pub(super) fn cell_fg_hinted(tag: HintTag, renderer: &Renderer) -> [u8; 4] {
    let color = neoism_ui::terminal_grid_emit::hint_foreground(
        tag,
        renderer.named_colors.search_match_foreground,
        renderer.named_colors.search_focused_match_foreground,
    );
    neoism_ui::terminal_grid_emit::rgba_f32_to_u8(color)
}
