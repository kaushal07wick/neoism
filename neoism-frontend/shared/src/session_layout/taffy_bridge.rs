//! Pure helpers that bridge the desktop fork's `taffy`-driven
//! `ContextGrid` into the shared `SessionTree` model.
//!
//! Lifted from `frontends/neoism/src/layout/grid/mod.rs` free fns
//! 662-693. Nothing in here depends on taffy or PTY — they walk a
//! `SessionTreeNode` and emit route → leaf bookkeeping.

use super::tree::{SessionTreeLeafId, SessionTreeNode};

/// Walk `node`, recording every leaf with an `external_id` (the
/// desktop fork uses the route id) into both directions of a
/// route ↔ leaf map.
pub fn collect_session_tree_leaf_routes(
    node: &SessionTreeNode,
    route_to_leaf: &mut Vec<(usize, SessionTreeLeafId)>,
    leaf_to_route: &mut Vec<(SessionTreeLeafId, usize)>,
) {
    match node {
        SessionTreeNode::Leaf(leaf) => {
            if let Some(route_id) =
                leaf.external_id.and_then(|id| usize::try_from(id).ok())
            {
                route_to_leaf.push((route_id, leaf.id));
                leaf_to_route.push((leaf.id, route_id));
            }
        }
        SessionTreeNode::Split { children, .. }
        | SessionTreeNode::Tabbed { children, .. } => {
            for child in children {
                collect_session_tree_leaf_routes(child, route_to_leaf, leaf_to_route);
            }
        }
    }
}

/// First leaf id (depth-first preorder) inside `node`, or `None` if
/// the tree contains no leaves.
pub fn first_session_tree_leaf_id(node: &SessionTreeNode) -> Option<SessionTreeLeafId> {
    match node {
        SessionTreeNode::Leaf(leaf) => Some(leaf.id),
        SessionTreeNode::Split { children, .. }
        | SessionTreeNode::Tabbed { children, .. } => {
            children.iter().find_map(first_session_tree_leaf_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_layout::tree::{SessionLeafKind, SessionTreeLeaf, SessionTreeLeafId};

    fn leaf(id: u64, ext: Option<u64>) -> SessionTreeNode {
        SessionTreeNode::Leaf(SessionTreeLeaf {
            id: SessionTreeLeafId(id),
            kind: SessionLeafKind::Terminal,
            title: None,
            external_id: ext,
        })
    }

    #[test]
    fn first_leaf_walks_in_preorder() {
        let tree = SessionTreeNode::Split {
            axis: crate::session_layout::tree::SplitAxis::Horizontal,
            children: vec![leaf(1, None), leaf(2, None)],
            ratios: vec![0.5, 0.5],
        };
        assert_eq!(first_session_tree_leaf_id(&tree), Some(SessionTreeLeafId(1)));
    }

    #[test]
    fn collects_only_leaves_with_external_id() {
        let tree = SessionTreeNode::Split {
            axis: crate::session_layout::tree::SplitAxis::Horizontal,
            children: vec![leaf(1, Some(7)), leaf(2, None), leaf(3, Some(11))],
            ratios: vec![0.33, 0.33, 0.34],
        };
        let mut r2l = Vec::new();
        let mut l2r = Vec::new();
        collect_session_tree_leaf_routes(&tree, &mut r2l, &mut l2r);
        assert_eq!(r2l, vec![(7, SessionTreeLeafId(1)), (11, SessionTreeLeafId(3))]);
        assert_eq!(l2r, vec![(SessionTreeLeafId(1), 7), (SessionTreeLeafId(3), 11)]);
    }
}
