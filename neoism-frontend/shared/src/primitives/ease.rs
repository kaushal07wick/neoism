//! Easing curves shared across chrome animations.
//!
//! Callers are expected to pass `t` already clamped to [0, 1] — these
//! match the unclamped 3-of-4 panel copies so output is identical for
//! in-range inputs. (`command_composer`'s old local copy clamped `t`,
//! but every call site there already clamped before calling, so the
//! results are bit-identical.)

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}
