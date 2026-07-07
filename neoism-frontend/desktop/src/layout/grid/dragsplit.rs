//! Drag-to-split + drop-zone hit-testing for the desktop grid, driven
//! by the shared golden-standard solver
//! ([`neoism_ui::session_layout::geometry`]) over the canonical
//! [`SessionTree`] this grid already maintains.
//!
//! This is the desktop's adoption point for the shared pane brain: the
//! grid keeps its Taffy node-id storage, but pane geometry for
//! interaction (drop zones, dividers) now comes from the same solver the
//! web side will use — so drag-to-split behaves identically everywhere.

use super::ContextGrid;
use neoism_backend::event::EventListener;
use neoism_ui::layout::Rect;
use neoism_ui::session_layout::geometry::{self, DropPlacement};
use taffy::NodeId;

/// A resolved drag-to-split target: the panel under the pointer plus the
/// placement the dragged surface would take and a window-space highlight
/// rect for the live preview overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneDropTarget {
    pub node: NodeId,
    pub placement: DropPlacement,
    /// `[x, y, w, h]` in window space (scaled-margin already folded in).
    pub highlight: [f32; 4],
}

impl<T: EventListener> ContextGrid<T> {
    fn content_rect(&self) -> Rect {
        let w =
            (self.width - self.scaled_margin.left - self.scaled_margin.right).max(0.0);
        let h =
            (self.height - self.scaled_margin.top - self.scaled_margin.bottom).max(0.0);
        Rect::new(0.0, 0.0, w, h)
    }

    fn interaction_gap(&self) -> f32 {
        self.panel_config.column_gap * self.scale
    }

    /// Hit-test a drag-to-split drop at window coordinates `(x, y)`.
    ///
    /// Returns the panel `NodeId` under the pointer, the side the dragged
    /// surface would land on (edge → split, center → adopt-as-tab), and a
    /// window-space highlight rect for the live overlay.
    #[allow(dead_code)] // wired by the drag-input pass
    pub fn pane_drop_zone_at(&self, x: f32, y: f32) -> Option<PaneDropTarget> {
        let solved = geometry::solve(
            self.session_tree_snapshot(),
            self.content_rect(),
            self.interaction_gap(),
            3.0 * self.scale,
        );
        let adj_x = x - self.scaled_margin.left;
        let adj_y = y - self.scaled_margin.top;
        let zone = geometry::drop_zone_at(&solved, adj_x, adj_y, 0.25)?;
        let node = *self.leaf_to_node.get(&zone.target)?;
        let h = zone.highlight;
        Some(PaneDropTarget {
            node,
            placement: zone.placement,
            highlight: [
                h.x + self.scaled_margin.left,
                h.y + self.scaled_margin.top,
                h.w,
                h.h,
            ],
        })
    }

    /// Translate a dropped target into a split direction (or `None` for an
    /// adopt-as-tab center drop), so the caller can drive the existing
    /// `split_existing_*` / stack ops.
    #[allow(dead_code)] // wired by the drag-input pass
    pub fn drop_placement_to_split_down(placement: DropPlacement) -> Option<bool> {
        match placement {
            DropPlacement::Top | DropPlacement::Bottom => Some(true),
            DropPlacement::Left | DropPlacement::Right => Some(false),
            DropPlacement::Center => None,
        }
    }
}
