//! Pure pane hit-test helper.
//!
//! Lifted from `frontends/neoism/src/layout/grid/layout.rs::find_context_at_position`.
//! Operates on a list of (node_id, rect) pairs so the desktop fork can
//! filter to visible panels first and the web side can pass its own
//! pane handle type as `NodeId`.

/// Layout rectangle in the desktop fork's `[left, top, width, height]`
/// shape, relative to the grid's root container.
pub type LayoutRect = [f32; 4];

/// Find the first panel whose layout rect contains `(x, y)`, after
/// adjusting for the grid's scaled top-left margin. Walks `panels` in
/// order — callers are expected to pre-filter to visible panels.
pub fn find_context_at_position<NodeId: Copy>(
    x: f32,
    y: f32,
    scaled_margin_left: f32,
    scaled_margin_top: f32,
    panels: &[(NodeId, LayoutRect)],
) -> Option<NodeId> {
    let adj_x = x - scaled_margin_left;
    let adj_y = y - scaled_margin_top;

    for (node_id, [left, top, width, height]) in panels {
        if adj_x >= *left
            && adj_x < *left + *width
            && adj_y >= *top
            && adj_y < *top + *height
        {
            return Some(*node_id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_panel_at_point() {
        let panels = vec![(0u32, [0.0, 0.0, 100.0, 100.0]), (1u32, [100.0, 0.0, 50.0, 100.0])];
        assert_eq!(find_context_at_position(50.0, 50.0, 0.0, 0.0, &panels), Some(0));
        assert_eq!(find_context_at_position(125.0, 50.0, 0.0, 0.0, &panels), Some(1));
        assert_eq!(find_context_at_position(160.0, 50.0, 0.0, 0.0, &panels), None);
    }

    #[test]
    fn applies_scaled_margin_offset() {
        let panels = vec![(0u32, [0.0, 0.0, 100.0, 100.0])];
        // Point at (110, 110) lives outside the panel rect (0..100),
        // but after subtracting margin (10, 10) it falls inside.
        assert_eq!(
            find_context_at_position(110.0, 110.0, 10.0, 10.0, &panels),
            Some(0)
        );
    }
}
