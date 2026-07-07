use super::rebuild::rebuild_taffy_from_tree;
use super::{session_leaf_spec_for_grid_item, ContextGrid, ContextGridItem};
use crate::context::Context;
use neoism_backend::event::EventListener;
use neoism_backend::sugarloaf::Sugarloaf;
use neoism_ui::session_layout::{
    SessionLeafKind, SessionLeafSpec, SplitAxis, SplitPlacement,
};
use rustc_hash::FxHashMap;
use taffy::{geometry, style_helpers::length, NodeId, TaffyError};

impl<T: EventListener> ContextGrid<T> {
    pub(crate) fn clear_stack_metadata(&mut self, node: NodeId) {
        let old_parent = self.stacked_parents.remove(&node);
        self.stacked_nodes.retain(|stacked| *stacked != node);
        if self.active_stacked == Some(node) {
            self.active_stacked = None;
        }
        if let Some(parent) = old_parent {
            if self.active_stacked_by_parent.get(&parent).copied() == Some(node) {
                self.active_stacked_by_parent.remove(&parent);
            }
        }
        self.active_stacked_by_parent
            .retain(|parent, active| *parent != node && *active != node);
    }

    pub(crate) fn set_active_stacked_for_parent(&mut self, parent: NodeId, node: NodeId) {
        if Some(parent) == self.root {
            self.active_stacked = Some(node);
        } else {
            self.active_stacked_by_parent.insert(parent, node);
        }
    }

    pub(crate) fn clear_active_stacked_for_parent(&mut self, parent: NodeId) {
        if Some(parent) == self.root {
            self.active_stacked = None;
        } else {
            self.active_stacked_by_parent.remove(&parent);
        }
    }

    pub(crate) fn try_split_right(&mut self) -> Result<NodeId, TaffyError> {
        self.split_panel(taffy::FlexDirection::Row)
    }

    pub(crate) fn try_split_down(&mut self) -> Result<NodeId, TaffyError> {
        self.split_panel(taffy::FlexDirection::Column)
    }

    /// SessionTree-first split (PR2c). Mutates the canonical
    /// [`SessionTree`] via `split_focused`, rebuilds Taffy from the
    /// resulting tree, then splices the existing ContextGridItem map
    /// onto the freshly-allocated NodeIds via [`Self::splice_rebuild`].
    ///
    /// Returns the new pane's Taffy `NodeId` so the caller can insert a
    /// [`ContextGridItem`] keyed by it.
    pub(crate) fn split_panel(
        &mut self,
        direction: taffy::FlexDirection,
    ) -> Result<NodeId, TaffyError> {
        let previous_current = self.current;
        if !self.inner.contains_key(&previous_current) {
            return Err(TaffyError::InvalidInputNode(self.root_node));
        }

        // Focus the ACTUAL current leaf (even if it's a stacked tab) and
        // let `SessionTree::split_focused` wrap the enclosing Tabbed group
        // so the new pane lands BESIDE the tab group, not nested inside the
        // visible tab. This keeps the source pane on the tab the user was
        // viewing (no reveal-of-host) AND makes the new split pane visible.
        let focus_target_node = previous_current;
        let focus_target_leaf = *self
            .node_to_leaf
            .get(&focus_target_node)
            .ok_or(TaffyError::InvalidInputNode(focus_target_node))?;
        if self.session_tree.focus_leaf(focus_target_leaf).is_err() {
            return Err(TaffyError::InvalidInputNode(focus_target_node));
        }

        let axis = match direction {
            taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse => {
                SplitAxis::Horizontal
            }
            taffy::FlexDirection::Column | taffy::FlexDirection::ColumnReverse => {
                SplitAxis::Vertical
            }
        };

        // Allocate a placeholder spec for the new leaf. The caller will
        // insert the real Context into `inner` after this returns; the
        // next structural sync re-derives the spec from that item.
        let new_spec = SessionLeafSpec::new(SessionLeafKind::Terminal);
        let prev_node_to_leaf = self.node_to_leaf.clone();
        let outcome = self
            .session_tree
            .split_focused(axis, SplitPlacement::After, new_spec)
            .map_err(|_| TaffyError::InvalidInputNode(focus_target_node))?;

        let available_width =
            self.width - self.scaled_margin.left - self.scaled_margin.right;
        let available_height =
            self.height - self.scaled_margin.top - self.scaled_margin.bottom;
        let rebuilt = rebuild_taffy_from_tree(
            &self.session_tree,
            &self.panel_config,
            self.scale,
            available_width,
            available_height,
        );

        // Re-establish outer-root sizing so layout matches the rest of
        // the grid (rebuild seeds size from inputs which we just
        // computed above).
        let mut root_style = match rebuilt.tree.style(rebuilt.root) {
            Ok(s) => s.clone(),
            Err(err) => return Err(err),
        };
        root_style.size = geometry::Size {
            width: length(available_width),
            height: length(available_height),
        };

        // The newly created leaf has no ContextGridItem yet — the caller
        // (split_right/split_down) will insert one keyed by the returned
        // NodeId. We pass an empty additional_items map.
        let new_node_id = *rebuilt
            .leaf_to_node
            .get(&outcome.new_leaf)
            .ok_or(TaffyError::InvalidInputNode(focus_target_node))?;

        self.splice_rebuild(
            rebuilt,
            &prev_node_to_leaf,
            outcome.new_leaf,
            FxHashMap::default(),
        );

        // After splice, restore the proper root_node size that
        // splice_rebuild may have left at the rebuild's default.
        let _ = self.tree.set_style(self.root_node, root_style);

        Ok(new_node_id)
    }

    /// SessionTree-first split that takes a pre-existing leaf node and
    /// elevates it out of its current stacked position into a real
    /// split sibling of the focused panel.
    pub(crate) fn split_panel_with_existing_node(
        &mut self,
        node: NodeId,
        direction: taffy::FlexDirection,
    ) -> Result<NodeId, TaffyError> {
        if !self.inner.contains_key(&node) || !self.is_stacked_node(node) {
            return Err(TaffyError::InvalidInputNode(node));
        }
        let previous_current = self.current;
        if !self.inner.contains_key(&previous_current) || previous_current == node {
            return Err(TaffyError::InvalidInputNode(previous_current));
        }

        // Look up the leaf ids for both nodes so we can mutate the
        // canonical SessionTree.
        let host_target = if self.is_stacked_node(previous_current) {
            self.stacked_parents
                .get(&previous_current)
                .copied()
                .unwrap_or_else(|| self.root.unwrap_or(previous_current))
        } else {
            previous_current
        };
        let host_leaf = *self
            .node_to_leaf
            .get(&host_target)
            .ok_or(TaffyError::InvalidInputNode(host_target))?;
        let moving_leaf = *self
            .node_to_leaf
            .get(&node)
            .ok_or(TaffyError::InvalidInputNode(node))?;

        // Remove the moving leaf from its current (Tabbed) position in
        // the SessionTree, then split the host with the moving leaf as
        // its new sibling.
        let prev_node_to_leaf = self.node_to_leaf.clone();
        let axis = match direction {
            taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse => {
                SplitAxis::Horizontal
            }
            taffy::FlexDirection::Column | taffy::FlexDirection::ColumnReverse => {
                SplitAxis::Vertical
            }
        };

        // Detach the moving leaf from its current tabbed home, keeping
        // its full spec/title/external_id so we can restore them after
        // the placeholder-based split.
        let detached = self.session_tree.detach_leaf(moving_leaf).map_err(|err| {
            tracing::warn!(?err, "could not detach stacked leaf from session tree");
            TaffyError::InvalidInputNode(node)
        })?;

        // Focus the host leaf and split it with a placeholder carrying
        // the detached leaf's spec; we'll then rename the placeholder
        // back to the moving leaf's id so adapters keep their route
        // mappings.
        if self.session_tree.focus_leaf(host_leaf).is_err() {
            return Err(TaffyError::InvalidInputNode(host_target));
        }
        let carried_spec = SessionLeafSpec {
            kind: detached.kind.clone(),
            title: detached.title.clone(),
            external_id: detached.external_id,
        };
        let outcome = self
            .session_tree
            .split_focused(axis, SplitPlacement::After, carried_spec)
            .map_err(|_| TaffyError::InvalidInputNode(host_target))?;

        // Replace the placeholder leaf id with the moving leaf's id.
        let placeholder_leaf_id = outcome.new_leaf;
        self.session_tree
            .replace_leaf_id(placeholder_leaf_id, moving_leaf)
            .map_err(|_| TaffyError::InvalidInputNode(node))?;
        // Refocus on the moved leaf (now the focused pane).
        let _ = self.session_tree.focus_leaf(moving_leaf);

        let available_width =
            self.width - self.scaled_margin.left - self.scaled_margin.right;
        let available_height =
            self.height - self.scaled_margin.top - self.scaled_margin.bottom;
        let rebuilt = rebuild_taffy_from_tree(
            &self.session_tree,
            &self.panel_config,
            self.scale,
            available_width,
            available_height,
        );
        let mut root_style = match rebuilt.tree.style(rebuilt.root) {
            Ok(s) => s.clone(),
            Err(err) => return Err(err),
        };
        root_style.size = geometry::Size {
            width: length(available_width),
            height: length(available_height),
        };
        let new_node_id = *rebuilt
            .leaf_to_node
            .get(&moving_leaf)
            .ok_or(TaffyError::InvalidInputNode(node))?;

        self.splice_rebuild(
            rebuilt,
            &prev_node_to_leaf,
            moving_leaf,
            FxHashMap::default(),
        );
        let _ = self.tree.set_style(self.root_node, root_style);

        Ok(new_node_id)
    }

    #[allow(dead_code)] // superseded by tree.set_ratio; Taffy sizing retired
    pub(crate) fn set_panel_size(
        &mut self,
        node: NodeId,
        width: Option<f32>,
        height: Option<f32>,
    ) -> Result<(), TaffyError> {
        let mut style = self.tree.style(node)?.clone();

        // Use flex_grow proportional to the desired size so panels
        // scale correctly when the window is resized.
        if let Some(w) = width {
            style.flex_basis = length(0.0);
            style.flex_grow = w;
            style.flex_shrink = 1.0;
        } else if let Some(h) = height {
            style.flex_basis = length(0.0);
            style.flex_grow = h;
            style.flex_shrink = 1.0;
        }

        self.tree.set_style(node, style)?;
        Ok(())
    }

    /// Reset all panels to flexible sizing so they expand to fill available space
    /// Reset all nodes (panels and containers) to flexible sizing.
    pub(crate) fn reset_panel_styles_to_flexible(&mut self) {
        let mut stack = vec![self.root_node];
        while let Some(node) = stack.pop() {
            if let Ok(mut style) = self.tree.style(node).cloned() {
                style.flex_basis = taffy::Dimension::auto();
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                let _ = self.tree.set_style(node, style);
            }
            if let Ok(children) = self.tree.children(node) {
                for child in children {
                    stack.push(child);
                }
            }
        }
    }

    /// Remove containers that have only one child by promoting the child
    /// to the container's parent. Repeats until no single-child containers remain.
    pub(crate) fn collapse_single_child_containers(&mut self) {
        loop {
            let mut collapsed = false;
            let mut stack = vec![self.root_node];

            while let Some(node) = stack.pop() {
                let children = match self.tree.children(node) {
                    Ok(c) => c,
                    _ => continue,
                };

                for &child in &children {
                    // Only consider non-panel nodes (containers)
                    if self.inner.contains_key(&child) {
                        continue;
                    }

                    let grandchildren = match self.tree.children(child) {
                        Ok(gc) => gc,
                        _ => continue,
                    };

                    if grandchildren.len() == 1 {
                        // Promote the single grandchild to replace this container,
                        // inheriting the container's flex sizing so siblings keep
                        // their proportions.
                        let grandchild = grandchildren[0];
                        let child_idx = children.iter().position(|&c| c == child);

                        if let Some(idx) = child_idx {
                            // Copy container's flex properties to the promoted child
                            if let Ok(container_style) = self.tree.style(child).cloned() {
                                if let Ok(mut gc_style) =
                                    self.tree.style(grandchild).cloned()
                                {
                                    gc_style.flex_basis = container_style.flex_basis;
                                    gc_style.flex_grow = container_style.flex_grow;
                                    gc_style.flex_shrink = container_style.flex_shrink;
                                    let _ = self.tree.set_style(grandchild, gc_style);
                                }
                            }

                            let _ = self.tree.remove_child(child, grandchild);
                            let _ = self.tree.remove_child(node, child);
                            let _ =
                                self.tree.insert_child_at_index(node, idx, grandchild);
                            collapsed = true;
                            break; // Tree changed, restart
                        }
                    } else if grandchildren.is_empty() {
                        // Empty container — remove it
                        let _ = self.tree.remove_child(node, child);
                        collapsed = true;
                        break;
                    } else {
                        stack.push(child);
                    }
                }

                if collapsed {
                    break;
                }
            }

            if !collapsed {
                break;
            }
        }
    }

    pub fn remove_current(&mut self, sugarloaf: &mut Sugarloaf) {
        self.remove_node(self.current, sugarloaf);
    }

    pub fn remove_node(&mut self, to_remove: NodeId, sugarloaf: &mut Sugarloaf) {
        if self.inner.is_empty() {
            tracing::error!("Attempted to remove from empty grid");
            return;
        }

        // Can't remove the last panel
        if self.inner.len() == 1 {
            tracing::warn!("Cannot remove the last remaining context");
            return;
        }

        if !self.inner.contains_key(&to_remove) {
            tracing::error!("Node {:?} not found in grid", to_remove);
            return;
        }

        // Get rich text ID before removing
        let rich_text_id = self.inner.get(&to_remove).map(|item| item.val.rich_text_id);

        let next_current = if to_remove == self.current {
            // Select next panel before removing (use visual ordering)
            let ordered_keys = self.get_ordered_keys();
            let current_pos = ordered_keys.iter().position(|&k| k == to_remove);
            if let Some(pos) = current_pos {
                // Try next panel, or previous if we're at the end
                if pos + 1 < ordered_keys.len() {
                    ordered_keys[pos + 1]
                } else if pos > 0 {
                    ordered_keys[pos - 1]
                } else {
                    // Fallback to any other panel
                    *ordered_keys
                        .iter()
                        .find(|&&k| k != to_remove)
                        .unwrap_or(&to_remove)
                }
            } else {
                // Fallback to first panel
                *self
                    .inner
                    .keys()
                    .find(|&&k| k != to_remove)
                    .unwrap_or(&to_remove)
            }
        } else {
            self.current
        };

        // Remove from Taffy - to_remove IS the NodeId
        let _ = self.tree.remove(to_remove);

        // Remove from inner map
        self.inner.remove(&to_remove);
        self.clear_stack_metadata(to_remove);

        // Cleanup rich text from sugarloaf
        if let Some(id) = rich_text_id {
            sugarloaf.remove_content(id);
        }

        // Update root if necessary
        if Some(to_remove) == self.root {
            self.root = self.inner.keys().next().copied();
        }

        // Set new current
        self.current = if next_current == self.root.unwrap_or(next_current) {
            self.active_stacked.unwrap_or(next_current)
        } else {
            next_current
        };
        if self.is_stacked_node(self.current) {
            if let Some(parent) = self.stacked_parents.get(&self.current).copied() {
                self.set_active_stacked_for_parent(parent, self.current);
            }
        } else {
            self.clear_active_stacked_for_parent(self.current);
        }

        // Collapse single-child containers left behind by removal
        self.collapse_single_child_containers();

        // Re-derive the canonical SessionTree from the now-updated Taffy
        // structure BEFORE laying out — geometry reads the SessionTree, so
        // syncing afterwards (as before) left the remaining pane sized as
        // if the closed one still existed (the "close a split tab but the
        // pane stays" bug).
        self.sync_session_tree();

        // Recompute layout
        if self.panel_count() > 0 {
            // When back to a single panel, reset to flexible so it fills the window
            if self.panel_count() == 1 {
                self.reset_panel_styles_to_flexible();
            }
            self.apply_taffy_layout(sugarloaf);
        }
    }

    /// Extract the context at `route_id` from this grid and return it
    /// **without** tearing down its session — the live PTY / editor keeps
    /// running so the caller can splice it into another grid (e.g. moving
    /// a buffer tab to a different workspace in the same window). Returns
    /// `None` if the route isn't present or it's the grid's last context.
    pub fn take_context_by_route(
        &mut self,
        route_id: usize,
        sugarloaf: &mut Sugarloaf,
    ) -> Option<Context<T>> {
        let node = self
            .inner
            .iter()
            .find_map(|(node, item)| (item.val.route_id == route_id).then_some(*node))?;
        self.take_node(node, sugarloaf)
    }

    /// Non-destroying sibling of [`Self::remove_node`]: detach `to_remove`
    /// from the layout and return its owning [`Context`] with the session
    /// intact. The rich-text object is intentionally left registered with
    /// `sugarloaf` (the caller re-homes it within the same window).
    pub fn take_node(
        &mut self,
        to_remove: NodeId,
        sugarloaf: &mut Sugarloaf,
    ) -> Option<Context<T>> {
        if self.inner.len() <= 1 || !self.inner.contains_key(&to_remove) {
            return None;
        }

        let next_current = if to_remove == self.current {
            let ordered_keys = self.get_ordered_keys();
            match ordered_keys.iter().position(|&k| k == to_remove) {
                Some(pos) if pos + 1 < ordered_keys.len() => ordered_keys[pos + 1],
                Some(pos) if pos > 0 => ordered_keys[pos - 1],
                _ => *ordered_keys
                    .iter()
                    .find(|&&k| k != to_remove)
                    .unwrap_or(&to_remove),
            }
        } else {
            self.current
        };

        let _ = self.tree.remove(to_remove);
        let item = self.inner.remove(&to_remove)?;
        self.clear_stack_metadata(to_remove);

        if Some(to_remove) == self.root {
            self.root = self.inner.keys().next().copied();
        }
        self.current = if next_current == self.root.unwrap_or(next_current) {
            self.active_stacked.unwrap_or(next_current)
        } else {
            next_current
        };
        if self.is_stacked_node(self.current) {
            if let Some(parent) = self.stacked_parents.get(&self.current).copied() {
                self.set_active_stacked_for_parent(parent, self.current);
            }
        } else {
            self.clear_active_stacked_for_parent(self.current);
        }

        self.collapse_single_child_containers();
        // Sync the canonical SessionTree from Taffy BEFORE layout — the
        // geometry solver reads the SessionTree (see remove_node).
        self.sync_session_tree();
        if self.panel_count() > 0 {
            if self.panel_count() == 1 {
                self.reset_panel_styles_to_flexible();
            }
            self.apply_taffy_layout(sugarloaf);
        }
        Some(item.val)
    }

    pub fn split_right(&mut self, context: Context<T>, sugarloaf: &mut Sugarloaf) {
        if !self.inner.contains_key(&self.current) {
            return;
        }

        // Create taffy node first, then item
        if let Ok(new_node) = self.try_split_right() {
            let new_context = ContextGridItem::new(context);
            self.inner.insert(new_node, new_context);
            // Focus the new pane BEFORE laying out so visibility/clip is
            // computed against the final state (matches the resize path).
            self.current = new_node;
            self.apply_taffy_layout(sugarloaf);
        }
    }

    pub fn split_existing_right(
        &mut self,
        node: NodeId,
        sugarloaf: &mut Sugarloaf,
    ) -> bool {
        match self.split_panel_with_existing_node(node, taffy::FlexDirection::Row) {
            Ok(new_current) => {
                self.apply_taffy_layout(sugarloaf);
                self.current = new_current;
                true
            }
            Err(error) => {
                tracing::warn!(?node, ?error, "could not split existing context right");
                false
            }
        }
    }

    /// Add a context to the same visual slot as the root panel. This is
    /// used for workspace buffer tabs: the terminal and the embedded
    /// editor are peers in the tab strip, not split panes, so only the
    /// selected one is visible and it occupies the full workspace body.
    pub fn add_stacked_context(
        &mut self,
        context: Context<T>,
        sugarloaf: &mut Sugarloaf,
    ) -> Option<NodeId> {
        let parent = self.root?;
        self.add_stacked_context_on_parent(context, parent, sugarloaf)
    }

    /// SessionTree-first add_stacked. Wraps the parent's leaf in a
    /// [`SessionTreeNode::Tabbed`] group (or appends to one if already
    /// present) and stores the new ContextGridItem at the freshly
    /// allocated NodeId returned by the rebuild.
    pub fn add_stacked_context_on_parent(
        &mut self,
        context: Context<T>,
        parent: NodeId,
        sugarloaf: &mut Sugarloaf,
    ) -> Option<NodeId> {
        if !self.inner.contains_key(&parent) {
            return None;
        }
        let parent_leaf = *self.node_to_leaf.get(&parent)?;
        let new_item = ContextGridItem::new(context);
        let new_spec = session_leaf_spec_for_grid_item(&new_item);
        let prev_node_to_leaf = self.node_to_leaf.clone();

        // Wrap the parent leaf in a Tabbed group (or append if already
        // tabbed), returning the freshly-allocated tab id.
        let new_leaf_id = self
            .session_tree
            .wrap_leaf_in_tabbed(parent_leaf, new_spec.clone())
            .ok()?;

        let available_width =
            self.width - self.scaled_margin.left - self.scaled_margin.right;
        let available_height =
            self.height - self.scaled_margin.top - self.scaled_margin.bottom;
        let rebuilt = rebuild_taffy_from_tree(
            &self.session_tree,
            &self.panel_config,
            self.scale,
            available_width,
            available_height,
        );
        let mut root_style = match rebuilt.tree.style(rebuilt.root) {
            Ok(s) => s.clone(),
            Err(_) => return None,
        };
        root_style.size = geometry::Size {
            width: length(available_width),
            height: length(available_height),
        };
        let new_node = *rebuilt.leaf_to_node.get(&new_leaf_id)?;

        let mut additional = FxHashMap::default();
        additional.insert(new_leaf_id, new_item);
        self.splice_rebuild(rebuilt, &prev_node_to_leaf, new_leaf_id, additional);
        let _ = self.tree.set_style(self.root_node, root_style);

        // Make the new tab the active one in its group.
        self.current = new_node;
        if self.is_stacked_node(new_node) {
            if let Some(p) = self.stacked_parents.get(&new_node).copied() {
                self.set_active_stacked_for_parent(p, new_node);
            }
        }

        self.apply_taffy_layout(sugarloaf);
        Some(new_node)
    }

    /// Split down - create new panel below using Taffy
    pub fn split_down(&mut self, context: Context<T>, sugarloaf: &mut Sugarloaf) {
        if !self.inner.contains_key(&self.current) {
            return;
        }

        // Create taffy node first, then item
        if let Ok(new_node) = self.try_split_down() {
            let new_context = ContextGridItem::new(context);
            self.inner.insert(new_node, new_context);
            // Focus the new pane BEFORE laying out so visibility/clip is
            // computed against the final state (matches the resize path).
            self.current = new_node;
            self.apply_taffy_layout(sugarloaf);
        }
    }

    pub fn split_existing_down(
        &mut self,
        node: NodeId,
        sugarloaf: &mut Sugarloaf,
    ) -> bool {
        match self.split_panel_with_existing_node(node, taffy::FlexDirection::Column) {
            Ok(new_current) => {
                self.apply_taffy_layout(sugarloaf);
                self.current = new_current;
                true
            }
            Err(error) => {
                tracing::warn!(?node, ?error, "could not split existing context down");
                false
            }
        }
    }

    pub fn stack_existing_on_root(
        &mut self,
        node: NodeId,
        sugarloaf: &mut Sugarloaf,
    ) -> bool {
        let Some(parent) = self.root else {
            return false;
        };
        self.stack_existing_on_parent(node, parent, sugarloaf)
    }

    /// SessionTree-first stack_existing. Moves the leaf at `node` into
    /// a [`SessionTreeNode::Tabbed`] group anchored on `parent` (creating
    /// the Tabbed wrapper if it does not exist yet). Preserves the
    /// moving leaf's id so adapters do not lose their route mappings.
    pub fn stack_existing_on_parent(
        &mut self,
        node: NodeId,
        parent: NodeId,
        sugarloaf: &mut Sugarloaf,
    ) -> bool {
        if node == parent
            || !self.inner.contains_key(&node)
            || !self.inner.contains_key(&parent)
        {
            return false;
        }

        let Some(&parent_leaf) = self.node_to_leaf.get(&parent) else {
            return false;
        };
        let Some(&moving_leaf) = self.node_to_leaf.get(&node) else {
            return false;
        };
        if parent_leaf == moving_leaf {
            return false;
        }

        let prev_node_to_leaf = self.node_to_leaf.clone();

        // Detach the moving leaf from wherever it lives (split or
        // existing tabbed) — keeping its full leaf data — then append
        // it as a tab sibling of the parent (which creates the Tabbed
        // wrapper if needed). This preserves the moving leaf's id so
        // adapters' route mappings round-trip.
        let detached = match self.session_tree.detach_leaf(moving_leaf) {
            Ok(leaf) => leaf,
            Err(err) => {
                tracing::warn!(?err, ?node, "could not detach leaf for stack_existing");
                return false;
            }
        };
        let moved_leaf_id = match self
            .session_tree
            .insert_leaf_as_tab_sibling(parent_leaf, detached)
        {
            Ok(id) => id,
            Err(err) => {
                tracing::warn!(
                    ?err,
                    ?node,
                    "could not anchor leaf onto parent tabbed group"
                );
                return false;
            }
        };

        let available_width =
            self.width - self.scaled_margin.left - self.scaled_margin.right;
        let available_height =
            self.height - self.scaled_margin.top - self.scaled_margin.bottom;
        let rebuilt = rebuild_taffy_from_tree(
            &self.session_tree,
            &self.panel_config,
            self.scale,
            available_width,
            available_height,
        );
        let mut root_style = match rebuilt.tree.style(rebuilt.root) {
            Ok(s) => s.clone(),
            Err(_) => return false,
        };
        root_style.size = geometry::Size {
            width: length(available_width),
            height: length(available_height),
        };
        let new_node = match rebuilt.leaf_to_node.get(&moved_leaf_id).copied() {
            Some(n) => n,
            None => return false,
        };

        let _ = self.session_tree.focus_leaf(moved_leaf_id);
        self.splice_rebuild(
            rebuilt,
            &prev_node_to_leaf,
            moved_leaf_id,
            FxHashMap::default(),
        );
        let _ = self.tree.set_style(self.root_node, root_style);

        self.current = new_node;
        if self.is_stacked_node(new_node) {
            if let Some(p) = self.stacked_parents.get(&new_node).copied() {
                self.set_active_stacked_for_parent(p, new_node);
            }
        }

        if self.panel_count() == 1 {
            self.reset_panel_styles_to_flexible();
        }
        self.apply_taffy_layout(sugarloaf)
    }
}
