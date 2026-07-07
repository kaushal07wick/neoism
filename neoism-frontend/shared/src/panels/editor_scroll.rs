// Pixel-perfect scroll for nvim editor panes — neovide-style.
//
// Per-pane critically-damped spring tracks Neovide's signed scroll
// position in *rows*. Whole-row
// wheel commits go to nvim immediately; nvim's `win_viewport` response
// seeds the one-cell visual lag that decays back to zero through the
// spring, producing the neovide-style slide.
//
// The offset is applied per editor `CellBg` / `CellText` in
// `screen/mod.rs`, Ghostty-style, so the pane origin remains stable
// while just the buffer cells slide. `sugarloaf.set_position` is for
// chrome text overlays, NOT for the per-cell grid renderer — shifting
// it is a no-op for the actual terminal/editor surface. (Documented as
// a memory trap so the next animated surface doesn't re-step on it.)
//
// VSync: render() ticks against real elapsed frame dt, so 60Hz / 144Hz /
// variable refresh all settle in identical wall-clock time. The
// `Renderer::needs_redraw` hook keeps the event loop firing redraws
// while any spring is still in motion.

use std::collections::HashMap;
use web_time::Instant;

use crate::animation::CriticallyDampedSpring;

/// Velocity gain applied to each trackpad/wheel pixel of input. Each
/// pixel of physical scroll contributes this many pixels/second to the
/// kinetic integrator so a flick keeps gliding briefly after release.
/// Conservative compared to the agent pane's `7.0` so the editor's
/// row commits don't blow past the user's intent.
const WHEEL_VELOCITY_GAIN: f32 = 3.0;

/// Hard cap on the kinetic velocity (pixels per second). Trackpad
/// flicks compound fast; the cap stops the glide from launching into
/// runaway scrolling on a single big swipe.
const WHEEL_VELOCITY_MAX_PX_S: f32 = 1200.0;

/// Exponential decay time constant for the kinetic velocity. After
/// this many seconds the velocity is at 1/e of its peak — keeps the
/// glide short and predictable rather than the long agent-pane tail.
const WHEEL_VELOCITY_DECAY_TAU: f32 = 0.18;

/// Velocity floor — below this we zero out and stop ticking, so an
/// idle pane carries no per-frame work.
const WHEEL_VELOCITY_MIN_PX_S: f32 = 20.0;

/// Time-to-target for the wheel/keyboard scroll spring. Matches the
/// old desktop/neovide path closely enough that a half-page nvim
/// scroll slides the previous grid through the scrollback ring instead
/// of snapping to the new viewport before a second animation runs.
const ANIMATION_LENGTH: f32 = 0.30;

/// Number of rows to animate through for a true far jump (`gg`/`G`,
/// scrollbar teleport, etc.). Ctrl-D/U is not a far jump: it is a
/// half-page scroll and must animate the full half-page delta.
const FAR_SCROLL_LINES: f32 = 1.0;

/// Desktop and web both use the same Neovide-style scroll cap. Web now
/// keeps explicit edge rows plus an extra painted output row during
/// fractional scroll, so it no longer needs a wasm-only clamp that made
/// trackpad and held-key scrolling diverge from desktop.
const MAX_GRID_SCROLL_ANIM_ROWS: f32 = 10_000.0;

const MAX_REPEATED_SINGLE_ROW_ANIM_ROWS: f32 = 10_000.0;

/// Maximum elastic edge offset, in rows. Caps the rubber-band stretch
/// when the user scrolls past a file boundary. Small (2 cells) so the
/// effect reads as a gentle resistance, not a free-scroll into empty
/// space. Apple's macOS overscroll uses ~3-4× this in pixels but the
/// content there is dense pages; for code editing 2 cells feels right.
const MAX_ELASTIC_ROWS: f32 = 2.0;

/// Time-to-target for the elastic bounce-back. Distinct from
/// `ANIMATION_LENGTH` because rubber-band wants a slower, eased
/// settle (not the snappy spring decay used for scroll). 0.45s gives
/// the "drift back to rest" feel Apple's bounce has.
const ELASTIC_BOUNCE_LENGTH: f32 = 0.45;

struct PaneScroll {
    spring: CriticallyDampedSpring,
    /// Cumulative wheel input in physical pixels. Mirrors neovide's
    /// `MouseManager::scroll_position` — input from the wheel/touchpad
    /// goes here, NOT into the spring. We only emit a discrete row
    /// commit to nvim when the floor() of `wheel_accumulator / cell_h`
    /// crosses an integer boundary. Spring lag comes from the
    /// `win_viewport` event nvim emits in response, NOT directly from
    /// wheel input — that separation is what makes neovide's wheel
    /// scroll feel smooth: the wheel and the animation never fight
    /// because they're driven by different sources.
    wheel_accumulator: f32,
    /// Kinetic velocity in pixels/second. Each wheel/trackpad event
    /// injects into this; the per-frame tick decays it exponentially
    /// and folds the residual back into `wheel_accumulator` so a
    /// trackpad flick keeps gliding briefly after the user lifts off.
    /// Keyboard scrolling (add_grid_scroll) does NOT touch this — it
    /// stays at zero so arrow-hold behaves exactly like Neovide.
    wheel_velocity_px_s: f32,
    /// Wall-clock of the previous kinetic tick — `None` while velocity
    /// has settled so an idle pane carries no per-frame work.
    wheel_velocity_last_tick: Option<Instant>,
    /// Apple-style elastic edge offset in physical pixels. Independent
    /// of the main scroll spring — kicks in only when the user keeps
    /// scrolling AT a buffer edge (top/bottom of file). Capped low
    /// (~2 cells) and decays with `ease_out_expo` for the rubber-band
    /// feel. Not a spring: rubber-band wants exponential pull-back,
    /// not critical damping.
    elastic: f32,
}

impl PaneScroll {
    fn new() -> Self {
        Self {
            spring: CriticallyDampedSpring::new(),
            wheel_accumulator: 0.0,
            wheel_velocity_px_s: 0.0,
            wheel_velocity_last_tick: None,
            elastic: 0.0,
        }
    }

    fn has_motion(&self) -> bool {
        self.spring.position != 0.0
            || self.elastic != 0.0
            || self.wheel_velocity_px_s.abs() >= WHEEL_VELOCITY_MIN_PX_S
    }
}

#[derive(Default)]
pub struct EditorScroll {
    panes: HashMap<usize, PaneScroll>,
}

impl EditorScroll {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a wheel delta (physical pixels, positive winit Y = scroll
    /// up direction = content slides down). Returns the integer number
    /// of rows committed to nvim — caller fires that many
    /// `<ScrollWheelUp>` / `<ScrollWheelDown>` notations.
    ///
    /// Wheel input accumulates separately from the spring. We emit
    /// discrete row commits as cell boundaries are crossed and leave the
    /// sub-row remainder here for the next wheel event. The spring is
    /// touched only by nvim's `win_viewport` response, which keeps input
    /// accumulation and visual animation from fighting each other.
    pub fn add_wheel_delta(
        &mut self,
        rich_text_id: usize,
        delta_pixels: f32,
        cell_height: f32,
    ) -> i32 {
        if cell_height <= 0.0 {
            return 0;
        }
        let pane = self
            .panes
            .entry(rich_text_id)
            .or_insert_with(PaneScroll::new);

        // Subtract-style accumulator (same as terminal_scroll): once a
        // whole cell is committed, drop it from the accumulator so the
        // residual lives in `(-cell, +cell)`. Built-in hysteresis ≈
        // 1 cell — touchpad jitter near a freshly-crossed boundary
        // can no longer toggle alternating commits in opposite
        // directions ("jerks back and forth, stays in same place").
        // The previous floor()-on-monotonic-accumulator algorithm
        // (mirrored from neovide) had no hysteresis: ±1px noise at a
        // multiple-of-cell crossing fired ±1 commits every event.
        pane.wheel_accumulator += delta_pixels;
        let mut committed = 0i32;
        while pane.wheel_accumulator.abs() >= cell_height {
            let sign = pane.wheel_accumulator.signum();
            pane.wheel_accumulator -= sign * cell_height;
            committed += sign as i32;
        }

        // Kinetic velocity injection — matches the agent pane's
        // timeline scroll shape but with smaller gain + faster decay
        // so the editor's row commits don't overshoot the user's
        // intent. Each pixel of physical scroll contributes to the
        // pixels/second integrator; the per-frame `tick_wheel` decays
        // it and folds the residual back into `wheel_accumulator` so a
        // trackpad flick keeps gliding briefly after release.
        let injected = delta_pixels * WHEEL_VELOCITY_GAIN;
        pane.wheel_velocity_px_s = (pane.wheel_velocity_px_s + injected)
            .clamp(-WHEEL_VELOCITY_MAX_PX_S, WHEEL_VELOCITY_MAX_PX_S);
        pane.wheel_velocity_last_tick = Some(Instant::now());

        committed
    }

    /// Per-frame kinetic tick. Decays the wheel velocity, integrates
    /// the residual into `wheel_accumulator`, and returns the number
    /// of rows that crossed a cell boundary this tick — the caller
    /// fires the equivalent of an extra wheel event for each. Returns
    /// 0 when the pane has no kinetic glide in flight (the common
    /// idle case; carries zero per-frame cost).
    pub fn tick_wheel(&mut self, rich_text_id: usize, cell_height: f32) -> i32 {
        if cell_height <= 0.0 {
            return 0;
        }
        let Some(pane) = self.panes.get_mut(&rich_text_id) else {
            return 0;
        };
        if pane.wheel_velocity_px_s.abs() < WHEEL_VELOCITY_MIN_PX_S {
            pane.wheel_velocity_px_s = 0.0;
            pane.wheel_velocity_last_tick = None;
            return 0;
        }
        let now = Instant::now();
        let dt = pane
            .wheel_velocity_last_tick
            .map(|last| now.saturating_duration_since(last).as_secs_f32().min(0.05))
            .unwrap_or(0.016);
        pane.wheel_velocity_last_tick = Some(now);

        let decay = (-dt / WHEEL_VELOCITY_DECAY_TAU).exp();
        pane.wheel_velocity_px_s *= decay;
        pane.wheel_accumulator += pane.wheel_velocity_px_s * dt;

        let mut committed = 0i32;
        while pane.wheel_accumulator.abs() >= cell_height {
            let sign = pane.wheel_accumulator.signum();
            pane.wheel_accumulator -= sign * cell_height;
            committed += sign as i32;
        }

        if pane.wheel_velocity_px_s.abs() < WHEEL_VELOCITY_MIN_PX_S {
            pane.wheel_velocity_px_s = 0.0;
            pane.wheel_velocity_last_tick = None;
        }
        committed
    }

    /// Reset the wheel accumulator residual for a pane to zero. Used at
    /// hard editor edges so rejected wheel input cannot leak into the
    /// next in-bounds scroll gesture.
    pub fn reset_wheel(&mut self, rich_text_id: usize) {
        let remove = if let Some(pane) = self.panes.get_mut(&rich_text_id) {
            pane.wheel_accumulator = 0.0;
            // Kill any in-flight kinetic glide at edges so it can't
            // keep firing commits nvim already rejects.
            pane.wheel_velocity_px_s = 0.0;
            pane.wheel_velocity_last_tick = None;
            !pane.has_motion()
        } else {
            false
        };
        if remove {
            self.panes.remove(&rich_text_id);
        }
    }

    /// Push a line-count delta from nvim's `win_viewport` redraw event.
    /// This matches Neovide's `flush`: positive `rows` subtracts from
    /// `scroll_animation.position`, negative `rows` adds to it.
    pub fn add_grid_scroll(
        &mut self,
        rich_text_id: usize,
        rows: i32,
        cell_height: f32,
        viewport_rows: usize,
    ) {
        if rows == 0 || cell_height <= 0.0 {
            return;
        }
        let pane = self
            .panes
            .entry(rich_text_id)
            .or_insert_with(PaneScroll::new);
        let viewport_rows = (viewport_rows.max(1) as f32).min(MAX_GRID_SCROLL_ANIM_ROWS);
        // Match Neovide's `flush()` in `rendered_window.rs:764-795`:
        //   if scroll_delta.unsigned_abs() > max_delta {
        //       scroll_offset = -(far_lines * scroll_delta.signum())
        //   } else {
        //       scroll_offset -= scroll_delta
        //       scroll_offset = scroll_offset.clamp(-max, max)
        //   }
        //   self.scroll_animation.position = scroll_offset
        // Neovide does NOT call `scroll_animation.reset()` on the
        // far-scroll branch — only `position` is replaced; `velocity`
        // is preserved so a mid-flight Ctrl-D doesn't have its
        // in-progress momentum killed. Our previous `spring.reset()`
        // here zeroed velocity too, which made the far-scroll branch
        // diverge from Neovide's animation curve.
        let single_delta = rows.unsigned_abs() as f32;
        if single_delta > viewport_rows {
            pane.spring.position =
                -(rows.signum() as f32) * FAR_SCROLL_LINES.min(viewport_rows);
        } else {
            let cap = if single_delta <= 1.0 {
                viewport_rows.min(MAX_REPEATED_SINGLE_ROW_ANIM_ROWS)
            } else {
                viewport_rows
            };
            pane.spring.position = (pane.spring.position - rows as f32).clamp(-cap, cap);
        }
    }

    /// Client-side scroll PREDICTION (netcode-style). Called at wheel
    /// COMMIT time — before nvim has confirmed — with the rows the
    /// authoritative `win_viewport` is EXPECTED to report (`rows` in
    /// pending/scroll_delta sign space, i.e. `-committed_wheel_rows`).
    ///
    /// Mirror of `add_grid_scroll`: prediction does `position += rows`
    /// so the still-unsnapped content renders as if it already
    /// scrolled; when the authoritative delta lands, `add_grid_scroll`
    /// does `position -= rows` which exactly cancels the kick — no
    /// double animation, no bookkeeping. A prediction nvim rejects
    /// (e.g. plugin swallowed the wheel) simply decays back to rest
    /// through the spring, reading as a gentle rubber-band.
    pub fn predict_grid_scroll(
        &mut self,
        rich_text_id: usize,
        rows: i32,
        cell_height: f32,
        viewport_rows: usize,
    ) {
        if rows == 0 || cell_height <= 0.0 {
            return;
        }
        let pane = self
            .panes
            .entry(rich_text_id)
            .or_insert_with(PaneScroll::new);
        let cap = (viewport_rows.max(1) as f32).min(MAX_GRID_SCROLL_ANIM_ROWS);
        pane.spring.position = (pane.spring.position + rows as f32).clamp(-cap, cap);
    }

    /// Apply elastic edge resistance: user kept scrolling but nvim
    /// can't (at top/bottom of file). Pushes a small visual offset in
    /// the direction the user wanted to go, capped at `MAX_ELASTIC`.
    /// `direction_pixels` should carry the magnitude AND sign of the
    /// rejected delta. Diminishing returns: each push is scaled down
    /// by `(1 - |elastic| / MAX_ELASTIC)` so the further the elastic
    /// is from rest, the harder it resists — that's the rubber-band
    /// feel Apple's bounce uses.
    pub fn push_elastic(
        &mut self,
        rich_text_id: usize,
        direction_pixels: f32,
        cell_height: f32,
    ) {
        if cell_height <= 0.0 || direction_pixels == 0.0 {
            return;
        }
        let pane = self
            .panes
            .entry(rich_text_id)
            .or_insert_with(PaneScroll::new);
        let max_elastic = MAX_ELASTIC_ROWS * cell_height;
        // Resistance grows with current elastic offset — Apple's
        // bounce caps via this exponential resistance, not a hard
        // clip. `resistance` is 0 at rest, 1 at the cap.
        let resistance = (pane.elastic.abs() / max_elastic).clamp(0.0, 0.95);
        let scaled = direction_pixels * (1.0 - resistance);
        pane.elastic = (pane.elastic + scaled).clamp(-max_elastic, max_elastic);
    }

    /// Per-frame step — advances every spring by the compositor frame
    /// interval and returns `true` if any spring is still in motion
    /// (caller schedules another redraw). Settled springs are removed
    /// so idle panes carry zero overhead.
    pub fn step(&mut self, dt: f32) -> bool {
        // Clamp to ANIMATION_LENGTH (0.30s) not 50 ms. The previous
        // 50 ms ceiling caused a visible chunky tail on the spring's
        // low-amplitude decay: during quiescent moments the event
        // loop on Hyprland/Wayland can stall for ~250 ms between
        // RedrawRequested dispatches (compositor frame-callback
        // throttling when damage area is small). With the 50 ms
        // clamp the spring's 250 ms gap was treated as a 50 ms step,
        // so each render decayed by only ~50 % (exp(-omega*0.05)).
        // The user saw the content "almost settle" then jump a
        // fraction closer every quarter-second for over a second —
        // exactly the held-arrow flicker symptom. With the clamp at
        // ANIMATION_LENGTH, a 250 ms gap decays by ~97 % in a single
        // step (exp(-omega*0.25)), so the spring effectively settles
        // in one frame after a stall instead of crawling through 5+.
        let dt = dt.clamp(0.0, ANIMATION_LENGTH);
        if dt <= 0.0 {
            return self.is_animating();
        }

        // Sub-step the spring at up to 120Hz so a 60Hz frame integrates
        // the analytical solution as two ~8.3ms steps rather than one
        // 16.7ms step. With the closed-form solution the result is
        // mathematically equivalent for FREE motion, but Neovide does
        // this and the sub-stepping shields the spring from numerical
        // drift when the frame interval gets long (low-FPS stall or
        // a Wayland compositor hiccup). MAX_SUB_DT mirrors Neovide's
        // `MAX_ANIMATION_DT = 1.0 / 120.0`.
        const MAX_SUB_DT: f32 = 1.0 / 120.0;
        let num_sub_steps = (dt / MAX_SUB_DT).ceil().max(1.0) as usize;
        let sub_dt = dt / num_sub_steps as f32;

        let mut still = false;
        let mut to_remove: Vec<usize> = Vec::new();
        for (id, pane) in self.panes.iter_mut() {
            let mut spring_moving = false;
            for _ in 0..num_sub_steps {
                if pane.spring.update(sub_dt, ANIMATION_LENGTH) {
                    spring_moving = true;
                }
            }

            // Elastic decay: rubber-band uses ease_out_expo, NOT
            // critical damping. Each frame, pull toward 0 by an
            // exponential factor of dt over ELASTIC_BOUNCE_LENGTH.
            // The closed-form: position(dt) = position * (1 - eased(dt/T))
            // gives that "drifts back, decelerating" feel.
            let elastic_moving = if pane.elastic.abs() > 0.05 {
                // ease_out_expo: 1 - 2^(-10*t). At t=0 returns 0
                // (no movement), at t=1 returns ~1 (fully there).
                // We sample over `dt / ELASTIC_BOUNCE_LENGTH`.
                let t = (dt / ELASTIC_BOUNCE_LENGTH).clamp(0.0, 1.0);
                let eased = if (t - 1.0).abs() < f32::EPSILON {
                    1.0
                } else {
                    1.0 - 2.0f32.powf(-10.0 * t)
                };
                pane.elastic *= 1.0 - eased;
                true
            } else {
                pane.elastic = 0.0;
                false
            };

            if spring_moving || elastic_moving {
                still = true;
            } else if pane.wheel_accumulator.abs() < 0.01 {
                to_remove.push(*id);
            }
        }
        for id in to_remove {
            self.panes.remove(&id);
        }
        still
    }

    /// Compatibility accessor for older call sites. This returns the
    /// signed row position only; callers that paint in pixels should use
    /// `current_scroll_offset` with their own cell height.
    #[allow(dead_code)]
    pub fn current_offset(&self, rich_text_id: usize) -> f32 {
        self.panes
            .get(&rich_text_id)
            .map(|p| p.spring.position)
            .unwrap_or(0.0)
    }

    /// Scrollback animation position in rows. This feeds the
    /// Neovide/Ghostty `floor(position) + row` lookup exactly.
    pub fn current_scroll_offset(&self, rich_text_id: usize) -> f32 {
        self.panes
            .get(&rich_text_id)
            .map(|p| p.spring.position)
            .unwrap_or(0.0)
    }

    /// Elastic edge offset only. This is a direct pixel translation on
    /// top of the scrollback row/fraction split.
    pub fn current_elastic_offset(&self, rich_text_id: usize) -> f32 {
        self.panes
            .get(&rich_text_id)
            .map(|p| p.elastic)
            .unwrap_or(0.0)
    }

    pub fn forget(&mut self, rich_text_id: usize) {
        self.panes.remove(&rich_text_id);
    }

    pub fn reset_all(&mut self) {
        self.panes.clear();
    }

    pub fn is_animating(&self) -> bool {
        self.panes.values().any(PaneScroll::has_motion)
    }
}

#[cfg(test)]
mod tests {
    use super::EditorScroll;

    #[test]
    fn grid_scroll_animates_full_trackpad_burst_without_web_cap() {
        let mut scroll = EditorScroll::new();
        scroll.add_grid_scroll(1, 6, 16.0, 30);

        assert_eq!(scroll.current_scroll_offset(1), -6.0);
    }

    #[test]
    fn repeated_single_row_scroll_accumulates_like_desktop() {
        let mut scroll = EditorScroll::new();
        for _ in 0..4 {
            scroll.add_grid_scroll(1, 1, 16.0, 30);
        }

        assert_eq!(scroll.current_scroll_offset(1), -4.0);
    }

    #[test]
    fn predicted_scroll_cancels_exactly_against_authoritative_echo() {
        let mut scroll = EditorScroll::new();
        // Wheel-up commit of 3 rows predicted at input time
        // (pending sign space: -3)...
        scroll.predict_grid_scroll(1, -3, 16.0, 30);
        assert_eq!(scroll.current_scroll_offset(1), -3.0);
        // ...then nvim's win_viewport confirms the same delta: the
        // reconciliation must cancel the kick, not double-animate.
        scroll.add_grid_scroll(1, -3, 16.0, 30);
        assert_eq!(scroll.current_scroll_offset(1), 0.0);
    }

    #[test]
    fn unpredicted_scroll_still_glides() {
        let mut scroll = EditorScroll::new();
        // No prediction (e.g. j/k keyboard scroll): the authoritative
        // delta seeds the classic neovide lag as before.
        scroll.add_grid_scroll(1, -3, 16.0, 30);
        assert_eq!(scroll.current_scroll_offset(1), 3.0);
    }
}
