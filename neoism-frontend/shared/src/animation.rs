//! Easing curves + critically damped spring shared by chrome panels.
//!
//! Lifted from `neoism_backend::animation` so the same animation math
//! runs on native and web. No platform deps — pure f32 math.

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
pub fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

#[inline]
pub fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

#[inline]
pub fn ease(ease_func: fn(f32) -> f32, start: f32, end: f32, t: f32) -> f32 {
    lerp(start, end, ease_func(t))
}

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
/// `position` to the signed distance from the target; each step nudges
/// it toward zero. Mid-flight new deltas are *added* to position so the
/// motion feels responsive (no restart-from-rest).
#[derive(Clone, Debug, Default)]
pub struct CriticallyDampedSpring {
    pub position: f32,
    velocity: f32,
}

impl CriticallyDampedSpring {
    pub fn new() -> Self {
        Self::default()
    }

    /// Step the spring forward `dt` seconds. `animation_length` is the
    /// time-to-target-within-2% — tune 0.06–0.12s for scroll. Returns
    /// `true` while still animating; `false` once settled.
    pub fn update(&mut self, dt: f32, animation_length: f32) -> bool {
        if animation_length <= dt {
            self.reset();
            return false;
        }
        if self.position == 0.0 {
            return false;
        }

        let zeta = 1.0;
        let omega = 4.0 / (zeta * animation_length);

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
        while t < len * 5.0 {
            if !s.update(dt, len) {
                break;
            }
            t += dt;
        }
        assert_eq!(s.position, 0.0);
    }

    #[test]
    fn spring_dt_exceeds_length_snaps_home() {
        let mut s = CriticallyDampedSpring::new();
        s.position = 50.0;
        assert!(!s.update(1.0, 0.1));
        assert_eq!(s.position, 0.0);
    }
}
