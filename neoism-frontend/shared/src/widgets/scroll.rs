//! Reusable critically-damped spring for kinetic scroll.
//!
//! Lifted verbatim from `crate::animation::CriticallyDampedSpring`
//! so behaviour is byte-identical to the per-panel implementations it
//! replaces (`editor/file_tree`, eventually `editor/markdown`,
//! `neoism/agent/pane`). The math is the closed-form solution for a
//! critically damped harmonic oscillator with zeta = 1; the only
//! tunable is `animation_length`, the time-to-2% of remaining distance.
//!
//! Model: `current` chases `target`. Most callers leave `target = 0` and
//! treat `current` as a visual *offset* that decays back to zero after
//! the underlying snapped position has changed (the "lag" pattern used
//! by file_tree / git_diff_panel / command_palette). Callers who need
//! a moving destination can drive `target` directly via `set_target`
//! and read `current()` each frame.
//!
//! The widget intentionally does NOT own a clock — `tick` takes a
//! pre-computed `dt`. Callers track `Instant::now()` themselves so they
//! can clamp `dt` (50 ms cap is typical) and reset on idle frames.

/// Critically-damped spring that drives `current` toward `target`.
///
/// See the module docs for the lag-offset usage pattern. Construct with
/// [`Scroll::new`] for default damping (matches `file_tree`'s
/// `SCROLL_ANIMATION_LENGTH` = 0.30s), or tune via
/// [`Scroll::with_animation_length`] / [`Scroll::with_damping`] /
/// [`Scroll::with_spring`].
#[derive(Clone, Debug)]
pub struct Scroll {
    current: f32,
    target: f32,
    velocity: f32,
    /// Time-to-2%-of-target in seconds. Critically damped: zeta = 1,
    /// omega = 4 / animation_length. Stored directly because the spring
    /// math is naturally parameterised by this single knob.
    animation_length: f32,
    /// When true, [`Scroll::current`] rounds to whole pixels before
    /// returning. Off by default — most callers snap externally (e.g.
    /// `snap_to_device_px` against the HiDPI scale factor).
    pub snap_to_pixel: bool,
}

/// Default time-to-2%-of-target. Matches `file_tree::SCROLL_ANIMATION_LENGTH`
/// so a bare `Scroll::new()` reproduces the file-tree scroll feel.
pub const DEFAULT_ANIMATION_LENGTH: f32 = 0.30;

// The widget is a reusable primitive: file_tree only exercises a
// subset of the API today, with `editor/markdown` and `agent/pane`
// migrations queued. Suppress per-method dead-code warnings until
// those callers land — the surface area is intentional, not stale.
#[allow(dead_code)]
impl Scroll {
    pub fn new() -> Self {
        Self {
            current: 0.0,
            target: 0.0,
            velocity: 0.0,
            animation_length: DEFAULT_ANIMATION_LENGTH,
            snap_to_pixel: false,
        }
    }

    /// Set the time-to-2%-of-target in seconds. Primary tuning knob.
    pub fn with_animation_length(mut self, animation_length: f32) -> Self {
        self.animation_length = animation_length.max(1e-4);
        self
    }

    /// Set damping coefficient. Critically damped springs have
    /// `damping = 2 * omega` and `omega = 4 / animation_length`, so
    /// `damping = 8 / animation_length`. Provided for API parity with
    /// the spec; most callers should prefer
    /// [`Scroll::with_animation_length`].
    pub fn with_damping(mut self, damping: f32) -> Self {
        let damping = damping.max(1e-4);
        // omega = damping / 2  ⇒  animation_length = 4 / omega = 8 / damping
        self.animation_length = 8.0 / damping;
        self
    }

    /// Set spring constant `k`. Critically damped springs have
    /// `k = omega^2` so `animation_length = 4 / sqrt(k)`. Provided for
    /// API parity with the spec.
    pub fn with_spring(mut self, spring_constant: f32) -> Self {
        let k = spring_constant.max(1e-8);
        self.animation_length = 4.0 / k.sqrt();
        self
    }

    /// Current value of the spring. With [`Self::snap_to_pixel`] enabled
    /// the result is rounded to the nearest whole pixel.
    pub fn current(&self) -> f32 {
        if self.snap_to_pixel {
            self.current.round()
        } else {
            self.current
        }
    }

    /// Destination the spring is chasing.
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Animation length in seconds (time-to-2%-of-target).
    pub fn animation_length(&self) -> f32 {
        self.animation_length
    }

    /// Move the target. The spring will animate `current` toward it.
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Same as [`Self::set_target`] but clears any in-flight velocity
    /// and snaps `current` to the new target immediately. Useful for
    /// scrollbar drag, where the user is steering directly and any
    /// residual spring motion would feel sluggish.
    pub fn set_target_immediate(&mut self, target: f32) {
        self.target = target;
        self.current = target;
        self.velocity = 0.0;
    }

    /// Add `delta` to `current` without touching `target`. This is the
    /// "kinetic kick" used by wheel-scroll handlers in the lag-offset
    /// pattern: the underlying snapped position has already moved, so
    /// the visual offset is bumped away from the target and the spring
    /// decays it back.
    pub fn scroll_by(&mut self, delta: f32) {
        self.current += delta;
    }

    /// Jump straight to `target` with no animation.
    pub fn snap_to(&mut self, target: f32) {
        self.target = target;
        self.current = target;
        self.velocity = 0.0;
    }

    /// Clamp `current` and `target` into `[min, max]`. Velocity is
    /// zeroed if either value was clipped — otherwise the spring would
    /// keep pushing past the bound.
    pub fn clamp(&mut self, min: f32, max: f32) {
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        let new_current = self.current.clamp(lo, hi);
        let new_target = self.target.clamp(lo, hi);
        if new_current != self.current || new_target != self.target {
            self.velocity = 0.0;
        }
        self.current = new_current;
        self.target = new_target;
    }

    /// `true` while the spring still has work to do — either it has
    /// not reached the target, or it has nonzero velocity. Callers use
    /// this to decide whether to schedule another redraw frame.
    pub fn is_animating(&self) -> bool {
        (self.current - self.target).abs() >= 0.01 || self.velocity.abs() >= 0.01
    }

    /// Reset to a quiescent state at zero.
    pub fn reset(&mut self) {
        self.current = 0.0;
        self.target = 0.0;
        self.velocity = 0.0;
    }

    /// Step the spring forward by `dt` seconds. Returns `true` while
    /// still animating; `false` once the spring has settled (caller can
    /// stop ticking).
    ///
    /// Ported verbatim from neovide's `animation_utils.rs` via
    /// `crate::animation::CriticallyDampedSpring::update` —
    /// the closed-form solution for a critically damped harmonic
    /// oscillator, evaluated against `(current - target)` so the
    /// equilibrium point can be arbitrary rather than fixed at zero.
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.animation_length <= dt {
            self.current = self.target;
            self.velocity = 0.0;
            return false;
        }
        let delta = self.current - self.target;
        if delta == 0.0 && self.velocity == 0.0 {
            return false;
        }

        // < 1 underdamped, 1 critically damped, > 1 overdamped.
        let zeta = 1.0;
        // omega chosen so the destination is reached within 2%
        // tolerance in animation_length seconds.
        let omega = 4.0 / (zeta * self.animation_length);

        // Closed-form solution for a critically damped harmonic
        // oscillator about the equilibrium `target`. `a` and `b` are
        // the initial conditions derived by setting dt=0 in the
        // position and velocity equations.
        let a = delta;
        let b = delta * omega + self.velocity;
        let c = (-omega * dt).exp();

        let new_delta = (a + b * dt) * c;
        self.velocity = c * (-a * omega - b * dt * omega + b);
        self.current = self.target + new_delta;

        if new_delta.abs() < 0.01 {
            self.current = self.target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }
}

impl Default for Scroll {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_quiescent() {
        let s = Scroll::new();
        assert_eq!(s.current(), 0.0);
        assert_eq!(s.target(), 0.0);
        assert!(!s.is_animating());
    }

    #[test]
    fn scroll_by_kicks_current_away_from_target() {
        let mut s = Scroll::new();
        s.scroll_by(50.0);
        assert_eq!(s.current(), 50.0);
        assert_eq!(s.target(), 0.0);
        assert!(s.is_animating());
    }

    #[test]
    fn tick_decays_offset_toward_target() {
        // Reproduces the existing CriticallyDampedSpring contract: an
        // offset added to current decays back to target (= 0 here)
        // within a few animation_length intervals.
        let mut s = Scroll::new().with_animation_length(0.1);
        s.scroll_by(100.0);
        let dt = 0.001;
        let mut t = 0.0;
        while t < 0.1 * 5.0 {
            if !s.tick(dt) {
                break;
            }
            t += dt;
        }
        assert_eq!(s.current(), 0.0, "spring should fully settle to target");
        assert_eq!(s.target(), 0.0);
    }

    #[test]
    fn tick_dt_exceeds_animation_length_snaps_home() {
        // Frame ran longer than the animation budget → snap to target,
        // don't integrate over a giant step that would overshoot.
        let mut s = Scroll::new().with_animation_length(0.1);
        s.scroll_by(50.0);
        assert!(!s.tick(1.0));
        assert_eq!(s.current(), 0.0);
    }

    #[test]
    fn scroll_by_mid_flight_keeps_tracking() {
        // Caller stacks another scroll while the spring is moving —
        // exercises the "responsive" property: current shifts further
        // from target, not reset, no overshoot.
        let mut s = Scroll::new().with_animation_length(0.1);
        s.scroll_by(100.0);
        s.tick(0.01);
        let after_first = s.current().abs();
        s.scroll_by(100.0);
        s.tick(0.01);
        assert!(s.current().abs() > after_first);
        assert!(s.current() > 0.0);
    }

    #[test]
    fn set_target_drives_current_toward_destination() {
        let mut s = Scroll::new().with_animation_length(0.1);
        s.set_target(200.0);
        // Step a few frames; current should climb toward 200.
        for _ in 0..50 {
            if !s.tick(0.001) {
                break;
            }
        }
        // Not necessarily exactly 200 yet — but well underway.
        assert!(s.current() > 0.0);
        assert!(s.current() <= 200.0);
    }

    #[test]
    fn snap_to_skips_animation() {
        let mut s = Scroll::new();
        s.snap_to(500.0);
        assert_eq!(s.current(), 500.0);
        assert_eq!(s.target(), 500.0);
        assert!(!s.is_animating());
    }

    #[test]
    fn set_target_immediate_clears_velocity() {
        let mut s = Scroll::new().with_animation_length(0.1);
        s.scroll_by(100.0);
        s.tick(0.01);
        // velocity is nonzero here; immediate set should zero it.
        s.set_target_immediate(0.0);
        assert_eq!(s.current(), 0.0);
        assert!(!s.is_animating());
    }

    #[test]
    fn clamp_pins_both_current_and_target() {
        let mut s = Scroll::new();
        s.set_target(500.0);
        s.scroll_by(800.0);
        s.clamp(0.0, 100.0);
        assert_eq!(s.current(), 100.0);
        assert_eq!(s.target(), 100.0);
    }

    #[test]
    fn with_damping_inversely_sets_animation_length() {
        let s = Scroll::new().with_damping(80.0);
        // damping = 8 / animation_length → animation_length = 0.1
        assert!((s.animation_length() - 0.1).abs() < 1e-6);
    }

    #[test]
    fn with_spring_inversely_sets_animation_length() {
        let s = Scroll::new().with_spring(1600.0);
        // animation_length = 4 / sqrt(k) = 4 / 40 = 0.1
        assert!((s.animation_length() - 0.1).abs() < 1e-6);
    }

    #[test]
    fn snap_to_pixel_rounds_current() {
        let mut s = Scroll::new();
        s.snap_to_pixel = true;
        s.scroll_by(12.4);
        assert_eq!(s.current(), 12.0);
        s.scroll_by(0.2); // current = 12.6
        assert_eq!(s.current(), 13.0);
    }

    #[test]
    fn tick_matches_legacy_spring_for_offset_decay() {
        // Bit-for-bit parity with the lifted `crate::animation`
        // `CriticallyDampedSpring` when target = 0. Same omega, same
        // closed-form, same settle threshold — the offset trajectory
        // must agree to within f32 rounding.
        use crate::animation::CriticallyDampedSpring;
        let mut legacy = CriticallyDampedSpring::new();
        legacy.position = 100.0;
        let mut scroll = Scroll::new().with_animation_length(0.3);
        scroll.scroll_by(100.0);
        let dt = 0.016;
        for _ in 0..30 {
            let l_alive = legacy.update(dt, 0.3);
            let s_alive = scroll.tick(dt);
            assert!(
                (legacy.position - scroll.current()).abs() < 1e-4,
                "trajectory diverged: legacy={}, scroll={}",
                legacy.position,
                scroll.current()
            );
            assert_eq!(l_alive, s_alive);
            if !l_alive {
                break;
            }
        }
    }
}
