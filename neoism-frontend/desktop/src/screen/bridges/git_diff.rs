// Auto-split from screen/mod.rs. See sibling mod.rs for the Screen struct and
// the constructor/core methods. This file is part of the impl Screen<'_> block.


use super::super::*;

impl Screen<'_> {
    pub fn is_hovering_git_diff_panel_resize_edge(&self) -> bool {
        let (mouse_x, mouse_y) = self.mouse_logical_for_hit_test();
        self.renderer
            .git_diff_panel
            .is_hovering_resize_edge(mouse_x, mouse_y)
    }

    pub fn begin_git_diff_panel_resize(&mut self) -> bool {
        if !self.is_hovering_git_diff_panel_resize_edge() {
            return false;
        }
        let (mouse_x, _) = self.mouse_logical_for_hit_test();
        self.git_diff_panel_resize_state = Some(GitDiffPanelResizeState {
            start_x: mouse_x,
            original_width: self.renderer.git_diff_panel.width(),
        });
        true
    }

    pub fn git_diff_panel_resize_active(&self) -> bool {
        self.git_diff_panel_resize_state.is_some()
    }

    pub fn drag_git_diff_panel_resize(&mut self) -> bool {
        let Some(state) = self.git_diff_panel_resize_state else {
            return false;
        };
        let (mouse_x, _) = self.mouse_logical_for_hit_test();
        // Right-side panel: dragging mouse left grows the panel.
        let target_width = state.original_width - (mouse_x - state.start_x);
        let current_width = self.renderer.git_diff_panel.width();
        self.renderer
            .git_diff_panel
            .resize(target_width - current_width);
        self.reapply_chrome_layout();
        self.mark_dirty();
        true
    }

    pub fn end_git_diff_panel_resize(&mut self) -> bool {
        let was_active = self.git_diff_panel_resize_state.take().is_some();
        if was_active {
            self.mark_dirty();
        }
        was_active
    }

    pub fn begin_git_diff_panel_scrollbar_drag(&mut self) -> bool {
        if !self.renderer.git_diff_panel.is_visible() {
            return false;
        }
        let (mouse_x, mouse_y) = self.mouse_logical_for_hit_test();
        let Some(kind) = self.renderer.git_diff_panel.scrollbar_hit(mouse_x, mouse_y)
        else {
            return false;
        };
        self.git_diff_panel_scrollbar_drag =
            Some(GitDiffPanelScrollbarDragState { kind });
        // Snap immediately on press so the thumb tracks the cursor's
        // initial position even if the user clicks slightly off the
        // thumb itself.
        self.renderer.git_diff_panel.drag_scrollbar(kind, mouse_y);
        self.mark_dirty();
        true
    }

    pub fn git_diff_panel_scrollbar_drag_active(&self) -> bool {
        self.git_diff_panel_scrollbar_drag.is_some()
    }

    pub fn drag_git_diff_panel_scrollbar(&mut self) -> bool {
        let Some(state) = self.git_diff_panel_scrollbar_drag else {
            return false;
        };
        let (_, mouse_y) = self.mouse_logical_for_hit_test();
        self.renderer
            .git_diff_panel
            .drag_scrollbar(state.kind, mouse_y);
        self.mark_dirty();
        true
    }

    pub fn end_git_diff_panel_scrollbar_drag(&mut self) -> bool {
        let was_active = self.git_diff_panel_scrollbar_drag.take().is_some();
        if was_active {
            self.mark_dirty();
        }
        was_active
    }

    pub(crate) fn handle_git_diff_panel_key(
        &mut self,
        key: &neoism_window::event::KeyEvent,
    ) -> bool {
        use neoism_window::keyboard::{Key, NamedKey};
        let mods = self.modifiers.state();
        let plain = !mods.alt_key() && !mods.control_key() && !mods.super_key();
        let alt_only = mods.alt_key() && !mods.control_key() && !mods.super_key();

        match &key.logical_key {
            Key::Named(NamedKey::ArrowDown) if alt_only => {
                self.renderer.git_diff_panel.scroll_diff_rows(1);
                true
            }
            Key::Named(NamedKey::ArrowUp) if alt_only => {
                self.renderer.git_diff_panel.scroll_diff_rows(-1);
                true
            }
            Key::Named(NamedKey::ArrowDown) if plain => {
                self.renderer.git_diff_panel.select_next();
                true
            }
            Key::Named(NamedKey::ArrowUp) if plain => {
                self.renderer.git_diff_panel.select_prev();
                true
            }
            Key::Character(s) if plain && s.as_str() == "j" => {
                self.renderer.git_diff_panel.select_next();
                true
            }
            Key::Character(s) if plain && s.as_str() == "k" => {
                self.renderer.git_diff_panel.select_prev();
                true
            }
            Key::Named(NamedKey::Enter) => {
                // Enter on the focused panel jumps to the selected
                // file in the editor — same activation gesture the
                // file_tree uses on Enter.
                if let Some((path, _root)) =
                    self.renderer.git_diff_panel.selected_file_target()
                {
                    self.renderer.git_diff_panel.set_focused(false);
                    if crate::editor::markdown::state::is_markdown_path(&path) {
                        self.open_path_in_markdown(path);
                    } else {
                        self.open_path_in_editor(path);
                    }
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.close_git_diff_panel();
                true
            }
            // Plain typed characters belong to the focused panel — swallow
            // them so they don't leak into the terminal/editor behind it.
            // Mod-key combos (Ctrl/Alt/Super) fall through to global
            // shortcuts and chrome focus navigation.
            Key::Character(_) if plain => true,
            _ => false,
        }
    }

    pub fn toggle_git_diff_panel(&mut self) {
        let _ = self.sync_workspace_root_from_active_pane();
        let target_route = self.finder_target_route_for_current_focus();
        let cwd = self.finder_cwd(target_route);
        let repo_root = neoism_ui::panels::git_branch::repo_root_for(&cwd);
        let branch = neoism_ui::panels::git_branch::branch_for(&cwd);
        self.renderer.file_tree.set_focused(false);
        self.renderer.git_diff_panel.toggle(repo_root, branch);
        self.reapply_chrome_layout();
        self.mark_dirty();
    }

    pub fn close_git_diff_panel(&mut self) -> bool {
        if !self.renderer.git_diff_panel.is_visible() {
            return false;
        }
        self.renderer.git_diff_panel.close();
        self.reapply_chrome_layout();
        self.mark_dirty();
        true
    }

    pub fn handle_git_diff_panel_click(&mut self) -> bool {
        if !self.renderer.git_diff_panel.is_visible() {
            return false;
        }
        let (mouse_x, mouse_y) = self.mouse_logical_for_hit_test();
        match self.renderer.git_diff_panel.hit_test(mouse_x, mouse_y) {
            crate::editor::git_diff_panel::PanelHit::Outside => {
                if self.renderer.git_diff_panel.is_focused() {
                    self.renderer.git_diff_panel.set_focused(false);
                    self.mark_dirty();
                }
                false
            }
            crate::editor::git_diff_panel::PanelHit::Close => {
                self.renderer.git_diff_panel.close();
                self.reapply_chrome_layout();
                self.mark_dirty();
                true
            }
            crate::editor::git_diff_panel::PanelHit::FileRow(idx) => {
                self.renderer.git_diff_panel.set_focused(true);
                self.renderer.file_tree.set_focused(false);
                self.renderer.git_diff_panel.select_file(idx);
                self.mark_dirty();
                true
            }
            crate::editor::git_diff_panel::PanelHit::Inside => {
                self.renderer.git_diff_panel.set_focused(true);
                self.renderer.file_tree.set_focused(false);
                self.mark_dirty();
                true
            }
        }
    }
}
