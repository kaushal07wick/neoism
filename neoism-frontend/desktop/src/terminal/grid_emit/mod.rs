// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Translates terminal `Square` cells into `CellBg` / `CellText`
//! instances for the grid GPU renderer.
//!
//! `build_row_bg` is one CellBg per cell; `build_row_fg` does
//! **run-level shaping** so ligatures (`=>`, `!=`, `fi`) form
//! correctly — a contiguous run of cells sharing `(font_id,
//! style_flags)` is shaped in one call, and one `CellText` is emitted
//! per resulting glyph (not per input cell).
//!
//! Shape + rasterize backends split by platform:
//! - **macOS**: CoreText via `font::macos::shape_text` /
//!   `rasterize_glyph`.
//! - **non-macOS**: swash `ShapeContext` + `ScaleContext`.
//!
//! Both populate the same `ShapedGlyph` shape and route into the same
//! `GridRenderer` atlases via the same emit loop.
//!
//! `font::shaper::run::RunIterator`.

mod bg;
mod cell_color;
mod cursor;
mod decoration;
mod fg;
mod glyph_raster;
mod hints;
mod run_shaping;
mod selection;

// Public surface — re-exported so external callers continue using
// `crate::terminal::grid_emit::Item` paths unchanged.
pub use bg::build_row_bg;
pub use cursor::{
    cursor_render_style, emit_cursor_sprite, CursorRenderInputs, CursorRenderStyle,
};
pub use fg::build_row_fg;
pub use hints::{row_hints_for, RowHint};
pub use run_shaping::GridGlyphRasterizer;
pub use selection::row_selection_for;
