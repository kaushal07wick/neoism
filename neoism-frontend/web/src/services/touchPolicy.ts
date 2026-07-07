/**
 * Touch gesture decisions, mirrored 1:1 from the shared Rust
 * `neoism-ui::touch_policy` module. Keeps the web frontend and the
 * desktop fork in lock step on every touch-driven choice:
 *
 *   - tap-vs-drag-vs-scroll-vs-pinch state machine
 *   - long-press → context-menu promotion (500ms threshold)
 *   - pinch-zoom dead-zones (don't zoom on chrome panel headers)
 *   - two-finger same-direction pan → scroll (not zoom)
 *   - swipe-from-edge suppression in the editor area
 *
 * If the shared Rust module changes one of these decisions, this
 * file is the single mirror that must change with it.
 *
 * See `neoism-frontend/shared/src/touch_policy.rs` for the source of
 * truth and the unit tests that pin the behaviour.
 */

/** Pixel motion above which a tap becomes a drag (matches MAX_TAP_DISTANCE). */
export const MAX_TAP_DISTANCE = 5;

/** Editor-area motion budget before a tap becomes scroll. */
export const EDITOR_SCROLL_TAP_DISTANCE = 16;

/** Wall-clock millis a finger must hold before a long-press fires. */
export const LONG_PRESS_MS = 500;

/** Pixel pan budget before two-finger pan commits to scroll. */
export const TWO_FINGER_PAN_THRESHOLD = 6;

/** Pixel distance-change before pinch commits to zoom. */
export const PINCH_COMMIT_THRESHOLD = 18;

/** Pinch zoom speed multiplier. Matches Rust `TOUCH_ZOOM_FACTOR`. */
const TOUCH_ZOOM_FACTOR = 1.0;

/** Quantisation step for font-size deltas (matches FONT_SIZE_STEP). */
const FONT_SIZE_STEP = 1.0;

/** Coarse classification of the zone a touch started in. */
export type TouchZone = "terminal-body" | "chrome-panel" | "editor-area";

/** POD touch sample fed into the policy. */
export interface TouchSample {
  /** Stable per-finger id (Touch.identifier). */
  id: number;
  /** Canvas-local logical-pixel x. */
  x: number;
  /** Canvas-local logical-pixel y. */
  y: number;
  /** Wall-clock millis (performance.now() rounded is fine). */
  timeMs: number;
}

/** Window-local layout for coordinate clamping. */
export interface TouchLayoutSize {
  width: number;
  height: number;
}

/** Plan returned to the caller; mirrors Rust `TouchAction`. */
export type TouchAction =
  | { kind: "none" }
  | { kind: "start-simulated-left-click"; x: number; y: number }
  | { kind: "scroll"; dx: number; dy: number; x: number; y: number }
  | { kind: "update-mouse-position"; x: number; y: number }
  | { kind: "change-font-size"; direction: "increase" | "decrease" }
  | { kind: "end-simulated-left-click"; x: number; y: number }
  | { kind: "end-select" }
  | { kind: "end-scroll" }
  | { kind: "promote-tap-to-scroll" }
  | { kind: "open-context-menu"; x: number; y: number }
  | { kind: "two-finger-scroll"; dx: number; dy: number }
  | { kind: "suppress-native-gesture" };

const NONE: TouchAction = { kind: "none" };

type State =
  | { kind: "none" }
  | { kind: "tap"; start: TouchSample; zone: TouchZone }
  | { kind: "select"; start: TouchSample }
  | { kind: "scroll"; last: TouchSample }
  | { kind: "long-pressed"; start: TouchSample }
  | { kind: "zoom"; zoom: ZoomState }
  | { kind: "two-finger-scroll"; a: TouchSample; b: TouchSample }
  | { kind: "invalid"; ids: Set<number> };

interface ZoomState {
  a: TouchSample;
  b: TouchSample;
  zone: TouchZone;
  initialDistance: number;
  initialMidpoint: { x: number; y: number };
  fractions: number;
  lastFontDelta: number;
}

function dist(a: TouchSample, b: TouchSample): number {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  return Math.hypot(dx, dy);
}

function clamp(touch: TouchSample, layout: TouchLayoutSize): { x: number; y: number } {
  const x = Math.max(0, Math.min(layout.width, touch.x)) | 0;
  const y = Math.max(0, Math.min(layout.height, touch.y)) | 0;
  return { x, y };
}

/**
 * Stateful gesture classifier. Hold one instance per
 * canvas/`TerminalPanel`; feed `start`/`move`/`end` from the DOM
 * `touchstart` / `touchmove` / `touchend` listeners and run
 * `tickLongPress` from the existing RAF loop.
 *
 * All decisions match `neoism-frontend/shared/src/touch_policy.rs`;
 * see its tests for the canonical behaviour.
 */
export class TouchPolicy {
  private state: State = { kind: "none" };

  /** Reset to the idle state; call when the canvas loses focus or
   *  the wasm bridge is replaced. */
  reset(): void {
    this.state = { kind: "none" };
  }

  /** True when at least one finger is currently active. */
  isActive(): boolean {
    return this.state.kind !== "none";
  }

  /**
   * Decide whether the platform's back/forward swipe-from-edge
   * should be eaten for a touch starting in `zone`. Mirror of the
   * shared `should_suppress_swipe_back` helper.
   */
  static shouldSuppressSwipeBack(zone: TouchZone): boolean {
    return zone === "editor-area";
  }

  /** Feed a `touchstart` sample with its zone hint. */
  start(sample: TouchSample, zone: TouchZone): TouchAction {
    const current = this.state;
    switch (current.kind) {
      case "none":
        this.state = { kind: "tap", start: sample, zone };
        return NONE;
      case "tap": {
        // Second finger lands while first is still a tap → pinch.
        // Inherit the first finger's zone (gesture is owned by where
        // the user first put their hand down).
        const zoom: ZoomState = {
          a: current.start,
          b: sample,
          zone: current.zone,
          initialDistance: dist(current.start, sample),
          initialMidpoint: {
            x: (current.start.x + sample.x) * 0.5,
            y: (current.start.y + sample.y) * 0.5,
          },
          fractions: 0,
          lastFontDelta: 0,
        };
        this.state = { kind: "zoom", zoom };
        return NONE;
      }
      case "zoom": {
        const ids = new Set<number>([current.zoom.a.id, current.zoom.b.id, sample.id]);
        this.state = { kind: "invalid", ids };
        return NONE;
      }
      case "two-finger-scroll": {
        const ids = new Set<number>([current.a.id, current.b.id, sample.id]);
        this.state = { kind: "invalid", ids };
        return NONE;
      }
      case "scroll":
      case "select": {
        const ids = new Set<number>([
          current.kind === "scroll" ? current.last.id : current.start.id,
        ]);
        this.state = { kind: "invalid", ids };
        return NONE;
      }
      case "long-pressed": {
        const ids = new Set<number>([current.start.id, sample.id]);
        this.state = { kind: "invalid", ids };
        return NONE;
      }
      case "invalid": {
        current.ids.add(sample.id);
        return NONE;
      }
    }
  }

  /** Feed a `touchmove` sample. */
  move(sample: TouchSample, layout: TouchLayoutSize): TouchAction {
    const current = this.state;
    switch (current.kind) {
      case "none":
      case "long-pressed":
      case "invalid":
        return NONE;
      case "tap": {
        const dx = sample.x - current.start.x;
        const dy = sample.y - current.start.y;
        if (current.zone === "editor-area") {
          if (
            Math.abs(dy) > EDITOR_SCROLL_TAP_DISTANCE ||
            Math.hypot(dx, dy) > EDITOR_SCROLL_TAP_DISTANCE
          ) {
            const start = current.start;
            this.state = { kind: "scroll", last: start };
            return { kind: "promote-tap-to-scroll" };
          }
          return NONE;
        }
        if (Math.abs(dx) > MAX_TAP_DISTANCE) {
          const start = current.start;
          this.state = { kind: "select", start };
          const { x, y } = clamp(start, layout);
          return { kind: "start-simulated-left-click", x, y };
        }
        if (Math.abs(dy) > MAX_TAP_DISTANCE) {
          const start = current.start;
          this.state = { kind: "scroll", last: start };
          return { kind: "promote-tap-to-scroll" };
        }
        return NONE;
      }
      case "zoom": {
        const z = current.zoom;
        // Update finger slots (matches Rust `font_delta`).
        const oldDistance = dist(z.a, z.b);
        if (sample.id === z.a.id) {
          z.a = sample;
        } else if (sample.id === z.b.id) {
          z.b = sample;
        }
        const newDistance = dist(z.a, z.b);
        const raw = (newDistance - oldDistance) * TOUCH_ZOOM_FACTOR + z.fractions;
        const stepCount = Math.floor(Math.abs(raw) / FONT_SIZE_STEP);
        const quantised = stepCount * FONT_SIZE_STEP * Math.sign(raw);
        z.fractions = raw - quantised;
        z.lastFontDelta = quantised;

        const distanceChange = Math.abs(newDistance - z.initialDistance);
        if (distanceChange < PINCH_COMMIT_THRESHOLD) {
          // Still ambiguous; check for two-finger pan.
          if (distanceChange < PINCH_COMMIT_THRESHOLD * 0.5) {
            const mid = {
              x: (z.a.x + z.b.x) * 0.5,
              y: (z.a.y + z.b.y) * 0.5,
            };
            const panDx = mid.x - z.initialMidpoint.x;
            const panDy = mid.y - z.initialMidpoint.y;
            if (Math.hypot(panDx, panDy) >= TWO_FINGER_PAN_THRESHOLD) {
              this.state = { kind: "two-finger-scroll", a: z.a, b: z.b };
              return { kind: "two-finger-scroll", dx: 0, dy: 0 };
            }
          }
          return NONE;
        }
        // Pinch committed.
        if (z.zone === "chrome-panel") {
          return { kind: "suppress-native-gesture" };
        }
        if (quantised === 0) return NONE;
        return {
          kind: "change-font-size",
          direction: quantised > 0 ? "increase" : "decrease",
        };
      }
      case "two-finger-scroll": {
        let last: TouchSample;
        if (sample.id === current.a.id) {
          last = current.a;
          current.a = sample;
        } else if (sample.id === current.b.id) {
          last = current.b;
          current.b = sample;
        } else {
          return NONE;
        }
        const dx = sample.x - last.x;
        const dy = sample.y - last.y;
        return { kind: "two-finger-scroll", dx, dy };
      }
      case "scroll": {
        const dy = sample.y - current.last.y;
        current.last = sample;
        return { kind: "scroll", dx: 0, dy, x: sample.x, y: sample.y };
      }
      case "select": {
        const { x, y } = clamp(sample, layout);
        return { kind: "update-mouse-position", x, y };
      }
    }
  }

  /** Drive on a RAF / interval loop with `nowMs = performance.now()`. */
  tickLongPress(nowMs: number, layout: TouchLayoutSize): TouchAction {
    const current = this.state;
    if (current.kind !== "tap") return NONE;
    if (current.start.timeMs === 0 || nowMs < current.start.timeMs) return NONE;
    if (nowMs - current.start.timeMs < LONG_PRESS_MS) return NONE;
    const start = current.start;
    this.state = { kind: "long-pressed", start };
    const { x, y } = clamp(start, layout);
    return { kind: "open-context-menu", x, y };
  }

  /** Feed a `touchend` / `touchcancel` sample. */
  end(sample: TouchSample, layout: TouchLayoutSize): TouchAction {
    const current = this.state;
    switch (current.kind) {
      case "none":
        return NONE;
      case "tap": {
        const start = current.start;
        this.state = { kind: "none" };
        const { x, y } = clamp(start, layout);
        return { kind: "end-simulated-left-click", x, y };
      }
      case "zoom": {
        const ids = new Set<number>([current.zoom.a.id, current.zoom.b.id]);
        ids.delete(sample.id);
        this.state = ids.size === 0 ? { kind: "none" } : { kind: "invalid", ids };
        return NONE;
      }
      case "two-finger-scroll": {
        const ids = new Set<number>([current.a.id, current.b.id]);
        ids.delete(sample.id);
        if (ids.size === 0) {
          this.state = { kind: "none" };
        } else {
          this.state = { kind: "invalid", ids };
        }
        return { kind: "end-scroll" };
      }
      case "long-pressed":
        this.state = { kind: "none" };
        return NONE;
      case "invalid":
        current.ids.delete(sample.id);
        if (current.ids.size === 0) this.state = { kind: "none" };
        return NONE;
      case "select":
        this.state = { kind: "none" };
        return { kind: "end-select" };
      case "scroll":
        this.state = { kind: "none" };
        return { kind: "end-scroll" };
    }
  }
}
