//! Animation primitives for the IDE-mode renderer.
//!
//! Foundation for Phase 4 pixel-scroll. Two pieces:
//!   - Standard easing functions (linear, quad, cubic, expo) so motion
//!     curves can be tuned per-surface (terminal scroll vs. cursor
//!     trail vs. tab swap).
//!   - `CriticallyDampedSpring`: target-tracking animator that lets a
//!     value chase a moving destination with no overshoot. Used for
//!     scroll position when input arrives while a previous animation
//!     is still mid-flight (the destination shifts; we don't want to
//!     restart from rest).
//!
//! Ported from `neovide/src/renderer/animation_utils.rs` with the
//! `glamour` dependency removed — Rio doesn't pull glamour in, and the
//! 2D-point variant is trivially expressible as `(f32, f32)`.

#[inline]
pub fn ease_linear(t: f32) -> f32 {
    t
}

#[inline]
pub fn ease_in_quad(t: f32) -> f32 {
    t * t
}

#[inline]
pub fn ease_out_quad(t: f32) -> f32 {
    -t * (t - 2.0)
}

#[inline]
pub fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        let n = t * 2.0 - 1.0;
        -0.5 * (n * (n - 2.0) - 1.0)
    }
}

#[inline]
pub fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let n = t - 1.0;
    n * n * n + 1.0
}

#[inline]
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let n = 2.0 * t;
    if n < 1.0 {
        0.5 * n * n * n
    } else {
        let n = n - 2.0;
        0.5 * (n * n * n + 2.0)
    }
}

#[inline]
pub fn ease_in_expo(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else {
        2.0f32.powf(10.0 * (t - 1.0))
    }
}

#[inline]
pub fn ease_out_expo(t: f32) -> f32 {
    if (t - 1.0).abs() < f32::EPSILON {
        1.0
    } else {
        1.0 - 2.0f32.powf(-10.0 * t)
    }
}

#[inline]
pub fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

#[inline]
pub fn ease(ease_func: fn(f32) -> f32, start: f32, end: f32, t: f32) -> f32 {
    lerp(start, end, ease_func(t))
}

/// 2-axis convenience for cursor / window position animations.
#[inline]
pub fn ease_point(
    ease_func: fn(f32) -> f32,
    start: (f32, f32),
    end: (f32, f32),
    t: f32,
) -> (f32, f32) {
    (
        ease(ease_func, start.0, end.0, t),
        ease(ease_func, start.1, end.1, t),
    )
}

/// Critically damped spring — chases zero with no overshoot. Set
/// `position` to the *signed distance from the target* (not the
/// absolute coordinate); each `update` step nudges it toward zero.
/// When a new scroll arrives mid-flight, add the new delta into
/// `position` rather than restarting — that's the property that makes
/// it feel responsive instead of laggy.
///
/// Ported verbatim from neovide's animation_utils so behavior matches.
#[derive(Clone, Debug, Default)]
pub struct CriticallyDampedSpring {
    pub position: f32,
    velocity: f32,
}

impl CriticallyDampedSpring {
    pub fn new() -> Self {
        Self::default()
    }

    /// Step the spring forward by `dt` seconds. `animation_length` is
    /// the time-to-target-within-2% — a tuning parameter, typically
    /// 0.06–0.12s for scroll. Returns `true` while still animating;
    /// `false` once the spring has settled (caller can stop ticking).
    pub fn update(&mut self, dt: f32, animation_length: f32) -> bool {
        if animation_length <= dt {
            self.reset();
            return false;
        }
        if self.position == 0.0 {
            return false;
        }

        // < 1 underdamped, 1 critically damped, > 1 overdamped.
        let zeta = 1.0;
        // omega chosen so the destination is reached within 2%
        // tolerance in animation_length seconds.
        let omega = 4.0 / (zeta * animation_length);

        // Closed-form solution for a critically damped harmonic
        // oscillator. `a` and `b` are the initial conditions derived
        // by setting dt=0 in the position and velocity equations.
        let a = self.position;
        let b = self.position * omega + self.velocity;
        let c = (-omega * dt).exp();

        self.position = (a + b * dt) * c;
        self.velocity = c * (-a * omega - b * dt * omega + b);

        if self.position.abs() < 0.01 {
            self.reset();
            false
        } else {
            true
        }
    }

    pub fn reset(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_endpoints() {
        assert_eq!(lerp(1.0, 0.0, 0.0), 1.0);
        assert_eq!(lerp(1.0, 0.0, 1.0), 0.0);
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
    }

    #[test]
    fn ease_funcs_hit_endpoints() {
        // Every easing curve must be 0 at t=0 and 1 at t=1 — that's
        // the contract the renderer relies on to land precisely.
        for f in [
            ease_linear as fn(f32) -> f32,
            ease_in_quad,
            ease_out_quad,
            ease_in_out_quad,
            ease_in_cubic,
            ease_out_cubic,
            ease_in_out_cubic,
            ease_in_expo,
            ease_out_expo,
        ] {
            assert!((f(0.0)).abs() < 1e-5, "{:?}(0.0) drifted", f as usize);
            assert!((f(1.0) - 1.0).abs() < 1e-4, "{:?}(1.0) drifted", f as usize);
        }
    }

    #[test]
    fn ease_point_2d_matches_componentwise() {
        let p = ease_point(ease_in_out_quad, (0.0, 10.0), (10.0, 0.0), 0.5);
        let expected = (
            ease(ease_in_out_quad, 0.0, 10.0, 0.5),
            ease(ease_in_out_quad, 10.0, 0.0, 0.5),
        );
        assert!((p.0 - expected.0).abs() < 1e-6);
        assert!((p.1 - expected.1).abs() < 1e-6);
    }

    #[test]
    fn spring_at_rest_returns_false() {
        let mut s = CriticallyDampedSpring::new();
        assert!(!s.update(0.016, 0.1));
        assert_eq!(s.position, 0.0);
    }

    #[test]
    fn spring_returns_to_zero_within_animation_length() {
        let mut s = CriticallyDampedSpring::new();
        s.position = 100.0;
        let dt = 0.001;
        let mut t = 0.0;
        let len = 0.1;
        // Step forward in 1ms increments; spring should settle well
        // before 5x the animation length even with conservative dt.
        while t < len * 5.0 {
            if !s.update(dt, len) {
                break;
            }
            t += dt;
        }
        // Once settled, reset() is called → position is exactly 0.
        assert_eq!(s.position, 0.0, "spring should fully settle");
    }

    #[test]
    fn spring_dt_exceeds_length_snaps_home() {
        let mut s = CriticallyDampedSpring::new();
        s.position = 50.0;
        // Frame ran longer than the animation budget → snap, don't
        // try to integrate over a giant step that would overshoot.
        assert!(!s.update(1.0, 0.1));
        assert_eq!(s.position, 0.0);
    }

    #[test]
    fn spring_accumulates_new_delta_mid_flight() {
        // Caller adds another scroll while the spring is still
        // moving — exercising the "responsive" property: position
        // shifts further from zero, but the spring keeps tracking.
        let mut s = CriticallyDampedSpring::new();
        s.position = 100.0;
        s.update(0.01, 0.1);
        let after_first = s.position.abs();
        s.position += 100.0;
        s.update(0.01, 0.1);
        // Still meaningfully off-zero; not snapped or overshooting.
        assert!(s.position.abs() > after_first);
        assert!(s.position > 0.0);
    }
}
