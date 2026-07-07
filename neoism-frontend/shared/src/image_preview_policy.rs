//! Inline image clipboard paste preview — shared decision policy.
//!
//! When the user pastes an image into a surface that doesn't have native
//! image rendering (web wasm terminal has no sixel/kitty graphics, the
//! desktop agent input shows `[image1]` tokens only), the frontends pop a
//! floating thumbnail near the cursor so the human can confirm "yes, this
//! is the image I expected" before committing the attachment.
//!
//! This module is the pure policy: how big the preview should be, where to
//! place it relative to the cursor, and which input events dismiss it.
//! Frontends own the actual painting (sugarloaf rect+image for native,
//! HTMLImageElement absolute-positioned div for web), but the geometry
//! and lifecycle decisions live here so a bug in the policy is fixed
//! once.
//!
//! Cf. `paste_policy.rs` — same split between policy + frontend driver.
//!
//! See task C4 in TASKS.md (image clipboard inline preview).

/// Hard cap on the longest preview edge, in logical pixels.
///
/// 256 px keeps the floating preview small enough to never cover an
/// editor block or a status line on a 1080p display, while still being
/// large enough to recognise the image content.
pub const PREVIEW_MAX_EDGE_PX: u32 = 256;

/// Hard cap on the shortest preview edge, in logical pixels.
///
/// 96 px is the smallest size where a thumbnail is still recognisable
/// for typical screenshot / icon images. Below this we'd be better off
/// showing a filename pill, but the inline preview is the whole point of
/// this UX.
pub const PREVIEW_MIN_EDGE_PX: u32 = 96;

/// Padding between the preview and the cursor, in logical pixels.
///
/// Keeps the preview from sitting directly on top of the caret so the
/// user can still see what they were typing when they triggered the
/// paste.
pub const PREVIEW_CURSOR_GAP_PX: f32 = 8.0;

/// Maximum age (in milliseconds) before the preview self-dismisses even
/// without an explicit input event. Long enough for the user to confirm
/// at a glance, short enough that the preview doesn't camp on screen
/// after they've moved on.
pub const PREVIEW_AUTO_DISMISS_MS: f32 = 4500.0;

/// Where the preview should sit relative to the cursor.
///
/// `Above`/`Below` carry the absolute y offset. The caller resolves which
/// side the cursor sits on the viewport from `decide_position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewSide {
    /// Preview rendered above the cursor (cursor near the bottom).
    Above,
    /// Preview rendered below the cursor (cursor near the top).
    Below,
}

/// Frame returned by [`decide_size`] — the logical-pixel rectangle the
/// frontend should paint the thumbnail into. Pixels are logical (caller
/// multiplies by `scale_factor` when handing to sugarloaf / canvas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewSize {
    pub width: u32,
    pub height: u32,
}

/// Decide the preview thumbnail size from the source image's intrinsic
/// dimensions.
///
/// Aspect ratio is preserved exactly; the longest edge is clamped to
/// [`PREVIEW_MAX_EDGE_PX`] and the shortest edge is floored at
/// [`PREVIEW_MIN_EDGE_PX`] (extreme aspect ratios bow toward the floor
/// before fitting back under the ceiling).
///
/// Returns `None` when the source has zero area — the frontends use this
/// as a "skip the preview, just attach silently" signal.
pub fn decide_size(src_w: u32, src_h: u32) -> Option<PreviewSize> {
    if src_w == 0 || src_h == 0 {
        return None;
    }
    let (w, h) = (src_w as f32, src_h as f32);
    let max_edge = PREVIEW_MAX_EDGE_PX as f32;
    let min_edge = PREVIEW_MIN_EDGE_PX as f32;

    // First pass: clamp the longest edge down to max_edge while
    // preserving aspect ratio.
    let scale_down = (max_edge / w.max(h)).min(1.0);
    let mut out_w = w * scale_down;
    let mut out_h = h * scale_down;

    // Second pass: if the result is smaller than min_edge on the shorter
    // side, scale up to floor it — but never let the longer side exceed
    // max_edge as a result. With a 1:8 aspect ratio this means the
    // shorter side may still be under min_edge; we accept that to keep
    // the preview bounded.
    let shorter = out_w.min(out_h);
    if shorter < min_edge && shorter > 0.0 {
        let scale_up = (min_edge / shorter).min(max_edge / out_w.max(out_h));
        out_w *= scale_up;
        out_h *= scale_up;
    }

    let width = out_w.round().max(1.0) as u32;
    let height = out_h.round().max(1.0) as u32;
    Some(PreviewSize { width, height })
}

/// Decide whether to place the preview above or below the cursor.
///
/// Heuristic: if there's enough vertical space below the cursor to fit
/// the preview, place below; otherwise place above. This matches
/// completion-menu policy (`panels::completion_menu`) — the preview
/// floats out of the cursor's eye-line in the direction with the most
/// breathing room.
///
/// `cursor_y` and `viewport_height` are in the same coordinate space
/// (logical pixels, viewport-relative). `preview_height` includes the
/// gap.
pub fn decide_position(
    cursor_y: f32,
    viewport_height: f32,
    preview_height: f32,
) -> PreviewSide {
    let below_room = (viewport_height - cursor_y).max(0.0);
    let above_room = cursor_y.max(0.0);
    let need = preview_height + PREVIEW_CURSOR_GAP_PX;
    if below_room >= need || below_room >= above_room {
        PreviewSide::Below
    } else {
        PreviewSide::Above
    }
}

/// Compute the top-left y offset (in viewport-local logical pixels) at
/// which the preview should be drawn, given the chosen side.
pub fn compute_y_offset(
    cursor_y: f32,
    cursor_line_h: f32,
    preview_height: f32,
    side: PreviewSide,
) -> f32 {
    match side {
        PreviewSide::Above => {
            (cursor_y - preview_height - PREVIEW_CURSOR_GAP_PX).max(0.0)
        }
        PreviewSide::Below => cursor_y + cursor_line_h + PREVIEW_CURSOR_GAP_PX,
    }
}

/// Clamp the preview's x position so it stays fully inside the viewport.
///
/// The preview's natural x is `cursor_x` (left-aligned with the caret).
/// If that would overflow the right edge, we slide the preview left so
/// its right edge sits at the viewport. We never let the preview slip
/// off the left edge.
pub fn clamp_x_to_viewport(
    cursor_x: f32,
    preview_width: f32,
    viewport_width: f32,
) -> f32 {
    let max_x = (viewport_width - preview_width).max(0.0);
    cursor_x.clamp(0.0, max_x)
}

/// Lifecycle outcome for an input event arriving while the preview is
/// visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissDecision {
    /// Preview should stay visible.
    Keep,
    /// User explicitly accepted (typically Enter/Tab) — commit the
    /// attachment and dismiss.
    Confirm,
    /// User dismissed (Escape, Backspace, or any other key) — discard
    /// the pending paste and dismiss.
    Cancel,
}

/// What the user did while the preview was visible.
///
/// The frontends translate their native key events into this enum
/// before calling [`decide_dismiss`] — keeping the policy free of
/// `winit::Key` / web `KeyboardEvent` shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewInput {
    /// Enter or Tab — confirm.
    Confirm,
    /// Escape — cancel.
    Escape,
    /// Any other character key — cancel (so typing keeps working).
    OtherKey,
    /// Mouse click outside the preview rect — cancel.
    ClickOutside,
    /// Mouse click inside the preview rect — confirm.
    ClickInside,
    /// Frame tick — checks the auto-dismiss timer.
    Tick { elapsed_ms: f32 },
}

/// Map a [`PreviewInput`] event to a [`DismissDecision`].
///
/// Pure, side-effect-free: callers stitch the resulting decision into
/// their own state (drop the attachment, send the bytes, etc.). Centred
/// here so the desktop and web frontends agree on what counts as a
/// confirm vs cancel.
pub fn decide_dismiss(input: PreviewInput) -> DismissDecision {
    match input {
        PreviewInput::Confirm => DismissDecision::Confirm,
        PreviewInput::ClickInside => DismissDecision::Confirm,
        PreviewInput::Escape => DismissDecision::Cancel,
        PreviewInput::OtherKey => DismissDecision::Cancel,
        PreviewInput::ClickOutside => DismissDecision::Cancel,
        PreviewInput::Tick { elapsed_ms } => {
            if elapsed_ms >= PREVIEW_AUTO_DISMISS_MS {
                DismissDecision::Cancel
            } else {
                DismissDecision::Keep
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_size_clamps_long_edge_to_max() {
        // A 4000x2000 screenshot must clamp to 256x128 (preserving 2:1).
        let s = decide_size(4000, 2000).expect("non-zero");
        assert_eq!(s.width, 256);
        assert_eq!(s.height, 128);
    }

    #[test]
    fn decide_size_preserves_aspect_ratio_when_under_max() {
        // 200x100 fits under max already; pass through.
        let s = decide_size(200, 100).expect("non-zero");
        // 200 width is bigger than min 96 on shorter axis (100), so no
        // upscaling kicks in.
        assert_eq!(s.width, 200);
        assert_eq!(s.height, 100);
    }

    #[test]
    fn decide_size_lifts_tiny_image_toward_min_edge() {
        // 32x16 should scale up so the shorter edge approaches 96.
        let s = decide_size(32, 16).expect("non-zero");
        assert!(s.height >= 96 || s.width >= PREVIEW_MIN_EDGE_PX);
        // Aspect ratio (2:1) preserved within rounding.
        assert!((s.width as f32 / s.height as f32 - 2.0).abs() < 0.05);
    }

    #[test]
    fn decide_size_rejects_zero_area() {
        assert!(decide_size(0, 100).is_none());
        assert!(decide_size(100, 0).is_none());
        assert!(decide_size(0, 0).is_none());
    }

    #[test]
    fn decide_size_extreme_aspect_clamps_long_edge_first() {
        // 8000x100 — long edge 8000 must drop to 256, even though that
        // makes shorter side < min_edge.
        let s = decide_size(8000, 100).expect("non-zero");
        assert_eq!(s.width, 256);
        // Height drops proportionally; we accept being below min_edge
        // here because clamping max edge wins.
        assert!(s.height >= 1);
    }

    #[test]
    fn decide_position_prefers_below_when_room() {
        // Cursor at top of viewport, plenty of room below.
        let side = decide_position(50.0, 800.0, 200.0);
        assert_eq!(side, PreviewSide::Below);
    }

    #[test]
    fn decide_position_flips_above_when_no_room_below() {
        // Cursor near bottom, no room below.
        let side = decide_position(750.0, 800.0, 200.0);
        assert_eq!(side, PreviewSide::Above);
    }

    #[test]
    fn decide_position_uses_better_side_when_neither_fits() {
        // Tiny viewport — neither side fits the preview. Pick whichever
        // has more room; below ties to above, above wins outright when
        // cursor is past the midpoint.
        let side = decide_position(60.0, 80.0, 200.0);
        assert_eq!(side, PreviewSide::Above);
    }

    #[test]
    fn compute_y_offset_above_subtracts_height_and_gap() {
        let y = compute_y_offset(400.0, 20.0, 100.0, PreviewSide::Above);
        assert!((y - (400.0 - 100.0 - PREVIEW_CURSOR_GAP_PX)).abs() < 0.01);
    }

    #[test]
    fn compute_y_offset_below_adds_line_height_and_gap() {
        let y = compute_y_offset(400.0, 20.0, 100.0, PreviewSide::Below);
        assert!((y - (400.0 + 20.0 + PREVIEW_CURSOR_GAP_PX)).abs() < 0.01);
    }

    #[test]
    fn compute_y_offset_above_clamps_to_top() {
        // Cursor very close to top — preview would render off-screen
        // above. We clamp to 0 rather than going negative.
        let y = compute_y_offset(5.0, 20.0, 100.0, PreviewSide::Above);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn clamp_x_to_viewport_keeps_inside_when_fits() {
        let x = clamp_x_to_viewport(100.0, 200.0, 800.0);
        assert_eq!(x, 100.0);
    }

    #[test]
    fn clamp_x_to_viewport_slides_left_when_would_overflow() {
        let x = clamp_x_to_viewport(700.0, 200.0, 800.0);
        assert_eq!(x, 600.0);
    }

    #[test]
    fn clamp_x_to_viewport_caps_at_zero_when_too_narrow() {
        let x = clamp_x_to_viewport(700.0, 1000.0, 800.0);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn decide_dismiss_confirms_on_enter_tab() {
        assert_eq!(
            decide_dismiss(PreviewInput::Confirm),
            DismissDecision::Confirm
        );
        assert_eq!(
            decide_dismiss(PreviewInput::ClickInside),
            DismissDecision::Confirm
        );
    }

    #[test]
    fn decide_dismiss_cancels_on_escape_and_other_key() {
        assert_eq!(
            decide_dismiss(PreviewInput::Escape),
            DismissDecision::Cancel
        );
        assert_eq!(
            decide_dismiss(PreviewInput::OtherKey),
            DismissDecision::Cancel
        );
        assert_eq!(
            decide_dismiss(PreviewInput::ClickOutside),
            DismissDecision::Cancel
        );
    }

    #[test]
    fn decide_dismiss_keeps_under_auto_dismiss_window() {
        assert_eq!(
            decide_dismiss(PreviewInput::Tick { elapsed_ms: 0.0 }),
            DismissDecision::Keep
        );
        assert_eq!(
            decide_dismiss(PreviewInput::Tick {
                elapsed_ms: PREVIEW_AUTO_DISMISS_MS - 1.0
            }),
            DismissDecision::Keep
        );
    }

    #[test]
    fn decide_dismiss_cancels_after_auto_dismiss_window() {
        assert_eq!(
            decide_dismiss(PreviewInput::Tick {
                elapsed_ms: PREVIEW_AUTO_DISMISS_MS
            }),
            DismissDecision::Cancel
        );
        assert_eq!(
            decide_dismiss(PreviewInput::Tick {
                elapsed_ms: PREVIEW_AUTO_DISMISS_MS + 500.0
            }),
            DismissDecision::Cancel
        );
    }
}
