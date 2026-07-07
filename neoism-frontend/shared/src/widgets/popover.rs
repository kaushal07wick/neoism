// Some API surface here (PopoverAnchor::Rect, EDGE_GAP/ANCHOR_GAP
// constants, helper accessors) is consumed by upcoming migrations
// rather than today's two call-sites — silence dead_code so the warning
// channel stays meaningful.
#![allow(dead_code)]

// Anchored overlay — `Popover<T>` is `Overlay<T>` plus the rules for
// placing the body relative to some anchor (a rect or a point). It
// handles flipping the popover so it stays inside the viewport, and
// hands the resolved rect back to the consumer's renderer.
//
// The popover itself doesn't paint; it computes a rect and delegates
// dismissal/animation primitives to its inner Overlay.

use crate::widgets::overlay::{Key, Overlay, OverlayKeyAction, OverlayState};

/// Where the popover wants to attach. `Rect` lets the popover decide
/// whether to sit above or below depending on available space; `Point`
/// is a hard-anchor (used by context menus that pop up at the mouse
/// position).
#[derive(Clone, Copy, Debug)]
pub enum PopoverAnchor {
    /// Anchor to a logical rect (e.g. a status-line pill). Popover
    /// prefers to render above; falls back below if there's no room.
    Rect([f32; 4]),
    /// Anchor to a single point. Popover renders with the point as its
    /// top-left, clamping inside the viewport.
    Point([f32; 2]),
}

/// Margin reserved between the popover edge and the viewport edge. The
/// popover never pokes into this gutter even when the anchor is in the
/// corner of the screen.
pub const POPOVER_EDGE_GAP: f32 = 8.0;

/// Gap between a `Rect` anchor and the popover body. Lets the eye see
/// the popover is attached to the pill / row, not stuck on top of it.
pub const POPOVER_ANCHOR_GAP: f32 = 6.0;

pub struct Popover<T> {
    overlay: Overlay<T>,
    anchor: PopoverAnchor,
}

impl<T> Popover<T> {
    pub fn new(content: T) -> Self {
        Self {
            overlay: Overlay::new(content),
            anchor: PopoverAnchor::Point([0.0, 0.0]),
        }
    }

    pub fn with_anim_ms(mut self, ms: f32) -> Self {
        self.overlay = self.overlay.with_anim_ms(ms);
        self
    }

    pub fn set_anchor(&mut self, anchor: PopoverAnchor) {
        self.anchor = anchor;
    }

    pub fn anchor(&self) -> PopoverAnchor {
        self.anchor
    }

    pub fn overlay(&self) -> &Overlay<T> {
        &self.overlay
    }

    pub fn overlay_mut(&mut self) -> &mut Overlay<T> {
        &mut self.overlay
    }

    pub fn content(&self) -> &T {
        self.overlay.content()
    }

    pub fn content_mut(&mut self) -> &mut T {
        self.overlay.content_mut()
    }

    pub fn state(&self) -> OverlayState {
        self.overlay.state()
    }

    pub fn is_visible(&self) -> bool {
        self.overlay.is_visible()
    }

    pub fn is_interactive(&self) -> bool {
        self.overlay.is_interactive()
    }

    pub fn is_animating(&self) -> bool {
        self.overlay.is_animating()
    }

    pub fn open(&mut self) {
        self.overlay.open();
    }

    pub fn open_instant(&mut self) {
        self.overlay.open_instant();
    }

    pub fn close(&mut self) {
        self.overlay.close();
    }

    pub fn close_instant(&mut self) {
        self.overlay.close_instant();
    }

    pub fn tick(&mut self, dt: f32) {
        self.overlay.tick(dt);
    }

    pub fn anim_t(&self) -> f32 {
        self.overlay.anim_t()
    }

    pub fn handle_key(&mut self, key: &Key) -> OverlayKeyAction {
        self.overlay.handle_key(key)
    }

    pub fn handle_click_outside(&mut self, point: [f32; 2], own_rect: [f32; 4]) -> bool {
        self.overlay.handle_click_outside(point, own_rect)
    }

    /// Compute the final `[x, y]` for a popover of the given `(w, h)`
    /// against the current anchor, clamped inside the viewport's
    /// `(window_w, window_h)`. Caller renders at the returned point.
    ///
    /// For a `Rect` anchor: prefers above the anchor (popover bottom
    /// sits just above the rect top); flips below when there's no room
    /// upward and the space below is larger.
    ///
    /// For a `Point` anchor: places the popover with its top-left at
    /// the point, then clamps so the rect stays inside `[edge_gap ..
    /// window_w - edge_gap]` and the equivalent for y.
    pub fn resolve_position(&self, size: (f32, f32), window: (f32, f32)) -> [f32; 2] {
        let (w, h) = size;
        let (ww, wh) = window;
        let edge = POPOVER_EDGE_GAP;
        let anchor_gap = POPOVER_ANCHOR_GAP;

        match self.anchor {
            PopoverAnchor::Point([px, py]) => {
                let x = px.clamp(edge, (ww - w - edge).max(edge));
                let y = py.clamp(edge, (wh - h - edge).max(edge));
                [x, y]
            }
            PopoverAnchor::Rect([rx, ry, rw, rh]) => {
                let cx = rx + rw / 2.0;
                let x = (cx - w / 2.0).clamp(edge, (ww - w - edge).max(edge));
                let above_room = ry - edge - anchor_gap;
                let below_room = wh - (ry + rh) - edge - anchor_gap;
                let y = if h <= above_room {
                    ry - h - anchor_gap
                } else if h <= below_room {
                    ry + rh + anchor_gap
                } else if above_room >= below_room {
                    // Cramped — pin above and let clip do its job.
                    (ry - h - anchor_gap).max(edge)
                } else {
                    (ry + rh + anchor_gap).min((wh - h - edge).max(edge))
                };
                [x, y]
            }
        }
    }
}

impl<T: Default> Default for Popover<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
