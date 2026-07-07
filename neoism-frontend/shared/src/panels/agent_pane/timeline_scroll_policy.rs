//! Shared timeline scroll policy for the agent pane.
//!
//! Wheel/touchpad scrolling and keyboard half-page scrolling both feed the
//! same timeline state, but Ctrl+U/Ctrl+D should feel like the markdown
//! renderer/nvim path: a fast half-page jump with the existing kinetic tail.

/// Fraction of the visible timeline that Ctrl+U / Ctrl+D should travel.
pub const CTRL_U_D_VIEWPORT_FRACTION: f32 = 0.5;

/// Convert the current viewport height into an agent timeline half-page step.
///
/// The sign mirrors the timeline's scroll offset convention: positive reveals
/// older history above the viewport (Ctrl+U), negative moves toward the bottom
/// / newer messages (Ctrl+D).
pub fn ctrl_u_d_scroll_delta(viewport_height_px: f32, older_history: bool) -> f32 {
    let magnitude = viewport_height_px.max(0.0) * CTRL_U_D_VIEWPORT_FRACTION;
    if older_history {
        magnitude
    } else {
        -magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_u_d_scroll_delta_is_signed_half_viewport() {
        assert_eq!(ctrl_u_d_scroll_delta(640.0, true), 320.0);
        assert_eq!(ctrl_u_d_scroll_delta(640.0, false), -320.0);
        assert_eq!(ctrl_u_d_scroll_delta(-12.0, true), 0.0);
    }
}
