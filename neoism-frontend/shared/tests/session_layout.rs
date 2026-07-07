use neoism_ui::session_layout::{
    active_tab_index_after_close, active_tab_move_target,
    active_tab_move_to_split_stack_plan, adjacent_tab_index, focused_tab_strip,
    nearest_horizontal_pane, ordered_secondary_routes_with_orphans,
    rebase_tab_index_after_move, rebase_tab_index_after_remove, SessionLayout,
    SessionLayoutError, SessionLeafKind, SessionLeafSpec, SessionMovableTabKind,
    SessionNode, SessionPaneRect, SessionTabMoveDestination, SessionTabMovePlan,
    SessionTabStripRef, SplitAxis, SplitPlacement,
};

fn terminal(external_id: u64) -> SessionLeafSpec {
    SessionLeafSpec::new(SessionLeafKind::Terminal).with_external_id(external_id)
}

fn editor(title: &str, external_id: u64) -> SessionLeafSpec {
    SessionLeafSpec::new(SessionLeafKind::Editor)
        .with_title(title)
        .with_external_id(external_id)
}

#[test]
fn split_focused_creates_renderer_independent_tree() {
    let mut layout = SessionLayout::new(terminal(10));
    let root = layout.focused_leaf();

    let split = layout
        .split_focused(
            SplitAxis::Horizontal,
            SplitPlacement::After,
            editor("lib.rs", 20),
        )
        .unwrap();

    assert_eq!(layout.active_leaves(), vec![root, split]);
    assert_eq!(layout.focused_leaf(), split);
    assert_eq!(layout.leaf(root).unwrap().external_id, Some(10));
    assert_eq!(layout.leaf(split).unwrap().external_id, Some(20));
    assert!(matches!(
        layout.node(layout.active_tab().root).unwrap(),
        SessionNode::Split(node) if node.axis == SplitAxis::Horizontal
    ));
    layout.validate().unwrap();
}

#[test]
fn split_focused_inserts_new_leaf_after_focused_leaf() {
    let mut layout = SessionLayout::new(terminal(10));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(20))
        .unwrap();
    layout.focus_leaf(first).unwrap();

    let inserted = layout
        .split_focused(
            SplitAxis::Horizontal,
            SplitPlacement::After,
            editor("main.rs", 30),
        )
        .unwrap();

    assert_eq!(layout.active_leaves(), vec![first, inserted, second]);
    assert_eq!(layout.focused_leaf(), inserted);
    let routes: Vec<_> = layout
        .active_leaves()
        .into_iter()
        .map(|leaf| layout.leaf(leaf).unwrap().external_id)
        .collect();
    assert_eq!(routes, vec![Some(10), Some(30), Some(20)]);
    layout.validate().unwrap();
}

#[test]
fn preview_split_focused_reports_route_order_without_mutating_layout() {
    let mut layout = SessionLayout::new(terminal(10));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(20))
        .unwrap();
    layout.focus_leaf(first).unwrap();

    let preview = layout
        .preview_split_focused(
            SplitAxis::Horizontal,
            SplitPlacement::After,
            editor("split.rs", 30),
        )
        .unwrap();

    assert_eq!(preview.focused_external_id_before, Some(10));
    assert_eq!(preview.focused_external_id_after, Some(30));
    assert_eq!(preview.active_external_ids_after, vec![10, 30, 20]);
    assert_eq!(layout.active_leaves(), vec![first, second]);
    assert_eq!(layout.focused_leaf(), first);
    layout.validate().unwrap();
}

#[test]
fn focus_moves_within_active_tab_without_wrapping() {
    let mut layout = SessionLayout::new(terminal(1));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(2))
        .unwrap();
    let third = layout
        .split_focused(SplitAxis::Horizontal, SplitPlacement::After, terminal(3))
        .unwrap();

    assert_eq!(layout.active_leaves(), vec![first, second, third]);
    assert_eq!(layout.focus_next_leaf(true).unwrap(), second);
    assert_eq!(layout.focus_next_leaf(true).unwrap(), first);
    assert_eq!(layout.focus_next_leaf(true).unwrap(), first);
    assert_eq!(layout.focus_next_leaf(false).unwrap(), second);
    assert_eq!(layout.focus_next_leaf(false).unwrap(), third);
    assert_eq!(layout.focus_next_leaf(false).unwrap(), third);
    layout.validate().unwrap();
}

#[test]
fn focus_adjacent_leaf_can_wrap_within_active_tab() {
    let mut layout = SessionLayout::new(terminal(1));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(2))
        .unwrap();
    let third = layout
        .split_focused(SplitAxis::Horizontal, SplitPlacement::After, terminal(3))
        .unwrap();

    assert_eq!(layout.active_leaves(), vec![first, second, third]);
    assert_eq!(layout.focus_adjacent_leaf(false, true).unwrap(), first);
    assert_eq!(layout.focus_adjacent_leaf(true, true).unwrap(), third);
    assert_eq!(layout.focus_adjacent_leaf(false, true).unwrap(), first);
    assert_eq!(layout.focus_adjacent_leaf(false, false).unwrap(), second);
    assert_eq!(layout.focus_adjacent_leaf(false, false).unwrap(), third);
    assert_eq!(layout.focus_adjacent_leaf(false, false).unwrap(), third);
    layout.validate().unwrap();
}

#[test]
fn focus_edge_leaf_uses_active_leaf_ordering() {
    let mut layout = SessionLayout::new(terminal(1));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(2))
        .unwrap();
    let third = layout
        .split_focused(SplitAxis::Horizontal, SplitPlacement::After, terminal(3))
        .unwrap();

    assert_eq!(layout.focus_edge_leaf(false).unwrap(), first);
    assert_eq!(layout.focused_leaf(), first);
    assert_eq!(layout.focus_edge_leaf(true).unwrap(), third);
    assert_eq!(layout.focused_leaf(), third);
    assert_eq!(layout.active_leaves(), vec![first, second, third]);
    layout.validate().unwrap();
}

#[test]
fn external_id_helpers_share_route_ordering_and_secondary_policy() {
    let mut layout = SessionLayout::new(terminal(10));
    let workspace = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(20))
        .unwrap();
    let third = layout
        .split_focused(SplitAxis::Horizontal, SplitPlacement::After, terminal(30))
        .unwrap();

    assert_eq!(layout.active_leaves(), vec![workspace, second, third]);
    assert_eq!(layout.active_leaf_external_ids(), vec![10, 20, 30]);
    assert_eq!(layout.external_ids_except(10), vec![20, 30]);
    assert_eq!(layout.external_ids_except(20), vec![10, 30]);
    assert_eq!(layout.first_external_id_except(10), Some(20));
    assert_eq!(layout.first_external_id_except(20), Some(10));

    layout.focus_leaf(workspace).unwrap();
    assert_eq!(layout.focused_external_id(), Some(10));
    assert_eq!(layout.focused_external_id_except(10), None);

    layout.focus_leaf(third).unwrap();
    assert_eq!(layout.focused_external_id(), Some(30));
    assert_eq!(layout.focused_external_id_except(10), Some(30));
    layout.validate().unwrap();
}

#[test]
fn close_leaf_promotes_sibling_and_keeps_focus_valid() {
    let mut layout = SessionLayout::new(terminal(1));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Horizontal, SplitPlacement::After, terminal(2))
        .unwrap();
    let third = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(3))
        .unwrap();

    assert_eq!(layout.active_leaves(), vec![first, second, third]);
    assert_eq!(layout.close_leaf(second).unwrap(), Some(third));
    assert_eq!(layout.active_leaves(), vec![first, third]);
    assert_eq!(layout.focused_leaf(), third);
    assert!(layout.leaf(second).is_none());
    layout.validate().unwrap();
}

#[test]
fn close_focused_leaf_uses_current_focus_as_close_target() {
    let mut layout = SessionLayout::new(terminal(1));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(2))
        .unwrap();

    layout.focus_leaf(first).unwrap();

    assert_eq!(layout.close_focused_leaf().unwrap(), Some(second));
    assert_eq!(layout.active_leaves(), vec![second]);
    assert_eq!(layout.focused_leaf(), second);
    assert!(layout.leaf(first).is_none());
    layout.validate().unwrap();
}

#[test]
fn close_only_leaf_is_rejected_but_closing_single_leaf_tab_selects_remaining_tab() {
    let mut layout = SessionLayout::new(terminal(1));
    let first = layout.focused_leaf();

    assert_eq!(layout.close_leaf(first), Err(SessionLayoutError::LastLeaf));
    layout.validate().unwrap();

    let second_tab_leaf = layout.add_tab(editor("notes.md", 2));
    assert_eq!(layout.active_tab_index(), 1);
    assert_eq!(layout.close_leaf(second_tab_leaf).unwrap(), Some(first));
    assert_eq!(layout.tabs().len(), 1);
    assert_eq!(layout.focused_leaf(), first);
    assert_eq!(layout.active_tab_index(), 0);
    layout.validate().unwrap();
}

#[test]
fn resize_split_toward_leaf_adjusts_and_clamps_ratio() {
    let mut layout = SessionLayout::new(terminal(1));
    let first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Horizontal, SplitPlacement::After, terminal(2))
        .unwrap();

    let ratio = layout
        .resize_split_toward_leaf(first, Some(SplitAxis::Horizontal), 0.20)
        .unwrap();
    assert!((ratio - 0.70).abs() < f32::EPSILON);

    let ratio = layout
        .resize_split_toward_leaf(second, Some(SplitAxis::Horizontal), 0.95)
        .unwrap();
    assert!((ratio - 0.10).abs() < f32::EPSILON);

    let err = layout
        .resize_split_toward_leaf(second, Some(SplitAxis::Vertical), 0.10)
        .unwrap_err();
    assert_eq!(err, SessionLayoutError::MissingLeaf(second));
    layout.validate().unwrap();
}

#[test]
fn resize_split_toward_leaf_uses_nearest_matching_axis() {
    let mut layout = SessionLayout::new(terminal(1));
    let _first = layout.focused_leaf();
    let second = layout
        .split_focused(SplitAxis::Horizontal, SplitPlacement::After, terminal(2))
        .unwrap();
    let third = layout
        .split_focused(SplitAxis::Vertical, SplitPlacement::After, terminal(3))
        .unwrap();

    let nested_ratio = layout
        .resize_split_toward_leaf(third, Some(SplitAxis::Vertical), 0.20)
        .unwrap();
    assert!((nested_ratio - 0.30).abs() < f32::EPSILON);

    let root_ratio = layout
        .resize_split_toward_leaf(third, Some(SplitAxis::Horizontal), 0.10)
        .unwrap();
    assert!((root_ratio - 0.40).abs() < f32::EPSILON);

    assert_eq!(layout.focused_leaf(), third);
    assert!(layout.leaf(second).is_some());
    layout.validate().unwrap();
}

#[test]
fn tab_navigation_wraps_and_rejects_invalid_current_index() {
    assert_eq!(adjacent_tab_index(4, 0, false), Some(1));
    assert_eq!(adjacent_tab_index(4, 3, false), Some(0));
    assert_eq!(adjacent_tab_index(4, 0, true), Some(3));
    assert_eq!(adjacent_tab_index(4, 2, true), Some(1));
    assert_eq!(adjacent_tab_index(0, 0, false), None);
    assert_eq!(adjacent_tab_index(3, 3, false), None);
}

#[test]
fn active_tab_move_target_matches_desktop_wrap_policy() {
    assert_eq!(active_tab_move_target(5, 0, false), Some(1));
    assert_eq!(active_tab_move_target(5, 4, false), Some(0));
    assert_eq!(active_tab_move_target(5, 0, true), Some(4));
    assert_eq!(active_tab_move_target(1, 0, true), None);
}

#[test]
fn close_tab_fallback_keeps_next_slot_or_previous_last_tab() {
    assert_eq!(active_tab_index_after_close(4, 0), Some(0));
    assert_eq!(active_tab_index_after_close(4, 2), Some(2));
    assert_eq!(active_tab_index_after_close(4, 3), Some(2));
    assert_eq!(active_tab_index_after_close(1, 0), None);
    assert_eq!(active_tab_index_after_close(3, 3), None);
}

#[test]
fn rebase_tab_index_after_remove_drops_removed_slot_and_shifts_tail() {
    assert_eq!(rebase_tab_index_after_remove(0, 2), Some(0));
    assert_eq!(rebase_tab_index_after_remove(1, 2), Some(1));
    assert_eq!(rebase_tab_index_after_remove(2, 2), None);
    assert_eq!(rebase_tab_index_after_remove(3, 2), Some(2));
    assert_eq!(rebase_tab_index_after_remove(4, 2), Some(3));
}

#[test]
fn rebase_tab_index_after_drag_reorder_keeps_same_workspace_focused() {
    assert_eq!(rebase_tab_index_after_move(2, 2, 4), 4);
    assert_eq!(rebase_tab_index_after_move(3, 1, 4), 2);
    assert_eq!(rebase_tab_index_after_move(1, 4, 1), 2);
    assert_eq!(rebase_tab_index_after_move(0, 4, 1), 0);
    assert_eq!(rebase_tab_index_after_move(4, 4, 1), 1);
}

#[test]
fn active_tab_move_to_split_stack_uses_existing_split_or_new_split() {
    assert_eq!(
        active_tab_move_to_split_stack_plan(
            SessionTabStripRef::Workspace,
            Some(42),
            SessionMovableTabKind::FileLike,
        ),
        SessionTabMovePlan {
            source: SessionTabStripRef::Workspace,
            destination: SessionTabMoveDestination::ExistingPane(42),
        }
    );
    assert_eq!(
        active_tab_move_to_split_stack_plan(
            SessionTabStripRef::Workspace,
            None,
            SessionMovableTabKind::AgentRoute,
        ),
        SessionTabMovePlan {
            source: SessionTabStripRef::Workspace,
            destination: SessionTabMoveDestination::NewSplit,
        }
    );
}

#[test]
fn active_tab_move_to_split_stack_moves_pane_tabs_back_to_workspace() {
    assert_eq!(
        active_tab_move_to_split_stack_plan(
            SessionTabStripRef::Pane(7),
            Some(42),
            SessionMovableTabKind::AgentTerminal,
        ),
        SessionTabMovePlan {
            source: SessionTabStripRef::Pane(7),
            destination: SessionTabMoveDestination::Workspace,
        }
    );
}

#[test]
fn ordered_secondary_routes_append_sorted_orphan_tab_strips() {
    assert_eq!(
        ordered_secondary_routes_with_orphans([20, 30, 20, 40], [50, 10, 30, 60]),
        vec![20, 30, 40, 10, 50, 60]
    );
    assert_eq!(
        ordered_secondary_routes_with_orphans([], [8, 4, 8, 2]),
        vec![2, 4, 8]
    );
}

#[test]
fn focused_tab_strip_uses_pane_strip_only_for_focused_pane_with_tabs() {
    assert_eq!(
        focused_tab_strip(Some(10), Some(10), [20, 30]),
        SessionTabStripRef::Workspace
    );
    assert_eq!(
        focused_tab_strip(Some(10), Some(20), [20, 30]),
        SessionTabStripRef::Pane(20)
    );
    assert_eq!(
        focused_tab_strip(Some(10), Some(40), [20, 30]),
        SessionTabStripRef::Workspace
    );
    assert_eq!(
        focused_tab_strip(Some(10), None, [20, 30]),
        SessionTabStripRef::Workspace
    );
}

#[test]
fn nearest_horizontal_pane_prefers_overlap_then_distance_then_center() {
    let current = SessionPaneRect::new(1, [100.0, 100.0, 100.0, 100.0]);
    let candidates = [
        SessionPaneRect::new(2, [220.0, 240.0, 100.0, 100.0]),
        SessionPaneRect::new(3, [260.0, 120.0, 100.0, 40.0]),
        SessionPaneRect::new(4, [210.0, 130.0, 100.0, 40.0]),
    ];

    assert_eq!(nearest_horizontal_pane(current, candidates, true), Some(4));
}

#[test]
fn nearest_horizontal_pane_uses_requested_direction_without_wrapping() {
    let current = SessionPaneRect::new(10, [100.0, 100.0, 100.0, 100.0]);
    let candidates = [
        SessionPaneRect::new(20, [230.0, 110.0, 100.0, 80.0]),
        SessionPaneRect::new(30, [-30.0, 110.0, 100.0, 80.0]),
        SessionPaneRect::new(40, [120.0, 110.0, 100.0, 80.0]),
    ];

    assert_eq!(nearest_horizontal_pane(current, candidates, true), Some(20));
    assert_eq!(
        nearest_horizontal_pane(current, candidates, false),
        Some(30)
    );
}
