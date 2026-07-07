//! Minimal scrollbar helpers used by the completion popup.
//!
//! Mirrors the public surface of
//! `frontends/neoism/src/chrome/widgets/scrollbar.rs` that this panel
//! actually consumes (`compute_thumb`, `opacity_from_last_scroll`,
//! `draw_thumb`, and the `SCROLLBAR_WIDTH` / `SCROLLBAR_MARGIN`
//! constants). The full widget — drag state, hit-testing, terminal
//! integration — lives in the native shim; this file exists so the
//! cross-frontend composer can paint its popup scrollbar without
//! pulling in the rest of that machinery.

use web_time::Instant;

use sugarloaf::Sugarloaf;

// Layout. Kept `pub` so other UI elements (command palette, future
// overlays) can render a scrollbar that matches the terminal's exactly
// without duplicating the numbers.
pub const SCROLLBAR_WIDTH: f32 = 6.0;
pub const SCROLLBAR_MARGIN: f32 = 2.0;
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;

// Timing
pub const FADE_OUT_DELAY_MS: u128 = 2000;
pub const FADE_OUT_DURATION_MS: u128 = 300;

// Colors
pub const SCROLLBAR_COLOR: [f32; 4] = [0.6, 0.6, 0.6, 0.5];
pub const SCROLLBAR_DRAG_COLOR: [f32; 4] = [0.7, 0.7, 0.7, 0.7];

/// Fade-in/out opacity for a scrollbar given the timestamp of the most
/// recent scroll event (`None` = never scrolled). `dragging` pins it to
/// fully opaque so a slow drag doesn't fade out under the user's cursor.
///
/// Matches the terminal scrollbar's envelope:
/// - 0.0 before any scroll ever happened
/// - 1.0 for the first `FADE_OUT_DELAY_MS` after a scroll
/// - linear fade over `FADE_OUT_DURATION_MS` back to 0.0
pub fn opacity_from_last_scroll(last_scroll: Option<Instant>, dragging: bool) -> f32 {
    if dragging {
        return 1.0;
    }
    let last_scroll = match last_scroll {
        Some(t) => t,
        None => return 0.0,
    };
    let elapsed = Instant::now()
        .saturating_duration_since(last_scroll)
        .as_millis();
    if elapsed < FADE_OUT_DELAY_MS {
        1.0
    } else {
        let fade_elapsed = elapsed - FADE_OUT_DELAY_MS;
        if fade_elapsed >= FADE_OUT_DURATION_MS {
            0.0
        } else {
            1.0 - (fade_elapsed as f32 / FADE_OUT_DURATION_MS as f32)
        }
    }
}

/// Thumb geometry (y offset, height) inside a vertical track of
/// `track_height` anchored at `track_top`. Returns `None` when the
/// list fits entirely (`visible >= total`) — caller skips drawing.
///
/// `normalized_offset` is the scroll position in `[0.0, 1.0]` where
/// 0.0 = top (unscrolled) and 1.0 = maximum scroll. Callers that
/// think in "scroll from the top" (command palette) and callers that
/// think in "scroll back from live edge" (terminal history) both
/// plug into the same geometry by normalizing on their side.
///
/// Thumb height is clamped at `SCROLLBAR_MIN_THUMB_HEIGHT` so very
/// long lists don't shrink the thumb to a sub-pixel sliver.
pub fn compute_thumb(
    visible: usize,
    total: usize,
    track_top: f32,
    track_height: f32,
    normalized_offset: f32,
) -> Option<(f32, f32)> {
    if total <= visible || track_height <= 0.0 {
        return None;
    }
    let ratio = visible as f32 / total as f32;
    let thumb_height = (track_height * ratio)
        .clamp(SCROLLBAR_MIN_THUMB_HEIGHT.min(track_height), track_height);
    let scrollable = (track_height - thumb_height).max(0.0);
    let progress = normalized_offset.clamp(0.0, 1.0);
    Some((track_top + scrollable * progress, thumb_height))
}

/// Paint a single scrollbar thumb — the one and only way rio renders a
/// scrollbar. Uses `SCROLLBAR_COLOR` (or `SCROLLBAR_DRAG_COLOR` if
/// `dragging`) modulated by `opacity`. `opacity <= 0.0` is a no-op so
/// callers can pipe the fade helper straight in.
///
/// `depth` + `order` let callers place the thumb above their own
/// background layers: the terminal uses `TERMINAL_DEPTH` /
/// `TERMINAL_ORDER` so the bar lives on top of the cell content, the
/// command palette uses a higher order so the bar isn't swallowed by
/// the palette's backdrop/bg rects.
#[allow(clippy::too_many_arguments)]
pub fn draw_thumb(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    height: f32,
    opacity: f32,
    dragging: bool,
    depth: f32,
    order: u8,
) {
    if opacity <= 0.0 {
        return;
    }
    let base = if dragging {
        SCROLLBAR_DRAG_COLOR
    } else {
        SCROLLBAR_COLOR
    };
    let color = [base[0], base[1], base[2], base[3] * opacity];
    sugarloaf.rect(None, x, y, SCROLLBAR_WIDTH, height, color, depth, order);
}
