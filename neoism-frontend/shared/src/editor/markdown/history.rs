use super::helpers::*;
use super::types::*;

impl MarkdownPane {
    /// Wave 7D: while the pane is bound to a CRDT document, snapshot
    /// undo is unsafe — a pre-remote-edit snapshot would resurrect or
    /// destroy a collaborator's text and `flush_local` would broadcast
    /// that destruction as OUR edit. The host flips this on once the
    /// binding is seeded; `undo`/`redo` then queue requests the host
    /// routes through the binding's origin-scoped Yrs undo manager.
    pub fn set_doc_history_bound(&mut self, bound: bool) {
        self.doc_history_bound = bound;
        if !bound {
            self.pending_doc_history.clear();
        }
    }

    pub fn doc_history_bound(&self) -> bool {
        self.doc_history_bound
    }

    /// Drain the undo/redo intents queued while doc-bound (in press
    /// order). Called by the host's per-pump CRDT choke point.
    pub fn take_doc_history_requests(&mut self) -> Vec<MarkdownDocHistoryRequest> {
        std::mem::take(&mut self.pending_doc_history)
    }

    pub fn undo(&mut self) -> bool {
        if self.doc_history_bound {
            self.pending_doc_history
                .push(MarkdownDocHistoryRequest::Undo);
            return true;
        }
        let Some(mut entry) = self.undo_stack.pop() else {
            return false;
        };
        match &mut entry {
            MarkdownHistoryEntry::Full { before, after } => {
                if after.is_none() {
                    *after = Some(self.history_snapshot());
                }
                self.restore_history_snapshot(before.clone());
            }
            MarkdownHistoryEntry::Lines { before, after } => {
                if after.is_none() {
                    *after = Some(self.history_line_snapshot(
                        before.start,
                        before.start.saturating_add(before.lines.len()),
                    ));
                }
                let replace = after
                    .as_ref()
                    .map(|snapshot| snapshot.lines.len())
                    .unwrap_or(0);
                self.restore_history_line_snapshot(before.clone(), before.start, replace);
            }
        }
        self.redo_stack.push(entry);
        self.follow_cursor = true;
        // No monotonic dirty flag: `is_dirty()` recomputes against the
        // saved baseline, so undoing back to the on-disk text clears the
        // tab dot (and a later redo into divergent text re-sets it).
        self.rebuild_blocks();
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.doc_history_bound {
            self.pending_doc_history
                .push(MarkdownDocHistoryRequest::Redo);
            return true;
        }
        let Some(entry) = self.redo_stack.pop() else {
            return false;
        };
        match &entry {
            MarkdownHistoryEntry::Full { after, .. } => {
                let Some(after) = after.clone() else {
                    return false;
                };
                self.restore_history_snapshot(after);
            }
            MarkdownHistoryEntry::Lines { before, after } => {
                let Some(after) = after.clone() else {
                    return false;
                };
                self.restore_history_line_snapshot(
                    after,
                    before.start,
                    before.lines.len(),
                );
            }
        }
        self.undo_stack.push(entry);
        if self.undo_stack.len() > 128 {
            self.undo_stack.remove(0);
        }
        self.follow_cursor = true;
        // Dirty is derived from `lines != saved_baseline`; no flag to set.
        self.rebuild_blocks();
        true
    }

    pub(crate) fn save_undo(&mut self) {
        self.undo_stack.push(MarkdownHistoryEntry::Full {
            before: self.history_snapshot(),
            after: None,
        });
        self.trim_undo_stack();
        self.redo_stack.clear();
        self.vim.clear_pending();
    }

    pub(crate) fn commit_undo(&mut self) {
        let snapshot = self.history_snapshot();
        if let Some(MarkdownHistoryEntry::Full { after, .. }) = self.undo_stack.last_mut()
        {
            if after.is_none() {
                *after = Some(snapshot);
            }
        }
    }

    pub(crate) fn save_local_undo(&mut self, start: usize, end: usize) -> bool {
        if !self.should_use_local_history() {
            self.save_undo();
            return false;
        }
        self.undo_stack.push(MarkdownHistoryEntry::Lines {
            before: self.history_line_snapshot(start, end),
            after: None,
        });
        self.trim_undo_stack();
        self.redo_stack.clear();
        self.vim.clear_pending();
        true
    }

    pub(crate) fn commit_local_undo(&mut self, local: bool, start: usize, end: usize) {
        if !local {
            self.commit_undo();
            return;
        }
        let snapshot = self.history_line_snapshot(start, end);
        if let Some(MarkdownHistoryEntry::Lines { after, .. }) =
            self.undo_stack.last_mut()
        {
            if after.is_none() {
                *after = Some(snapshot);
            }
        }
    }

    pub(super) fn history_snapshot(&self) -> MarkdownHistorySnapshot {
        MarkdownHistorySnapshot {
            lines: self.lines.clone(),
            cursor_line: self.cursor_line,
            cursor_col: self.cursor_col,
            enter_continuation_lines: self.enter_continuation_lines.clone(),
        }
    }

    pub(super) fn history_line_snapshot(
        &self,
        start: usize,
        end: usize,
    ) -> MarkdownHistoryLineSnapshot {
        let start = start.min(self.lines.len());
        let end = end.min(self.lines.len()).max(start);
        MarkdownHistoryLineSnapshot {
            start,
            lines: self.lines[start..end].to_vec(),
            cursor_line: self.cursor_line,
            cursor_col: self.cursor_col,
            enter_continuation_lines: self.enter_continuation_lines.clone(),
        }
    }

    pub(super) fn restore_history_snapshot(&mut self, snapshot: MarkdownHistorySnapshot) {
        let enter_continuation_lines = snapshot.enter_continuation_lines;
        self.lines = snapshot.lines;
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.reset_source_len_from_lines();
        self.cursor_line = snapshot.cursor_line.min(self.lines.len() - 1);
        self.cursor_col = snapshot.cursor_col.min(self.lines[self.cursor_line].len());
        self.enter_continuation_lines = enter_continuation_lines;
        self.enter_continuation_lines
            .retain(|line| *line < self.lines.len() && self.lines[*line].is_empty());
        self.clamp_cursor();
        self.pending_line_edit = None;
        self.vim.clear_pending();
        self.visual_anchor = None;
        self.mouse_select_anchor = None;
    }

    pub(super) fn restore_history_line_snapshot(
        &mut self,
        snapshot: MarkdownHistoryLineSnapshot,
        replace_start: usize,
        replace_len: usize,
    ) {
        let enter_continuation_lines = snapshot.enter_continuation_lines;
        let start = replace_start.min(self.lines.len());
        let end = start.saturating_add(replace_len).min(self.lines.len());
        self.lines.splice(start..end, snapshot.lines);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.reset_source_len_from_lines();
        self.cursor_line = snapshot.cursor_line.min(self.lines.len() - 1);
        self.cursor_col = snapshot.cursor_col.min(self.lines[self.cursor_line].len());
        self.enter_continuation_lines = enter_continuation_lines;
        self.enter_continuation_lines
            .retain(|line| *line < self.lines.len() && self.lines[*line].is_empty());
        self.clamp_cursor();
        self.pending_line_edit = None;
        self.vim.clear_pending();
        self.visual_anchor = None;
        self.mouse_select_anchor = None;
    }

    fn trim_undo_stack(&mut self) {
        if self.undo_stack.len() > 128 {
            self.undo_stack.remove(0);
        }
    }

    pub(crate) fn shift_enter_continuations_for_insert(&mut self, line: usize) {
        self.enter_continuation_lines = self
            .enter_continuation_lines
            .iter()
            .map(|existing| {
                if *existing >= line {
                    *existing + 1
                } else {
                    *existing
                }
            })
            .collect();
    }

    pub(crate) fn shift_enter_continuations_for_remove(&mut self, line: usize) {
        self.enter_continuation_lines = self
            .enter_continuation_lines
            .iter()
            .filter_map(|existing| {
                if *existing == line {
                    None
                } else if *existing > line {
                    Some(*existing - 1)
                } else {
                    Some(*existing)
                }
            })
            .collect();
    }

    pub(crate) fn rebuild_blocks(&mut self) {
        if self.should_defer_block_parse() {
            self.blocks.clear();
            self.source_revision = self.source_revision.saturating_add(1);
            return;
        }
        self.link_target_cache.borrow_mut().clear();
        self.blocks = parse_blocks(&source_from_lines(&self.lines));
        self.source_revision = self.source_revision.saturating_add(1);
    }
}
