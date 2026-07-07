// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Background-row builder. Emits one `CellBg` per cell, applying
//! selection / hint overrides on top of the cell's own bg color.

use neoism_backend::sugarloaf::grid::CellBg;
use neoism_terminal_core::colors::term::TermColors;
use neoism_terminal_core::crosswords::grid::row::Row;
use neoism_terminal_core::crosswords::pos::Column;
use neoism_terminal_core::crosswords::square::Square;
use neoism_terminal_core::crosswords::style::StyleSet;
use neoism_ui::terminal_grid_emit::{cell_in_row_sel, RowSelection};

use crate::host::Renderer;
use crate::terminal::grid_emit::cell_color::{cell_bg, normalized_to_u8};
use crate::terminal::grid_emit::hints::{cell_in_row_hints, HintTag, RowHint};

#[allow(clippy::too_many_arguments)]
pub fn build_row_bg(
    row: &Row<Square>,
    cols: usize,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    pixel_offset_y: i32,
    bg_scratch: &mut Vec<CellBg>,
) {
    bg_scratch.clear();

    // Defensive clamp — `cols` comes from a snapshot of `dim.columns`
    // taken before `terminal.lock()`, while `row` comes from the lock
    // itself. A zoom-driven resize between those two reads can leave
    // the row narrower than `dim`, and indexing past the end would
    // panic. Render the row we actually have; the next frame's
    // damage pass will repaint with consistent dims.
    let cols = cols.min(row.len());

    // Fast path: row has no selection and no color-changing hints
    // (HyperlinkHover only contributes an underline, never bg). The
    // overwhelming majority of rows in idle terminals hit this path —
    // strip the per-cell `cell_in_row_sel` / `cell_in_row_hints`
    // checks and just walk cells.
    let has_sel = row_sel.is_some();
    let has_color_hints = row_hints.iter().any(|rh| rh.tag != HintTag::HyperlinkHover);
    if !has_sel && !has_color_hints {
        bg_scratch.reserve(cols);
        for x in 0..cols {
            let sq = row[Column(x)];
            bg_scratch.push(CellBg {
                rgba: cell_bg(sq, style_set, renderer, term_colors),
                pixel_offset_y,
            });
        }
        return;
    }

    // Slow path: selection and/or hint highlighting present.
    let sel_bg = if has_sel {
        Some(normalized_to_u8(renderer.named_colors.selection_background))
    } else {
        None
    };
    let (match_bg, focused_bg) = if has_color_hints {
        (
            Some(normalized_to_u8(
                renderer.named_colors.search_match_background,
            )),
            Some(normalized_to_u8(
                renderer.named_colors.search_focused_match_background,
            )),
        )
    } else {
        (None, None)
    };
    for x in 0..cols {
        let sq = row[Column(x)];
        let col = x as u16;
        let rgba = if cell_in_row_sel(row_sel, col) {
            // Selection bg wins over hint bg and the cell's own bg,
            // matching `generic.zig:2775-2800` (selection check
            // runs before highlight check).
            sel_bg.unwrap_or_else(|| cell_bg(sq, style_set, renderer, term_colors))
        } else if let Some(tag) = cell_in_row_hints(row_hints, col) {
            match tag {
                HintTag::Focused => focused_bg
                    .unwrap_or_else(|| cell_bg(sq, style_set, renderer, term_colors)),
                HintTag::Match => match_bg
                    .unwrap_or_else(|| cell_bg(sq, style_set, renderer, term_colors)),
                // `cell_in_row_hints` filters HyperlinkHover out, but
                // make the match exhaustive so a future caller can't
                // accidentally hit a panic.
                HintTag::HyperlinkHover => cell_bg(sq, style_set, renderer, term_colors),
            }
        } else {
            cell_bg(sq, style_set, renderer, term_colors)
        };
        bg_scratch.push(CellBg {
            rgba,
            pixel_offset_y,
        });
    }
}
