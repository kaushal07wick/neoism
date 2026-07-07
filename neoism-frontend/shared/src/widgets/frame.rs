// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Reusable rounded-panel frame.
//!
//! Most chrome panels (file_tree, command palette, finder, diagnostics)
//! paint themselves as a `surface`-colored outer rect with a `bg`-colored
//! inner rect inset by a hairline border, with edge-aware corner radii so
//! the frame can sit flush against a window edge without rounding into
//! empty space. This widget centralizes that pattern.
//!
//! The widget intentionally does NOT clip or paint row content — callers
//! still compute their own content rect (`inner_rect`) and paint inside
//! it. Pair with [`crate::primitives::edge_row_radii`] for
//! selected-row corners that meet the frame cleanly.

use sugarloaf::Sugarloaf;

/// Which of the four outer corners get rounded. The unrounded corners
/// sit flush against whatever edge they're adjacent to (window edge,
/// neighbouring panel, etc.).
///
/// Only `Top` is exercised today (file_tree). Other variants are kept
/// for the upcoming command_palette / finder / diagnostics migrations.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameCorners {
    /// All four corners rounded — used for floating overlays.
    All,
    /// Top-left + top-right only. file_tree style — the bottom edge is
    /// flush against the status bar.
    Top,
    /// Bottom-left + bottom-right only. Mirror of `Top`.
    Bottom,
    /// Top-left + bottom-left only. Used by panels flush against the
    /// right side of the window.
    Left,
    /// Top-right + bottom-right only. Mirror of `Left`.
    Right,
    /// No rounding (square frame).
    None,
}

impl FrameCorners {
    /// Per-corner radii in the `[tl, tr, br, bl]` clockwise order
    /// Sugarloaf expects.
    fn radii(self, radius: f32) -> [f32; 4] {
        match self {
            FrameCorners::All => [radius, radius, radius, radius],
            FrameCorners::Top => [radius, radius, 0.0, 0.0],
            FrameCorners::Bottom => [0.0, 0.0, radius, radius],
            FrameCorners::Left => [radius, 0.0, 0.0, radius],
            FrameCorners::Right => [0.0, radius, radius, 0.0],
            FrameCorners::None => [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameConfig {
    /// Outer ring color — usually `theme.surface`.
    pub outer_color: [f32; 4],
    /// Inner fill color — usually `theme.bg`.
    pub inner_color: [f32; 4],
    /// Outer corner radius (logical px). Inner radius is derived by
    /// subtracting `border_thickness` so the ring stays uniform.
    pub radius: f32,
    /// Border thickness (logical px). The inner rect is inset by this
    /// amount on all four sides.
    pub border_thickness: f32,
    /// Which outer corners to round.
    pub rounded_corners: FrameCorners,
}

/// Inner content rect for a frame at `rect` with the given `border_thickness`.
/// Use this to lay out content (rows, text, scrollbar) inside the frame.
///
/// Returns `[x, y, w, h]`. `w` and `h` are clamped at 0.
pub fn inner_rect(rect: [f32; 4], border_thickness: f32) -> [f32; 4] {
    let [x, y, w, h] = rect;
    [
        x + border_thickness,
        y + border_thickness,
        (w - border_thickness * 2.0).max(0.0),
        (h - border_thickness * 2.0).max(0.0),
    ]
}

fn inner_rect_for_corners(
    rect: [f32; 4],
    border_thickness: f32,
    rounded_corners: FrameCorners,
) -> [f32; 4] {
    let [x, y, w, h] = rect;
    match rounded_corners {
        // Top-attached panels sit flush on the status bar: no bottom
        // inset means no visible bottom stroke, while the side strokes
        // still run all the way down to the status seam.
        FrameCorners::Top => [
            x + border_thickness,
            y + border_thickness,
            (w - border_thickness * 2.0).max(0.0),
            (h - border_thickness).max(0.0),
        ],
        FrameCorners::Bottom => [
            x + border_thickness,
            y,
            (w - border_thickness * 2.0).max(0.0),
            (h - border_thickness).max(0.0),
        ],
        FrameCorners::Left => [
            x + border_thickness,
            y + border_thickness,
            (w - border_thickness).max(0.0),
            (h - border_thickness * 2.0).max(0.0),
        ],
        FrameCorners::Right => [
            x,
            y + border_thickness,
            (w - border_thickness).max(0.0),
            (h - border_thickness * 2.0).max(0.0),
        ],
        FrameCorners::All | FrameCorners::None => inner_rect(rect, border_thickness),
    }
}

/// Inner corner radius derived from outer radius and border thickness.
/// Use this when computing per-row radii via `edge_row_radii` so selected
/// rows meet the frame cleanly.
pub fn inner_radius(outer_radius: f32, border_thickness: f32) -> f32 {
    (outer_radius - border_thickness).max(0.0)
}

/// Paint a rounded panel frame: outer ring + inner fill.
///
/// `rect` is `[x, y, w, h]` in logical pixels. `order_outer` should be
/// strictly less than `order_inner` so the inner fill paints on top of
/// the ring.
pub fn draw_frame(
    sugarloaf: &mut Sugarloaf,
    rect: [f32; 4],
    config: &FrameConfig,
    depth: f32,
    order_outer: u8,
    order_inner: u8,
) {
    let [x, y, w, h] = rect;
    let outer_radii = config.rounded_corners.radii(config.radius);
    sugarloaf.quad(
        None,
        x,
        y,
        w,
        h,
        config.outer_color,
        outer_radii,
        depth,
        order_outer,
    );

    let inner =
        inner_rect_for_corners(rect, config.border_thickness, config.rounded_corners);
    let inner_r = inner_radius(config.radius, config.border_thickness);
    let inner_radii = config.rounded_corners.radii(inner_r);
    sugarloaf.quad(
        None,
        inner[0],
        inner[1],
        inner[2],
        inner[3],
        config.inner_color,
        inner_radii,
        depth,
        order_inner,
    );
}
