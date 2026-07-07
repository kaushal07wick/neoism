//! PR2b: rebuild a Taffy layout tree purely from a `SessionTree`.
//!
//! This is the inverse of the PR2a dual-write: instead of letting the
//! Taffy tree be authoritative and mirroring it into `SessionTree`, this
//! module walks the (canonical) `SessionTree` and produces a fresh
//! `TaffyTree` plus the leaf↔node map that ContextGrid stores.
//!
//! The intent is to flip mutations one at a time: a caller mutates the
//! shared `SessionTree` first, then asks `rebuild_taffy_from_tree` to
//! produce the new Taffy structure, then splices the new nodes back
//! into the live `ContextGrid` (preserving the `ContextGridItem` keyed
//! by `NodeId`).
//!
//! The legacy split path between this rebuild and the live tree exists
//! so that PR2b can be merged incrementally: each individual mutation
//! can either keep the dual-write or flip to the rebuild — and a debug
//! helper (`assert_rebuild_matches_taffy`) compares the two outputs so a
//! parity drift fails loud rather than silently corrupting state.

use neoism_backend::config::layout::Panel;
use neoism_ui::session_layout::tree::{
    SessionTree, SessionTreeLeafId, SessionTreeNode, SplitAxis,
};
use rustc_hash::FxHashMap;
use taffy::{
    geometry, style_helpers::length, Display, FlexDirection, NodeId, Style, TaffyTree,
};

/// Output of [`rebuild_taffy_from_tree`].
///
/// `tree` is a freshly allocated `TaffyTree` whose `root` mirrors the
/// outer container that `ContextGrid::new` creates (a flex container
/// sized to the available space, with one child that materialises the
/// SessionTree root). `leaf_to_node` maps every leaf id in the
/// `SessionTree` to the Taffy node id that holds its panel style. The
/// caller is responsible for inserting their existing
/// `ContextGridItem`s into the map keyed by these new node ids.
pub struct RebuildResult {
    pub tree: TaffyTree<()>,
    pub root: NodeId,
    pub leaf_to_node: FxHashMap<SessionTreeLeafId, NodeId>,
}

/// Build a fresh `TaffyTree` from `session_tree` using `panel_config`
/// and the supplied device `scale`.
///
/// The produced tree intentionally mirrors the shape produced by
/// `ContextGrid::new` + a sequence of `split_panel` calls: the outer
/// "root_node" is a flex container, its single child is the materialised
/// SessionTree root. Splits become intermediate flex containers whose
/// `flex_direction` matches the split axis; leaves become panel-style
/// flex leaves. Cumulative `ratios` are inverted into per-child
/// `flex_grow` weights so the resulting layout reproduces the original
/// proportions when Taffy distributes free space.
///
/// `Tabbed` nodes intentionally collapse to their active child for the
/// Taffy structural shape: stacked tabs are tracked separately by the
/// host via `stacked_parents`/`active_stacked_by_parent`, so the Taffy
/// tree never models them as siblings.
pub fn rebuild_taffy_from_tree(
    session_tree: &SessionTree,
    panel_config: &Panel,
    scale: f32,
    available_width: f32,
    available_height: f32,
) -> RebuildResult {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let mut leaf_to_node = FxHashMap::default();

    let root_style = Style {
        display: Display::Flex,
        gap: geometry::Size {
            width: length(panel_config.column_gap * scale),
            height: length(panel_config.row_gap * scale),
        },
        size: geometry::Size {
            width: length(available_width),
            height: length(available_height),
        },
        ..Default::default()
    };
    let root = tree
        .new_leaf(root_style)
        .expect("failed to create rebuild root");

    let inner = build_node(
        &mut tree,
        session_tree.root(),
        panel_config,
        scale,
        &mut leaf_to_node,
    );
    tree.add_child(root, inner)
        .expect("failed to attach inner root to rebuild root");

    RebuildResult {
        tree,
        root,
        leaf_to_node,
    }
}

fn build_node(
    tree: &mut TaffyTree<()>,
    node: &SessionTreeNode,
    panel_config: &Panel,
    scale: f32,
    leaf_to_node: &mut FxHashMap<SessionTreeLeafId, NodeId>,
) -> NodeId {
    match node {
        SessionTreeNode::Leaf(leaf) => {
            let panel_node = tree
                .new_leaf(create_panel_style(panel_config, scale))
                .expect("failed to create rebuild leaf");
            leaf_to_node.insert(leaf.id, panel_node);
            panel_node
        }
        SessionTreeNode::Split {
            axis,
            children,
            ratios,
        } => {
            if children.is_empty() {
                // Should not happen in a validated SessionTree, but fall
                // back to an empty container rather than panicking.
                return tree
                    .new_leaf(container_style(panel_config, scale, *axis))
                    .expect("failed to create empty container");
            }
            if children.len() == 1 {
                return build_node(tree, &children[0], panel_config, scale, leaf_to_node);
            }

            let container = tree
                .new_leaf(container_style(panel_config, scale, *axis))
                .expect("failed to create split container");

            // ratios are cumulative shares for children [0..len-1]; the
            // last child gets the remainder. Invert to per-child weights
            // so Taffy `flex_grow` reproduces the original proportions.
            let shares = ratios_to_shares(ratios, children.len());

            for (idx, child) in children.iter().enumerate() {
                let child_node =
                    build_node(tree, child, panel_config, scale, leaf_to_node);
                // Stamp the child's flex weight so its slot inside the
                // container matches the cumulative ratio it came from.
                let weight = shares.get(idx).copied().unwrap_or(1.0).max(f32::EPSILON);
                if let Ok(mut style) = tree.style(child_node).cloned() {
                    style.flex_basis = length(0.0);
                    style.flex_grow = weight;
                    style.flex_shrink = 1.0;
                    let _ = tree.set_style(child_node, style);
                }
                tree.add_child(container, child_node)
                    .expect("failed to add child to split container");
            }
            container
        }
        SessionTreeNode::Tabbed { active, children } => {
            // Tabs are tracked outside the Taffy structural tree by the
            // host (see `stacked_parents`/`active_stacked_by_parent`).
            // The Taffy shape mirrors only the visible active child;
            // hidden tab leaves are still mapped to their own panel
            // nodes (created lazily here) so the caller can attach
            // existing ContextGridItems to them.
            let active_idx = (*active).min(children.len().saturating_sub(1));
            let main = if let Some(child) = children.get(active_idx) {
                build_node(tree, child, panel_config, scale, leaf_to_node)
            } else {
                tree.new_leaf(create_panel_style(panel_config, scale))
                    .expect("failed to create empty tabbed slot")
            };
            for (idx, child) in children.iter().enumerate() {
                if idx == active_idx {
                    continue;
                }
                // Allocate panel nodes for hidden tabs but do not add
                // them as Taffy children — the host owns their stacked
                // visibility outside Taffy.
                let _ = build_node(tree, child, panel_config, scale, leaf_to_node);
            }
            main
        }
    }
}

fn create_panel_style(panel_config: &Panel, scale: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_grow: 1.0,
        flex_shrink: 1.0,
        padding: geometry::Rect {
            left: length(panel_config.padding.left * scale),
            right: length(panel_config.padding.right * scale),
            top: length(panel_config.padding.top * scale),
            bottom: length(panel_config.padding.bottom * scale),
        },
        margin: geometry::Rect {
            left: length(panel_config.margin.left * scale),
            right: length(panel_config.margin.right * scale),
            top: length(panel_config.margin.top * scale),
            bottom: length(panel_config.margin.bottom * scale),
        },
        ..Default::default()
    }
}

fn container_style(panel_config: &Panel, scale: f32, axis: SplitAxis) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: match axis {
            SplitAxis::Horizontal => FlexDirection::Row,
            SplitAxis::Vertical => FlexDirection::Column,
        },
        flex_grow: 1.0,
        flex_shrink: 1.0,
        gap: geometry::Size {
            width: length(panel_config.column_gap * scale),
            height: length(panel_config.row_gap * scale),
        },
        ..Default::default()
    }
}

/// Convert cumulative split ratios into per-child share weights. The
/// inverse of `session_tree_ratios_from_sizes`. Returns a vector of
/// length `child_count` whose entries sum to ~1.0 (degenerate inputs
/// fall back to even shares).
fn ratios_to_shares(ratios: &[f32], child_count: usize) -> Vec<f32> {
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

/// Debug-only parity check: compare the live `TaffyTree` shape against
/// what `rebuild_taffy_from_tree` would have produced from the stored
/// `SessionTree`. Differences here mean the PR2b inversion drifted from
/// the original Taffy authority — the caller should stop and report
/// rather than ship a broken inversion.
///
/// Currently compares the structural shape (split/leaf parent chain
/// per leaf id and the axis at each split). It deliberately ignores
/// concrete ratio values because Taffy's solved `flex_grow` weights
/// only round-trip through ratios up to floating-point precision and
/// any clamp inside `MIN_SPLIT_RATIO..MAX_SPLIT_RATIO`.
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub fn assert_rebuild_matches_taffy<T>(
    live_tree: &TaffyTree<T>,
    live_root: NodeId,
    live_leaf_to_node: &FxHashMap<SessionTreeLeafId, NodeId>,
    session_tree: &SessionTree,
    panel_config: &Panel,
    scale: f32,
    available_width: f32,
    available_height: f32,
) {
    let rebuilt = rebuild_taffy_from_tree(
        session_tree,
        panel_config,
        scale,
        available_width,
        available_height,
    );

    for leaf in session_tree.all_leaves() {
        let live_node = match live_leaf_to_node.get(&leaf).copied() {
            Some(node) => node,
            None => continue, // host may have stacked-tab leaves not in taffy
        };
        let rebuilt_node = match rebuilt.leaf_to_node.get(&leaf).copied() {
            Some(node) => node,
            None => panic!(
                "rebuild_taffy_from_tree dropped leaf {:?} present in live tree",
                leaf
            ),
        };
        let live_axes = ancestor_axes(live_tree, live_node, live_root);
        let rebuilt_axes = ancestor_axes(&rebuilt.tree, rebuilt_node, rebuilt.root);
        assert_eq!(
            live_axes, rebuilt_axes,
            "rebuild parity drift: leaf {:?} ancestor split axes differ \
             (live={:?}, rebuilt={:?})",
            leaf, live_axes, rebuilt_axes
        );
    }
}

#[cfg(debug_assertions)]
fn ancestor_axes<T>(
    tree: &TaffyTree<T>,
    mut node: NodeId,
    root: NodeId,
) -> Vec<SplitAxis> {
    let mut out = Vec::new();
    while node != root {
        let Some(parent) = tree.parent(node) else {
            break;
        };
        if parent != root {
            if let Ok(style) = tree.style(parent) {
                let axis = match style.flex_direction {
                    FlexDirection::Row | FlexDirection::RowReverse => {
                        Some(SplitAxis::Horizontal)
                    }
                    FlexDirection::Column | FlexDirection::ColumnReverse => {
                        Some(SplitAxis::Vertical)
                    }
                };
                if let Some(axis) = axis {
                    out.push(axis);
                }
            }
        }
        node = parent;
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoism_ui::session_layout::tree::{
        SessionLeafKind, SessionLeafSpec, SessionTree, SessionTreeLeaf,
        SessionTreeLeafId, SessionTreeNode, SplitAxis,
    };

    fn leaf(id: u64, ext: u64) -> SessionTreeNode {
        SessionTreeNode::Leaf(SessionTreeLeaf {
            id: SessionTreeLeafId(id),
            kind: SessionLeafKind::Terminal,
            title: None,
            external_id: Some(ext),
        })
    }

    fn panel() -> Panel {
        Panel::default()
    }

    #[test]
    fn rebuild_single_leaf() {
        let spec = SessionLeafSpec::new(SessionLeafKind::Terminal).with_external_id(7);
        let session_tree = SessionTree::new(spec);
        let result = rebuild_taffy_from_tree(&session_tree, &panel(), 1.0, 800.0, 600.0);
        assert_eq!(result.leaf_to_node.len(), 1);
        // root has exactly one child (the leaf panel).
        let children = result.tree.children(result.root).unwrap();
        assert_eq!(children.len(), 1);
        let (&leaf_id, &node_id) = result.leaf_to_node.iter().next().unwrap();
        assert_eq!(leaf_id, session_tree.focus());
        assert_eq!(children[0], node_id);
    }

    #[test]
    fn rebuild_horizontal_split() {
        let root = SessionTreeNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![leaf(1, 10), leaf(2, 20)],
            ratios: vec![0.5],
        };
        let session_tree =
            SessionTree::from_root(root, SessionTreeLeafId(1)).expect("valid tree");
        let result = rebuild_taffy_from_tree(&session_tree, &panel(), 1.0, 800.0, 600.0);
        assert_eq!(result.leaf_to_node.len(), 2);
        let outer = result.tree.children(result.root).unwrap();
        assert_eq!(outer.len(), 1);
        let container = outer[0];
        let style = result.tree.style(container).unwrap();
        assert_eq!(style.flex_direction, FlexDirection::Row);
        let inner_children = result.tree.children(container).unwrap();
        assert_eq!(inner_children.len(), 2);
    }

    #[test]
    fn rebuild_three_way_split_assigns_per_child_weights() {
        // Cumulative ratios [0.3, 0.7] -> per-child shares [0.3, 0.4, 0.3].
        let root = SessionTreeNode::Split {
            axis: SplitAxis::Vertical,
            children: vec![leaf(1, 1), leaf(2, 2), leaf(3, 3)],
            ratios: vec![0.3, 0.7],
        };
        let session_tree =
            SessionTree::from_root(root, SessionTreeLeafId(1)).expect("valid tree");
        let result = rebuild_taffy_from_tree(&session_tree, &panel(), 1.0, 800.0, 600.0);
        let container = result.tree.children(result.root).unwrap()[0];
        let style = result.tree.style(container).unwrap();
        assert_eq!(style.flex_direction, FlexDirection::Column);
        let inner = result.tree.children(container).unwrap();
        assert_eq!(inner.len(), 3);
        let grows = inner
            .iter()
            .map(|n| result.tree.style(*n).unwrap().flex_grow)
            .collect::<Vec<_>>();
        let sum: f32 = grows.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-4,
            "weights should sum to ~1.0: {:?}",
            grows
        );
        assert!((grows[0] - 0.3).abs() < 1e-4);
        assert!((grows[1] - 0.4).abs() < 1e-4);
        assert!((grows[2] - 0.3).abs() < 1e-4);
    }

    #[test]
    fn rebuild_nested_splits_register_every_leaf() {
        // Horizontal split: [leaf1, (vertical split: [leaf2, leaf3])]
        let inner = SessionTreeNode::Split {
            axis: SplitAxis::Vertical,
            children: vec![leaf(2, 20), leaf(3, 30)],
            ratios: vec![0.5],
        };
        let root = SessionTreeNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![leaf(1, 10), inner],
            ratios: vec![0.5],
        };
        let session_tree =
            SessionTree::from_root(root, SessionTreeLeafId(1)).expect("valid tree");
        let result = rebuild_taffy_from_tree(&session_tree, &panel(), 1.0, 800.0, 600.0);
        assert_eq!(result.leaf_to_node.len(), 3);
        for id in [1u64, 2, 3] {
            assert!(
                result.leaf_to_node.contains_key(&SessionTreeLeafId(id)),
                "leaf {} missing from rebuild map",
                id
            );
        }
    }

    #[test]
    fn rebuild_tabbed_keeps_only_active_in_taffy() {
        // Tabbed with two leaves; active=1 (the second). Both leaves
        // should still be registered in `leaf_to_node` so hosts can map
        // stacked items, but the Taffy structural tree only includes
        // the active child.
        let root = SessionTreeNode::Tabbed {
            active: 1,
            children: vec![leaf(1, 10), leaf(2, 20)],
        };
        let session_tree =
            SessionTree::from_root(root, SessionTreeLeafId(2)).expect("valid tree");
        let result = rebuild_taffy_from_tree(&session_tree, &panel(), 1.0, 800.0, 600.0);
        assert_eq!(result.leaf_to_node.len(), 2);
        let outer = result.tree.children(result.root).unwrap();
        assert_eq!(outer.len(), 1);
        // The active leaf is what the outer root points at.
        let active_node = result.leaf_to_node[&SessionTreeLeafId(2)];
        assert_eq!(outer[0], active_node);
    }
}
