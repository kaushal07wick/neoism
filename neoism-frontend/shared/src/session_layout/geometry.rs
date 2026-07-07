//! Golden-standard pane geometry + interaction solver.
//!
//! This is the *single* source of pane geometry shared by every
//! renderer (desktop Taffy mirror, web rect computation, and the
//! [`crate::panels::pane_grid::PaneGrid`] chrome piece). It walks the
//! canonical [`SessionTree`] and turns it into:
//!
//! * one [`PaneRect`] per visible leaf (Tabbed groups contribute only
//!   their active child),
//! * one [`DividerHandle`] per resizable gap (for divider-drag resize),
//! * drop-zone hit-testing for drag-to-split (Zed/VS Code style).
//!
//! Ratios are interpreted exactly as the desktop Taffy rebuild does:
//! `ratios` are *cumulative* shares for children `[0..len-1]`, the last
//! child gets the remainder. See `ratios_to_shares`.
//!
//! Renderer-neutral: depends only on [`crate::layout::Rect`] and the
//! tree model. No Sugarloaf / Taffy / PTY.

use crate::layout::Rect;
use crate::session_layout::tree::{
    NodePath, SessionTree, SessionTreeLeafId, SessionTreeNode,
};
use crate::session_layout::{SessionLeafKind, SplitAxis};

/// One visible pane: a leaf placed at a solved rect.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneRect {
    pub leaf: SessionTreeLeafId,
    pub external_id: Option<u64>,
    pub kind: SessionLeafKind,
    pub rect: Rect,
    pub focused: bool,
    /// Path to the leaf's node (or, for an active tab, the path of the
    /// `Tabbed` node hosting it). Used to map divider/resize ops back
    /// onto the tree.
    pub path: NodePath,
}

/// A resizable gap between two siblings in a split. Dragging it nudges
/// the cumulative ratio at `gap` of the split node at `split_path`.
#[derive(Clone, Debug, PartialEq)]
pub struct DividerHandle {
    pub split_path: NodePath,
    pub gap: usize,
    pub axis: SplitAxis,
    /// Hit/draw rect for the divider band (already inflated by the
    /// caller-supplied tolerance).
    pub rect: Rect,
}

/// Where a dragged tab/pane would land relative to a target pane.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DropPlacement {
    /// Split the target, new pane on the given side.
    Left,
    Right,
    Top,
    Bottom,
    /// Drop onto the target's center — adopt as a tab in that pane.
    Center,
}

/// Result of a drag-to-split hit test.
#[derive(Clone, Debug, PartialEq)]
pub struct DropZone {
    pub target: SessionTreeLeafId,
    pub target_path: NodePath,
    pub placement: DropPlacement,
    /// Highlight rect for the live drop preview (the region the dragged
    /// surface would occupy if released now).
    pub highlight: Rect,
}

/// Full solved layout for one content rect.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SolvedLayout {
    pub panes: Vec<PaneRect>,
    pub dividers: Vec<DividerHandle>,
}

/// Convert cumulative split ratios into per-child share weights summing
/// to ~1.0. Mirrors `layout::grid::rebuild::ratios_to_shares` on the
/// desktop side so both renderers agree to the float.
pub fn ratios_to_shares(ratios: &[f32], child_count: usize) -> Vec<f32> {
    if child_count <= 1 {
        return vec![1.0; child_count];
    }
    if ratios.is_empty() {
        let even = 1.0_f32 / child_count as f32;
        return vec![even; child_count];
    }
    let mut shares = Vec::with_capacity(child_count);
    let mut prev = 0.0_f32;
    for r in ratios.iter().take(child_count.saturating_sub(1)) {
        let share = (r - prev).max(0.0);
        shares.push(share);
        prev = *r;
    }
    shares.push((1.0 - prev).max(0.0));
    let total: f32 = shares.iter().sum();
    if total <= f32::EPSILON {
        let even = 1.0_f32 / child_count as f32;
        return vec![even; child_count];
    }
    for s in shares.iter_mut() {
        *s /= total;
    }
    shares
}

/// Layout knobs for [`solve_with`]. Mirrors the desktop
/// `config::layout::Panel`: per-axis gutter between siblings plus a
/// per-panel `margin` inset (the gap a panel keeps from its slot edges).
/// `divider_tol` inflates each divider band for easy grabbing. All-zero
/// (`SolveOpts::default`) reproduces the simple web behaviour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolveOpts {
    pub gap_x: f32,
    pub gap_y: f32,
    pub margin: f32,
    pub divider_tol: f32,
}

impl Default for SolveOpts {
    fn default() -> Self {
        Self {
            gap_x: 0.0,
            gap_y: 0.0,
            margin: 0.0,
            divider_tol: 0.0,
        }
    }
}

/// Simple solver: a single gutter on both axes, no per-panel margin.
/// Thin wrapper over [`solve_with`] kept for callers that don't model
/// panel margins (and for the existing tests).
pub fn solve(
    tree: &SessionTree,
    content: Rect,
    gap: f32,
    divider_tol: f32,
) -> SolvedLayout {
    solve_with(
        tree,
        content,
        &SolveOpts {
            gap_x: gap,
            gap_y: gap,
            margin: 0.0,
            divider_tol,
        },
    )
}

/// Solve `tree` into pane rects + divider handles within `content`,
/// honoring per-axis gaps and per-panel margins (the desktop parity
/// path). A lone visible pane fills `content` exactly (no margin),
/// matching the desktop single-pane override.
pub fn solve_with(tree: &SessionTree, content: Rect, opts: &SolveOpts) -> SolvedLayout {
    let mut out = SolvedLayout::default();
    let focus = tree.focus();
    walk(tree.root(), &mut Vec::new(), content, opts, focus, &mut out);
    // A single visible pane ignores margins and fills the content rect
    // (desktop `repair_single_visible_pane_rect`).
    if out.panes.len() == 1 {
        out.panes[0].rect = content;
    }
    out
}

fn walk(
    node: &SessionTreeNode,
    path: &mut NodePath,
    rect: Rect,
    opts: &SolveOpts,
    focus: SessionTreeLeafId,
    out: &mut SolvedLayout,
) {
    match node {
        SessionTreeNode::Leaf(leaf) => {
            // Inset the panel by its margin within the allocated slot.
            let m = opts.margin;
            let inset = Rect::new(
                rect.x + m,
                rect.y + m,
                (rect.w - m * 2.0).max(0.0),
                (rect.h - m * 2.0).max(0.0),
            );
            out.panes.push(PaneRect {
                leaf: leaf.id,
                external_id: leaf.external_id,
                kind: leaf.kind.clone(),
                rect: inset,
                focused: leaf.id == focus,
                path: path.clone(),
            });
        }
        SessionTreeNode::Tabbed { active, children } => {
            // Only the active child is visible; it fills the whole rect.
            let idx = (*active).min(children.len().saturating_sub(1));
            if let Some(child) = children.get(idx) {
                path.push(idx);
                walk(child, path, rect, opts, focus, out);
                path.pop();
            }
        }
        SessionTreeNode::Split {
            axis,
            children,
            ratios,
        } => {
            if children.is_empty() {
                return;
            }
            if children.len() == 1 {
                path.push(0);
                walk(&children[0], path, rect, opts, focus, out);
                path.pop();
                return;
            }
            let shares = ratios_to_shares(ratios, children.len());
            let n = children.len();
            let horizontal = matches!(axis, SplitAxis::Horizontal);
            let gap = if horizontal { opts.gap_x } else { opts.gap_y };
            let total_gap = gap * (n as f32 - 1.0);
            let main = if horizontal { rect.w } else { rect.h };
            let avail = (main - total_gap).max(0.0);

            let mut cursor = if horizontal { rect.x } else { rect.y };
            for (idx, child) in children.iter().enumerate() {
                let len = avail * shares.get(idx).copied().unwrap_or(0.0);
                let child_rect = if horizontal {
                    Rect::new(cursor, rect.y, len, rect.h)
                } else {
                    Rect::new(rect.x, cursor, rect.w, len)
                };
                path.push(idx);
                walk(child, path, child_rect, opts, focus, out);
                path.pop();

                // Emit a divider for the gap after every child but the last.
                if idx + 1 < n {
                    let tol = opts.divider_tol;
                    let band = if horizontal {
                        Rect::new(cursor + len - tol, rect.y, gap + tol * 2.0, rect.h)
                    } else {
                        Rect::new(rect.x, cursor + len - tol, rect.w, gap + tol * 2.0)
                    };
                    out.dividers.push(DividerHandle {
                        split_path: path.clone(),
                        gap: idx,
                        axis: *axis,
                        rect: band,
                    });
                }
                cursor += len + gap;
            }
        }
    }
}

/// Find the visible pane under `(x, y)`.
pub fn pane_at(solved: &SolvedLayout, x: f32, y: f32) -> Option<&PaneRect> {
    solved.panes.iter().find(|p| p.rect.contains(x, y))
}

/// Find the divider handle under `(x, y)`.
pub fn divider_at(solved: &SolvedLayout, x: f32, y: f32) -> Option<&DividerHandle> {
    solved.dividers.iter().find(|d| d.rect.contains(x, y))
}

/// Hit-test a drag-to-split drop. `edge_frac` is the fraction of the
/// pane's width/height (each side) that counts as an edge band; the
/// remaining center maps to [`DropPlacement::Center`] (adopt-as-tab).
/// A typical value is `0.25`.
pub fn drop_zone_at(
    solved: &SolvedLayout,
    x: f32,
    y: f32,
    edge_frac: f32,
) -> Option<DropZone> {
    let pane = pane_at(solved, x, y)?;
    let r = pane.rect;
    let edge_frac = edge_frac.clamp(0.0, 0.5);
    let ex = r.w * edge_frac;
    let ey = r.h * edge_frac;

    let from_left = x - r.x;
    let from_right = (r.x + r.w) - x;
    let from_top = y - r.y;
    let from_bottom = (r.y + r.h) - y;

    // Pick the closest edge if the pointer is inside any edge band,
    // preferring the smallest normalised distance so corners resolve
    // deterministically.
    let mut best: Option<(f32, DropPlacement)> = None;
    let consider = |dist: f32,
                    band: f32,
                    placement: DropPlacement,
                    best: &mut Option<(f32, DropPlacement)>| {
        if band > 0.0 && dist <= band {
            let norm = dist / band;
            if best.map(|(b, _)| norm < b).unwrap_or(true) {
                *best = Some((norm, placement));
            }
        }
    };
    consider(from_left, ex, DropPlacement::Left, &mut best);
    consider(from_right, ex, DropPlacement::Right, &mut best);
    consider(from_top, ey, DropPlacement::Top, &mut best);
    consider(from_bottom, ey, DropPlacement::Bottom, &mut best);

    let placement = best.map(|(_, p)| p).unwrap_or(DropPlacement::Center);
    let highlight = match placement {
        DropPlacement::Left => Rect::new(r.x, r.y, r.w * 0.5, r.h),
        DropPlacement::Right => Rect::new(r.x + r.w * 0.5, r.y, r.w * 0.5, r.h),
        DropPlacement::Top => Rect::new(r.x, r.y, r.w, r.h * 0.5),
        DropPlacement::Bottom => Rect::new(r.x, r.y + r.h * 0.5, r.w, r.h * 0.5),
        DropPlacement::Center => r,
    };
    Some(DropZone {
        target: pane.leaf,
        target_path: pane.path.clone(),
        placement,
        highlight,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_layout::tree::{SessionTreeLeaf, SessionTreeNode};
    use crate::session_layout::SessionLeafSpec;

    fn leaf(id: u64) -> SessionTreeNode {
        SessionTreeNode::Leaf(SessionTreeLeaf {
            id: SessionTreeLeafId(id),
            kind: SessionLeafKind::Terminal,
            title: None,
            external_id: Some(id),
        })
    }

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 0.01, "{a} != {b}");
    }

    #[test]
    fn single_leaf_fills_content() {
        let tree = SessionTree::new(SessionLeafSpec::new(SessionLeafKind::Terminal));
        let solved = solve(&tree, Rect::new(0.0, 0.0, 800.0, 600.0), 4.0, 3.0);
        assert_eq!(solved.panes.len(), 1);
        assert_eq!(solved.dividers.len(), 0);
        assert_eq!(solved.panes[0].rect, Rect::new(0.0, 0.0, 800.0, 600.0));
        assert!(solved.panes[0].focused);
    }

    #[test]
    fn horizontal_split_halves_width_minus_gap() {
        let root = SessionTreeNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![leaf(1), leaf(2)],
            ratios: vec![0.5],
        };
        let tree = SessionTree::from_root(root, SessionTreeLeafId(1)).unwrap();
        let solved = solve(&tree, Rect::new(0.0, 0.0, 800.0, 600.0), 4.0, 3.0);
        assert_eq!(solved.panes.len(), 2);
        // (800 - 4) / 2 = 398 each.
        approx(solved.panes[0].rect.w, 398.0);
        approx(solved.panes[1].rect.w, 398.0);
        approx(solved.panes[1].rect.x, 402.0);
        assert_eq!(solved.dividers.len(), 1);
        assert_eq!(solved.dividers[0].axis, SplitAxis::Horizontal);
        assert_eq!(solved.dividers[0].gap, 0);
    }

    #[test]
    fn three_way_cumulative_ratios() {
        // Cumulative: child0=0.25, child1=0.25, child2=0.50.
        let root = SessionTreeNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![leaf(1), leaf(2), leaf(3)],
            ratios: vec![0.25, 0.5],
        };
        let tree = SessionTree::from_root(root, SessionTreeLeafId(1)).unwrap();
        let solved = solve(&tree, Rect::new(0.0, 0.0, 1000.0, 600.0), 0.0, 2.0);
        approx(solved.panes[0].rect.w, 250.0);
        approx(solved.panes[1].rect.w, 250.0);
        approx(solved.panes[2].rect.w, 500.0);
        assert_eq!(solved.dividers.len(), 2);
    }

    #[test]
    fn tabbed_shows_only_active_child() {
        let root = SessionTreeNode::Tabbed {
            active: 1,
            children: vec![leaf(1), leaf(2), leaf(3)],
        };
        let tree = SessionTree::from_root(root, SessionTreeLeafId(2)).unwrap();
        let solved = solve(&tree, Rect::new(0.0, 0.0, 400.0, 300.0), 4.0, 3.0);
        assert_eq!(solved.panes.len(), 1);
        assert_eq!(solved.panes[0].leaf, SessionTreeLeafId(2));
        assert_eq!(solved.panes[0].rect, Rect::new(0.0, 0.0, 400.0, 300.0));
    }

    #[test]
    fn drop_zone_edges_and_center() {
        let tree = SessionTree::new(SessionLeafSpec::new(SessionLeafKind::Terminal));
        let solved = solve(&tree, Rect::new(0.0, 0.0, 800.0, 600.0), 0.0, 2.0);
        // Far left → split left.
        let z = drop_zone_at(&solved, 10.0, 300.0, 0.25).unwrap();
        assert_eq!(z.placement, DropPlacement::Left);
        assert_eq!(z.highlight, Rect::new(0.0, 0.0, 400.0, 600.0));
        // Center → adopt as tab.
        let z = drop_zone_at(&solved, 400.0, 300.0, 0.25).unwrap();
        assert_eq!(z.placement, DropPlacement::Center);
        // Bottom edge → split bottom.
        let z = drop_zone_at(&solved, 400.0, 590.0, 0.25).unwrap();
        assert_eq!(z.placement, DropPlacement::Bottom);
    }

    #[test]
    fn divider_hit_test_grabs_band() {
        let root = SessionTreeNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![leaf(1), leaf(2)],
            ratios: vec![0.5],
        };
        let tree = SessionTree::from_root(root, SessionTreeLeafId(1)).unwrap();
        let solved = solve(&tree, Rect::new(0.0, 0.0, 800.0, 600.0), 4.0, 4.0);
        // Divider sits around x≈398..406; grab at 400.
        let d = divider_at(&solved, 400.0, 300.0).expect("divider grabbed");
        assert_eq!(d.gap, 0);
    }
}
