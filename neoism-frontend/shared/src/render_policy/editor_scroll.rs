use super::*;

/// Spring-quantized editor scroll offset, split into an integer source
/// row stride and a fractional pixel residual.
///
/// `source_line_offset` is the integer number of source rows the
/// viewport has shifted (positive scrolls forward). `pixel_offset_y`
/// is the residual sub-row offset in physical pixels, already rounded
/// to whole pixels so the GPU shader uniform path doesn't introduce
/// sub-pixel "swim".
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditorScrollRenderOffset {
    pub source_line_offset: i32,
    pub pixel_offset_y: f32,
}

/// Retained render state for one editor grid between frames.
///
/// Desktop stores this beside its resident GPU grid. Web stores the
/// same POD beside the daemon-fed snapshot so both hosts decide source
/// movement, cursor/cursorline position, and pixel residual changes
/// from the same shape instead of carrying parallel tuples.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditorScrollGridRenderState {
    pub source_line_offset: i32,
    pub pixel_offset_y: f32,
    pub scrollback_origin: Option<isize>,
}

impl EditorScrollGridRenderState {
    pub fn new(
        offset: EditorScrollRenderOffset,
        scrollback_origin: Option<isize>,
    ) -> Self {
        Self {
            source_line_offset: offset.source_line_offset,
            pixel_offset_y: offset.pixel_offset_y,
            scrollback_origin,
        }
    }

    pub fn source_base(self) -> i64 {
        editor_scroll_effective_source_base(
            self.scrollback_origin,
            self.source_line_offset,
        )
    }
}

/// Convert a smooth-scroll spring + elastic rubber-band offset into a
/// Neovide/Ghostty-style row + pixel split.
///
/// The spring position is in lines (floats allowed for sub-row
/// animation). We `floor()` the spring to pick the source-row stride
/// and let the fractional remainder become the pixel offset; that
/// bundles each row-crossing into a single SHIFT plan instead of
/// dragging across many uniform-only frames.
///
/// `previous_source_line_offset` is accepted for call-site parity with
/// the historical native helper but is currently unused — the floor
/// split is stateless. Keeping the parameter avoids reshuffling every
/// caller if hysteresis returns.
pub fn editor_scroll_render_offset(
    scroll_position_lines: f32,
    elastic_offset_y: f32,
    cell_h: f32,
    previous_source_line_offset: Option<i32>,
) -> EditorScrollRenderOffset {
    if cell_h <= 0.0 || !cell_h.is_finite() {
        return EditorScrollRenderOffset::default();
    }
    let line_position = scroll_position_lines;
    if !line_position.is_finite() {
        return EditorScrollRenderOffset::default();
    }

    let source_line_offset = line_position.floor() as i32;
    let _ = previous_source_line_offset;
    // Quantize the spring offset to integer pixels so the bg-cell
    // shader lookup and the glyph origin agree on cell boundaries.
    // Sub-pixel float offsets caused text-vs-bg "swim" during
    // continuous scroll on Linux/Vulkan; rounding here keeps the
    // editor smooth-scroll path in sync with the terminal pane's
    // `offset.abs().ceil()` step.
    let pixel_offset_y = (((source_line_offset as f32 - line_position) * cell_h)
        + elastic_offset_y)
        .round();

    EditorScrollRenderOffset {
        source_line_offset,
        pixel_offset_y,
    }
}

/// Convert scroll spring position for an already-mutated daemon grid
/// snapshot. Native desktop paints from a retained grid/ring and uses
/// the Neovide floor split above. Web daemon snapshots have already
/// applied nvim's `grid_scroll`, so positive offsets need the mirrored
/// row split: keep sampling the previous visible row (`ceil`) while
/// easing the new top row into view from above.
pub fn editor_scroll_render_offset_for_mutated_snapshot(
    scroll_position_lines: f32,
    elastic_offset_y: f32,
    cell_h: f32,
    previous_source_line_offset: Option<i32>,
) -> EditorScrollRenderOffset {
    if cell_h <= 0.0 || !cell_h.is_finite() {
        return EditorScrollRenderOffset::default();
    }
    let line_position = scroll_position_lines;
    if !line_position.is_finite() {
        return EditorScrollRenderOffset::default();
    }

    let source_line_offset = if line_position > 0.0 {
        line_position.ceil() as i32
    } else {
        line_position.floor() as i32
    };
    let _ = previous_source_line_offset;
    let pixel_offset_y = (((source_line_offset as f32 - line_position) * cell_h)
        + elastic_offset_y)
        .round();

    EditorScrollRenderOffset {
        source_line_offset,
        pixel_offset_y,
    }
}

/// Combine the nvim scrollback ring origin with the spring's integer
/// source offset into a single "effective" physical source base. The
/// SHIFT/REBUILD plan is keyed off this base, not either term alone,
/// so held-key scrolls that move the ring and the spring in opposite
/// directions still compute a zero-delta plan when they cancel out.
pub fn editor_scroll_effective_source_base(
    scrollback_origin: Option<isize>,
    source_line_offset: i32,
) -> i64 {
    scrollback_origin.unwrap_or(0) as i64 + source_line_offset as i64
}

/// Source-base + pixel-offset deltas between consecutive frames. The
/// previous tuple captures the per-grid `EditorScrollGridState` the
/// native frontend stores; web hosts pass their own equivalent.
///
/// Returns `(source_changed, pixel_changed)`.
pub fn editor_scroll_state_changes(
    previous_source_line_offset: Option<i32>,
    previous_scrollback_origin: Option<isize>,
    previous_pixel_offset_y: Option<f32>,
    current: EditorScrollRenderOffset,
    current_scrollback_origin: Option<isize>,
) -> (bool, bool) {
    let current_source_base = editor_scroll_effective_source_base(
        current_scrollback_origin,
        current.source_line_offset,
    );
    let source_changed = match previous_source_line_offset {
        Some(prev_slo) => {
            editor_scroll_effective_source_base(previous_scrollback_origin, prev_slo)
                != current_source_base
        }
        None => current.source_line_offset != 0,
    };
    let pixel_changed = match previous_pixel_offset_y {
        Some(prev_pixel) => (prev_pixel - current.pixel_offset_y).abs() > f32::EPSILON,
        None => current.pixel_offset_y.abs() > f32::EPSILON,
    };
    (source_changed, pixel_changed)
}

/// Same decision as [`editor_scroll_state_changes`], but with the
/// retained shared state object instead of separate host-local fields.
pub fn editor_scroll_render_state_changes(
    previous: Option<EditorScrollGridRenderState>,
    current: EditorScrollRenderOffset,
    current_scrollback_origin: Option<isize>,
) -> (bool, bool) {
    editor_scroll_state_changes(
        previous.map(|state| state.source_line_offset),
        previous.and_then(|state| state.scrollback_origin),
        previous.map(|state| state.pixel_offset_y),
        current,
        current_scrollback_origin,
    )
}

/// What to do with the GPU's resident editor rows when the source
/// base has moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorScrollSourcePlan {
    /// No source-base movement — keep resident rows as-is.
    None,
    /// Shift resident rows by `delta` (signed) and rewrite the rows in
    /// `exposed` (output-row range, end-exclusive).
    Shift { delta: i32, exposed: (usize, usize) },
    /// Source moved by more than the visible window; rebuild all rows.
    RebuildAll,
}

/// Pick between SHIFT and REBUILD given the previous and current
/// effective source bases.
pub fn editor_scroll_source_plan(
    previous_source_base: Option<i64>,
    current_source_base: i64,
    visible_rows: usize,
) -> EditorScrollSourcePlan {
    let previous_source_base = previous_source_base.unwrap_or(0);
    let delta = current_source_base - previous_source_base;
    if delta == 0 {
        return EditorScrollSourcePlan::None;
    }

    if visible_rows == 0 {
        return EditorScrollSourcePlan::None;
    }

    let amount = delta.unsigned_abs().min(usize::MAX as u64) as usize;
    if amount >= visible_rows {
        return EditorScrollSourcePlan::RebuildAll;
    }

    let exposed = if delta > 0 {
        (visible_rows - amount, visible_rows)
    } else {
        (0, amount)
    };

    EditorScrollSourcePlan::Shift {
        delta: delta.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        exposed,
    }
}

/// How many GPU rows the SHIFT plan would leave untouched. Used by
/// the scroll FPS log to verify SHIFT minimizes per-frame rewrites.
pub fn editor_scroll_shifted_row_count(
    plan: EditorScrollSourcePlan,
    visible_rows: usize,
) -> u32 {
    match plan {
        EditorScrollSourcePlan::Shift { delta, .. } => visible_rows
            .saturating_sub(delta.unsigned_abs() as usize)
            .min(u32::MAX as usize)
            as u32,
        EditorScrollSourcePlan::None | EditorScrollSourcePlan::RebuildAll => 0,
    }
}

/// Shared per-frame editor scroll decision. Hosts still own the row
/// emit/copy side effects, but this carries the state transition that
/// decides whether resident rows can shift or must be rebuilt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorScrollFramePlan {
    pub current_source_base: i64,
    pub previous_source_base: Option<i64>,
    pub source_changed: bool,
    pub pixel_changed: bool,
    pub scrollback_origin_changed: bool,
    pub source_line_jump: bool,
    pub origin_jump_without_source_shift: bool,
    pub source_plan: EditorScrollSourcePlan,
    pub force_full: bool,
}

/// Build the desktop/web editor-grid rebuild plan from the retained
/// shared scroll state. `grid_or_damage_full` represents host-owned
/// full-rebuild causes such as newly-created GPU buffers, resize, or
/// terminal damage == Full.
pub fn editor_scroll_frame_plan(
    previous: Option<EditorScrollGridRenderState>,
    current: EditorScrollRenderOffset,
    current_scrollback_origin: Option<isize>,
    visible_rows: usize,
    grid_or_damage_full: bool,
) -> EditorScrollFramePlan {
    let current_source_base = editor_scroll_effective_source_base(
        current_scrollback_origin,
        current.source_line_offset,
    );
    let previous_source_base = previous.map(|state| state.source_base());
    let (source_changed, pixel_changed) =
        editor_scroll_render_state_changes(previous, current, current_scrollback_origin);
    let scrollback_origin_changed = previous
        .map(|state| state.scrollback_origin)
        .unwrap_or(None)
        != current_scrollback_origin;
    // The retained editor grid is keyed by the effective source base:
    // scrollback ring origin + spring integer row offset. During steady
    // nvim scrolling those two terms intentionally move against each
    // other. Looking at the raw spring row alone marks normal scroll as
    // a "jump" and forces full visible-row rebuilds even when the
    // effective source base is unchanged.
    let source_line_jump = previous_source_base
        .map(|previous| (previous - current_source_base).abs() > 1)
        .unwrap_or(false);
    let source_plan = if !grid_or_damage_full {
        editor_scroll_source_plan(previous_source_base, current_source_base, visible_rows)
    } else {
        EditorScrollSourcePlan::None
    };
    let origin_jump_without_source_shift = scrollback_origin_changed
        && previous_source_base
            .map(|prev| prev == current_source_base)
            .unwrap_or(false);
    let force_full = grid_or_damage_full
        || matches!(source_plan, EditorScrollSourcePlan::RebuildAll)
        || source_line_jump;

    EditorScrollFramePlan {
        current_source_base,
        previous_source_base,
        source_changed,
        pixel_changed,
        scrollback_origin_changed,
        source_line_jump,
        origin_jump_without_source_shift,
        source_plan,
        force_full,
    }
}

/// Should the top/bottom edge slots of the scrollback ring be
/// re-emitted this frame? They need refreshing whenever the rendered
/// frame was forced full, the source base shifted (so an exposed row
/// landed at the edge), or the integer-pixel offset moved (sub-row
/// glide entered/exited the residual).
pub fn editor_scroll_edge_rows_need_update(
    force_full: bool,
    rebuilt_rows: u32,
    source_changed: bool,
    pixel_changed: bool,
) -> bool {
    force_full || rebuilt_rows > 0 || source_changed || pixel_changed
}

/// Consume a `win_viewport.scroll_delta` that corresponds to a
/// `grid_scroll` animation already seeded by the daemon snapshot path.
/// Some hosts report the viewport delta with the opposite sign for the
/// upward path; de-dupe by magnitude so web does not apply a second,
/// opposite spring kick after it already animated the grid scroll.
pub fn editor_consume_pending_grid_scroll_animation(
    pending_rows: i32,
    viewport_rows: i32,
) -> (i32, i32) {
    if pending_rows == 0 || viewport_rows == 0 {
        return (pending_rows, viewport_rows);
    }
    let consumed_abs = pending_rows.abs().min(viewport_rows.abs());
    let next_pending = pending_rows.saturating_sub(consumed_abs * pending_rows.signum());
    let remaining_viewport =
        viewport_rows.saturating_sub(consumed_abs * viewport_rows.signum());
    (next_pending, remaining_viewport)
}

/// Invert the visible-row -> source-row mapping for cursor/cursorline
/// overlays. The render path samples `source_y = output_y +
/// source_line_offset` per visible row; the cursor row reported by
/// nvim is a live source row, so we subtract to map it back to its
/// output row.
pub fn editor_cursor_output_row(cursor_row: i32, source_line_offset: i32) -> i32 {
    editor_output_row_for_source(cursor_row, source_line_offset)
}

/// GPU grid row for an editor cursor sprite/uniform. Editor panes keep
/// hidden rows around the visible viewport for fractional scroll, so
/// the cursor's visible output row is shifted by `buffer_above` and
/// clamped to the total resident grid height.
pub fn editor_cursor_grid_row(
    cursor_row: i32,
    source_line_offset: i32,
    visible_rows: u32,
    buffer_above: u32,
    buffer_below: u32,
) -> u32 {
    let raw = editor_cursor_output_row(cursor_row, source_line_offset)
        .saturating_add(buffer_above.min(i32::MAX as u32) as i32);
    let max = grid_total_row_count(visible_rows, buffer_above, buffer_below)
        .saturating_sub(1)
        .min(i32::MAX as u32) as i32;
    raw.clamp(0, max) as u32
}

/// Lower-level inverse of the scroll-offset row sampler.
pub fn editor_output_row_for_source(source_y: i32, source_line_offset: i32) -> i32 {
    source_y - source_line_offset
}

/// Source row that should be sampled for one visible editor output
/// row. Desktop's grid renderer applies the same relation when it
/// fills resident GPU rows: the source index is the output row plus
/// the integer scroll stride, while the fractional residual stays a
/// pixel uniform.
pub fn editor_source_row_for_output(output_row: i32, source_line_offset: i32) -> i32 {
    output_row + source_line_offset
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorVisibleRowSource {
    Current(usize),
    Scrollback(usize),
    AboveEdge(usize),
    BelowEdge(usize),
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorVisibleRowSample {
    pub source_row: i32,
    pub source: EditorVisibleRowSource,
}

/// Resolve one visible editor output row to the retained row source
/// that should be sampled. The current grid covers the live visible
/// rows; edge caches cover short rows that nvim scrolled out before
/// the daemon snapshot was mutated. The previous snapshot is only a
/// fallback for older hosts without edge captures; it must not win
/// over edge rows or adjacent scrolls wrap the opposite side of the
/// viewport into view.
pub fn editor_visible_row_sample(
    output_row: i32,
    source_line_offset: i32,
    visible_rows: usize,
    has_scrollback_grid: bool,
    above_edge_rows: usize,
    below_edge_rows: usize,
) -> EditorVisibleRowSample {
    let source_row = editor_source_row_for_output(output_row, source_line_offset);
    let visible_rows_i32 = visible_rows.min(i32::MAX as usize) as i32;
    let source = if (0..visible_rows_i32).contains(&source_row) {
        EditorVisibleRowSource::Current(source_row as usize)
    } else if source_row < 0 {
        let edge = edge_above_row(source_row, above_edge_rows);
        if !matches!(edge, EditorVisibleRowSource::Missing) {
            edge
        } else if has_scrollback_grid {
            let prev_row = visible_rows_i32 + source_row;
            if (0..visible_rows_i32).contains(&prev_row) {
                EditorVisibleRowSource::Scrollback(prev_row as usize)
            } else {
                EditorVisibleRowSource::Missing
            }
        } else {
            EditorVisibleRowSource::Missing
        }
    } else {
        let edge = edge_below_row(source_row, visible_rows_i32, below_edge_rows);
        if !matches!(edge, EditorVisibleRowSource::Missing) {
            edge
        } else if has_scrollback_grid {
            let prev_row = source_row - visible_rows_i32;
            if (0..visible_rows_i32).contains(&prev_row) {
                EditorVisibleRowSource::Scrollback(prev_row as usize)
            } else {
                EditorVisibleRowSource::Missing
            }
        } else {
            EditorVisibleRowSource::Missing
        }
    };
    EditorVisibleRowSample { source_row, source }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorGridHitCell {
    pub row: u32,
    pub col: u32,
}

/// Map a pointer pixel to the editor source cell currently painted at
/// that pixel. This is the inverse of the shared editor-grid paint
/// transform (`row * cell_h + pixel_offset_y`, then
/// `output_row + source_line_offset`).
pub fn editor_grid_hit_cell(
    pointer_x: f32,
    pointer_y: f32,
    panel_rect: [f32; 4],
    columns: u32,
    rows: u32,
    cell_w: f32,
    cell_h: f32,
    scroll_state: EditorScrollGridRenderState,
) -> Option<EditorGridHitCell> {
    if columns == 0 || rows == 0 || cell_w <= 0.0 || cell_h <= 0.0 {
        return None;
    }
    if !pointer_x.is_finite()
        || !pointer_y.is_finite()
        || !cell_w.is_finite()
        || !cell_h.is_finite()
        || !panel_rect.iter().all(|v| v.is_finite())
    {
        return None;
    }
    let [x, y, w, h] = panel_rect;
    if pointer_x < x || pointer_x > x + w || pointer_y < y || pointer_y > y + h {
        return None;
    }

    let col = ((pointer_x - x) / cell_w).floor() as i32;
    let output_row =
        ((pointer_y - y - scroll_state.pixel_offset_y) / cell_h).floor() as i32;
    let source_row =
        editor_source_row_for_output(output_row, scroll_state.source_line_offset);
    if source_row < 0 || col < 0 {
        return None;
    }
    let row = (source_row as u32).min(rows.saturating_sub(1));
    let col = (col as u32).min(columns.saturating_sub(1));
    Some(EditorGridHitCell { row, col })
}

fn edge_above_row(source_row: i32, above_edge_rows: usize) -> EditorVisibleRowSource {
    let rows = above_edge_rows.min(i32::MAX as usize) as i32;
    let idx = rows + source_row;
    if (0..rows).contains(&idx) {
        EditorVisibleRowSource::AboveEdge(idx as usize)
    } else {
        EditorVisibleRowSource::Missing
    }
}

fn edge_below_row(
    source_row: i32,
    visible_rows: i32,
    below_edge_rows: usize,
) -> EditorVisibleRowSource {
    let rows = below_edge_rows.min(i32::MAX as usize) as i32;
    let idx = source_row - visible_rows;
    if (0..rows).contains(&idx) {
        EditorVisibleRowSource::BelowEdge(idx as usize)
    } else {
        EditorVisibleRowSource::Missing
    }
}

/// Physical-pixel pane margin for the grid panel host. Mirrors
/// sugarloaf's `SugarloafLayout::margin.top/left/right/bottom` after
/// scale-factor application; we keep it as a POD here so policy code
/// doesn't depend on the sugarloaf layout type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScaledMargin {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}
const _: () = {
    // Keep ScaledMargin small/POD — only changes if we add fields.
    assert!(std::mem::size_of::<ScaledMargin>() == 16);
};

impl ScaledMargin {
    /// Construct a `ScaledMargin` from a `(top, right, bottom, left)`
    /// tuple — the canonical CSS-style ordering used by
    /// `neoism_backend::config::layout::Margin`. Lifted from
    /// `screen/render/mod.rs` where the same 4-field reorder was
    /// duplicated at every site that built the policy input.
    pub const fn from_trbl(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            left,
            right,
            bottom,
        }
    }
}

/// POD layout slice for a grid panel: the on-screen panel rectangle in
/// physical pixels (`[x, y, w, h]`), the cell width/height already
/// rounded to whole pixels with a 1px floor, and the grid column count.
/// All three values match what the GPU cell pipeline uses, so policy
/// outputs sit on the same pixel lattice as the rendered glyphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridPanelGeometry {
    pub panel_rect: [f32; 4],
    pub scaled_margin: ScaledMargin,
    pub cell_width: f32,
    pub cell_height: f32,
    pub columns: u32,
}

/// Editor-grid scroll state needed for cursor/cursorline target math.
/// Already-resolved (no Mutex inside) so policy stays lock-free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorScrollState {
    pub scroll_position_lines: f32,
    pub elastic_offset_y: f32,
    pub previous_source_line_offset: Option<i32>,
}

/// Inputs to the terminal-grid trail-cursor planner.
///
/// `visible_rows` is the number of physically-visible rows the
/// terminal exposes (`terminal.screen_lines()` on native, falling back
/// to `dimension.lines`). It's a POD so the policy doesn't need to
/// acquire the terminal lock on the host side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailCursorPlanInput {
    pub geometry: GridPanelGeometry,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub visible_rows: f32,
    /// `Some(scroll)` when the active pane hosts an editor grid (the
    /// trail snaps through the spring-quantized scroll offset); `None`
    /// for a raw terminal pane (no editor scroll math).
    pub editor_scroll: Option<EditorScrollState>,
    /// Identifier of the cursor cell the previous frame's trail
    /// destination was set against. When `Some` and equal to the
    /// current `(rich_text_id, cursor_row, cursor_col)`, the host
    /// should call `set_destination_no_jump`; otherwise
    /// `set_destination`. `rich_text_id` is opaque to the policy.
    pub last_editor_trail_cursor_cell: Option<(usize, usize, usize)>,
    /// Active pane's `rich_text_id`, used to compose the cell key the
    /// host stores back as `last_editor_trail_cursor_cell`.
    pub rich_text_id: usize,
}

/// What the host should pass to `set_destination` /
/// `set_destination_no_jump` after the policy collapses pane bounds +
/// editor-scroll spring into a single physical-pixel destination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailCursorDestination {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// `true` -> host should call `set_destination_no_jump` (this is
    /// the same cell as last frame, the move is scroll-driven so the
    /// corner ranking must not snap); `false` -> `set_destination`.
    pub no_jump: bool,
    /// Echo of the cell key the host should remember for next frame.
    /// `None` when the pane is not an editor grid (raw terminal panes
    /// don't track jump suppression).
    pub next_last_cell: Option<(usize, usize, usize)>,
}

/// Compute the trail cursor's destination for a terminal/editor grid
/// pane. Pure math — no Sugarloaf, no terminal lock. The host applies
/// the returned destination to its `TrailCursor` (or equivalent).
///
/// Mirrors the historical inline math in
/// `frontends/neoism/src/screen/render/mod.rs` for the
/// `TrailCursorOverlayTarget::TerminalGrid` branch, including the
/// `pane_top_phys` clamp that keeps a phantom cursor from flying into
/// the chrome band during a smooth-scroll spring.
pub fn terminal_grid_trail_cursor_destination(
    input: TrailCursorPlanInput,
) -> TrailCursorDestination {
    terminal_grid_trail_cursor_destination_inner(input, false)
}

pub fn terminal_grid_trail_cursor_destination_for_mutated_snapshot(
    input: TrailCursorPlanInput,
) -> TrailCursorDestination {
    terminal_grid_trail_cursor_destination_inner(input, true)
}

fn terminal_grid_trail_cursor_destination_inner(
    input: TrailCursorPlanInput,
    mutated_snapshot: bool,
) -> TrailCursorDestination {
    let TrailCursorPlanInput {
        geometry,
        cursor_row,
        cursor_col,
        visible_rows,
        editor_scroll,
        last_editor_trail_cursor_cell,
        rich_text_id,
    } = input;

    let cell_width = geometry.cell_width;
    let cell_height = geometry.cell_height;
    let origin_x = geometry.panel_rect[0] + geometry.scaled_margin.left;
    let pane_top_phys = geometry.panel_rect[1] + geometry.scaled_margin.top;

    let cursor_px_x = origin_x + cursor_col as f32 * cell_width;
    let mut cursor_px_y = match editor_scroll {
        Some(scroll) => {
            let scroll_offset = if mutated_snapshot {
                editor_scroll_render_offset_for_mutated_snapshot(
                    scroll.scroll_position_lines,
                    scroll.elastic_offset_y,
                    cell_height,
                    scroll.previous_source_line_offset,
                )
            } else {
                editor_scroll_render_offset(
                    scroll.scroll_position_lines,
                    scroll.elastic_offset_y,
                    cell_height,
                    scroll.previous_source_line_offset,
                )
            };
            pane_top_phys
                + editor_cursor_output_row(
                    cursor_row as i32,
                    scroll_offset.source_line_offset,
                ) as f32
                    * cell_height
                + scroll_offset.pixel_offset_y
        }
        None => pane_top_phys + cursor_row as f32 * cell_height,
    };

    // Clamp the trail destination to the visible pane in physical
    // pixels. Without this the scroll spring's residual would push the
    // trail past the pane bottom into the chrome band as a phantom
    // cursor.
    let pane_bottom_phys = pane_top_phys + visible_rows * cell_height;
    let trail_top_min = pane_top_phys;
    let trail_top_max = (pane_bottom_phys - cell_height).max(pane_top_phys);
    cursor_px_y = cursor_px_y.clamp(trail_top_min, trail_top_max);

    let (no_jump, next_last_cell) = match editor_scroll {
        Some(_) => {
            let cell = (rich_text_id, cursor_row, cursor_col);
            (last_editor_trail_cursor_cell == Some(cell), Some(cell))
        }
        None => (false, None),
    };

    TrailCursorDestination {
        x: cursor_px_x,
        y: cursor_px_y,
        width: cell_width,
        height: cell_height,
        no_jump,
        next_last_cell,
    }
}

/// Inputs to the editor-grid cursorline planner.
///
/// The cursorline overlay only paints on editor grids — non-editor
/// panes return `None` from `editor_cursorline_plan` so the host can
/// skip the section entirely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorlinePlanInput {
    pub geometry: GridPanelGeometry,
    pub cursor_row: i32,
    pub visible_rows: f32,
    pub editor_scroll: EditorScrollState,
}

/// What the host should hand to its `CursorlineOverlay::set_target`
/// and `render` calls. `editor_scroll_animating` tells the overlay to
/// snap to the grid instead of running its own hover-glide spring; the
/// hover glide is reserved for pure cursor-row jumps without scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorlinePlan {
    pub pane_x: f32,
    pub pane_w: f32,
    pub target_y: f32,
    pub cell_height: f32,
    pub editor_scroll_animating: bool,
}

/// Compute the animated cursorline overlay target for an editor grid.
///
/// The target y matches the trail cursor's destination y (they share a
/// row), so the tinted line and the cursor block glide to the same
/// place together instead of the row background snapping per cursor
/// move. Pane width spans the full editable column band — not the
/// scrollbar/minimap chrome — matching nvim's built-in `cursorline`.
pub fn editor_cursorline_plan(input: CursorlinePlanInput) -> CursorlinePlan {
    editor_cursorline_plan_inner(input, false)
}

pub fn editor_cursorline_plan_for_mutated_snapshot(
    input: CursorlinePlanInput,
) -> CursorlinePlan {
    editor_cursorline_plan_inner(input, true)
}

fn editor_cursorline_plan_inner(
    input: CursorlinePlanInput,
    mutated_snapshot: bool,
) -> CursorlinePlan {
    let CursorlinePlanInput {
        geometry,
        cursor_row,
        visible_rows,
        editor_scroll,
    } = input;

    let cell_height = geometry.cell_height;
    let cell_width = geometry.cell_width;
    let pane_top_phys = geometry.panel_rect[1] + geometry.scaled_margin.top;
    let pane_x = geometry.panel_rect[0] + geometry.scaled_margin.left;
    let pane_w = (geometry.columns as f32 * cell_width).max(0.0);

    let scroll_offset = if mutated_snapshot {
        editor_scroll_render_offset_for_mutated_snapshot(
            editor_scroll.scroll_position_lines,
            editor_scroll.elastic_offset_y,
            cell_height,
            editor_scroll.previous_source_line_offset,
        )
    } else {
        editor_scroll_render_offset(
            editor_scroll.scroll_position_lines,
            editor_scroll.elastic_offset_y,
            cell_height,
            editor_scroll.previous_source_line_offset,
        )
    };

    let target_y = pane_top_phys
        + editor_cursor_output_row(cursor_row, scroll_offset.source_line_offset) as f32
            * cell_height
        + scroll_offset.pixel_offset_y;
    let max_target_y =
        pane_top_phys + (visible_rows.max(1.0).floor() - 1.0) * cell_height;
    let target_y = target_y.clamp(pane_top_phys, max_target_y);

    let editor_scroll_animating = scroll_offset.source_line_offset != 0
        || scroll_offset.pixel_offset_y.abs() > f32::EPSILON;

    CursorlinePlan {
        pane_x,
        pane_w,
        target_y,
        cell_height,
        editor_scroll_animating,
    }
}
