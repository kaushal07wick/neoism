// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Cursor sprite rasterization + emission.
//!
//! Cursor sprites live in a dedicated `font_id` sentinel range
//! (`CURSOR_FONT_ID_BASE`) so they share the grid atlas's glyph cache
//! and eviction policy with regular glyphs and decorations.

use neoism_backend::sugarloaf::grid::{
    AtlasSlot, CellText, GlyphKey, GridRenderer, RasterizedGlyph,
};

pub use neoism_ui::terminal_grid_emit::{
    cursor_render_style, CursorRenderInputs, CursorRenderStyle,
};

use neoism_ui::terminal_grid_emit::{
    cursor_thickness, rasterize_cursor, CursorSpriteStyle, CURSOR_FONT_ID_BASE,
};

/// Lookup or insert a cursor sprite. `size_bucket` packs `(thickness,
/// cell_h)` so a font-size or DPI change invalidates the cached
/// sprite. `cell_w` is the glyph_id so wide-cell sprites (CJK
/// double-width) get their own slot.
fn ensure_cursor_sprite_slot(
    grid: &mut GridRenderer,
    style: CursorSpriteStyle,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
) -> Option<AtlasSlot> {
    let key = GlyphKey {
        font_id: CURSOR_FONT_ID_BASE + style as u32,
        glyph_id: cell_w,
        size_bucket: ((thickness as u16 & 0xF) << 12) | (cell_h.min(0xFFF) as u16),
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some(slot);
    }
    let (bytes, w, h, bearing_x, bearing_y) =
        rasterize_cursor(style, cell_w, cell_h, thickness);
    grid.insert_glyph(
        key,
        RasterizedGlyph {
            width: w,
            height: h,
            bearing_x,
            bearing_y,
            bytes: &bytes,
        },
    )
}

/// Emit a cursor sprite into the appropriate `fg_rows` slot, and
/// clear the OTHER slot in the same call. Setting both slots here
/// (instead of having the caller pre-call `grid.clear_cursor()`) lets
/// the underlying `set_*_cursor` diff-check skip the dirty mark when
/// the cursor is bit-stable across frames — which is the steady state
/// during a smooth-scroll animation. `addCursor` ).
pub fn emit_cursor_sprite(
    grid: &mut GridRenderer,
    style: CursorRenderStyle,
    col: u16,
    row: u16,
    color: [u8; 4],
    cell_w: u32,
    cell_h: u32,
    pixel_offset_y: i32,
) {
    let sprite = style.sprite();
    let thickness = cursor_thickness(cell_h);
    let Some(slot) = ensure_cursor_sprite_slot(grid, sprite, cell_w, cell_h, thickness)
    else {
        return;
    };
    if slot.w == 0 || slot.h == 0 {
        return;
    }
    let cursor_cell = CellText {
        glyph_pos: [slot.x as u32, slot.y as u32],
        glyph_size: [slot.w as u32, slot.h as u32],
        bearings: [slot.bearing_x, slot.bearing_y],
        grid_pos: [col, row],
        color,
        atlas: CellText::ATLAS_GRAYSCALE,
        // Marks this as "the cursor itself" so the text shader's
        // fg-swap skips it (the sprite paints in `color` directly,
        // not in `cursor_color` from the uniforms).
        bools: CellText::BOOL_IS_CURSOR_GLYPH,
        _pad: [0, 0],
        pixel_offset_y,
    };
    if sprite.is_block_slot() {
        grid.set_block_cursor(&[cursor_cell]);
        grid.set_non_block_cursor(&[]);
    } else {
        grid.set_block_cursor(&[]);
        grid.set_non_block_cursor(&[cursor_cell]);
    }
}
