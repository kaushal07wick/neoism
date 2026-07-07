// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Decoration sprites (underlines, strikethrough).
//!
//! Pre-rasterizes underline/strikethrough sprites into the grayscale
//! atlas and emits them as regular `CellText` entries
//! (`ghostty/src/font/sprite/draw/special.zig`,
//! `ghostty/src/renderer/generic.zig:3074`). One sprite per
//! (style, cell_w, thickness) cached in the grid atlas. Z-order is
//! enforced by emit order — underlines before glyphs (draws under),
//! strikethrough after (draws on top).

use neoism_backend::sugarloaf::grid::{
    AtlasSlot, GlyphKey, GridRenderer, RasterizedGlyph,
};
use neoism_terminal_core::colors::term::TermColors;
use neoism_terminal_core::crosswords::square::Square;
use neoism_terminal_core::crosswords::style::StyleSet;

use crate::host::Renderer;
use crate::terminal::grid_emit::cell_color::{cell_fg, normalized_to_u8};

pub(super) use neoism_ui::terminal_grid_emit::{
    decoration_thickness, rasterize_decoration, underline_style_from_flags,
    DecorationStyle, DECORATION_FONT_ID_BASE,
};

/// Look up or insert a decoration sprite into the grid atlas. Key is
/// (decoration font_id sentinel, cell_w as glyph_id, thickness as
/// size_bucket) — the same cache that backs regular glyphs, so
/// decorations ride the grid's glyph-eviction policy for free.
pub(super) fn ensure_decoration_slot(
    grid: &mut GridRenderer,
    style: DecorationStyle,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
) -> Option<AtlasSlot> {
    let key = GlyphKey {
        font_id: DECORATION_FONT_ID_BASE + style as u32,
        glyph_id: cell_w,
        size_bucket: thickness as u16,
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some(slot);
    }
    let (bytes, w, h, bearing_y) = rasterize_decoration(style, cell_w, cell_h, thickness);
    grid.insert_glyph(
        key,
        RasterizedGlyph {
            width: w.min(u16::MAX as u32) as u16,
            height: h.min(u16::MAX as u32) as u16,
            bearing_x: 0,
            bearing_y,
            bytes: &bytes,
        },
    )
}

/// Decoration color: SGR 58 `underline_color` if set, else the cell's
/// computed fg. `generic.zig:2968`.
#[inline]
pub(super) fn decoration_color(
    sq: Square,
    style: &neoism_terminal_core::crosswords::style::Style,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
) -> [u8; 4] {
    if let Some(uc) = style.underline_color {
        normalized_to_u8(renderer.compute_color(&uc, style.flags, term_colors))
    } else {
        cell_fg(sq, style_set, renderer, term_colors)
    }
}
