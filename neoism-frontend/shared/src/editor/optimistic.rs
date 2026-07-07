//! Pure optimistic editor state shared by desktop and web frontends.
//!
//! Hosts still own transport, rendering, and daemon RPCs. This module only
//! tracks local prediction: text edits are visible immediately, cursor blink
//! and drag selection are local, and pending mutations stay queued until the
//! server confirms or rejects them.

use std::collections::VecDeque;

pub type MutationId = u64;
pub type Revision = u64;
pub type TimestampMillis = u64;

pub const DEFAULT_TIMEOUT_MS: TimestampMillis = 1_000;
pub const DEFAULT_BLINK_PERIOD_MS: TimestampMillis = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerSnapshot {
    pub revision: Revision,
    pub text: String,
}

impl ServerSnapshot {
    pub fn new(revision: Revision, text: impl Into<String>) -> Self {
        Self {
            revision,
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextAck {
    pub mutation_id: MutationId,
    pub snapshot: ServerSnapshot,
}

impl TextAck {
    pub fn new(
        mutation_id: MutationId,
        revision: Revision,
        text: impl Into<String>,
    ) -> Self {
        Self {
            mutation_id,
            snapshot: ServerSnapshot::new(revision, text),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextMutation {
    Append { text: String },
    Insert { byte_index: usize, text: String },
    Delete { start: usize, end: usize },
}

impl TextMutation {
    pub fn append(text: impl Into<String>) -> Self {
        Self::Append { text: text.into() }
    }

    pub fn insert(byte_index: usize, text: impl Into<String>) -> Self {
        Self::Insert {
            byte_index,
            text: text.into(),
        }
    }

    pub fn delete(start: usize, end: usize) -> Self {
        Self::Delete { start, end }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionMutation {
    Set { anchor: usize, focus: usize },
    Clear,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptimisticMutation {
    Text(TextMutation),
    Selection(SelectionMutation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingStatus {
    InFlight,
    Uncommitted { retry_count: u32 },
    Conflicted,
}

impl PendingStatus {
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::Uncommitted { .. } | Self::Conflicted)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMutation {
    pub id: MutationId,
    pub base_revision: Revision,
    pub created_at_ms: TimestampMillis,
    pub last_sent_at_ms: TimestampMillis,
    pub status: PendingStatus,
    pub mutation: OptimisticMutation,
}

impl PendingMutation {
    pub fn should_timeout(
        &self,
        now_ms: TimestampMillis,
        timeout_ms: TimestampMillis,
    ) -> bool {
        matches!(self.status, PendingStatus::InFlight)
            && now_ms.saturating_sub(self.last_sent_at_ms) > timeout_ms
    }

    pub fn mark_sent(&mut self, now_ms: TimestampMillis) {
        self.last_sent_at_ms = now_ms;
        self.status = PendingStatus::InFlight;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    pub fn new(start: usize, end: usize) -> Self {
        if start <= end {
            Self { start, end }
        } else {
            Self {
                start: end,
                end: start,
            }
        }
    }

    pub const fn collapsed(byte_index: usize) -> Self {
        Self {
            start: byte_index,
            end: byte_index,
        }
    }

    pub const fn is_collapsed(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorPrediction {
    pub byte_index: usize,
    pub blink_started_at_ms: TimestampMillis,
    pub blink_period_ms: TimestampMillis,
    pub forced_visible_until_ms: TimestampMillis,
}

impl CursorPrediction {
    pub fn new(byte_index: usize, now_ms: TimestampMillis) -> Self {
        Self {
            byte_index,
            blink_started_at_ms: now_ms,
            blink_period_ms: DEFAULT_BLINK_PERIOD_MS,
            forced_visible_until_ms: now_ms,
        }
    }

    pub fn force_visible(&mut self, now_ms: TimestampMillis) {
        self.blink_started_at_ms = now_ms;
        self.forced_visible_until_ms = now_ms.saturating_add(self.blink_period_ms);
    }

    pub fn visible_at(&self, now_ms: TimestampMillis) -> bool {
        if now_ms <= self.forced_visible_until_ms {
            return true;
        }

        let elapsed = now_ms.saturating_sub(self.blink_started_at_ms);
        let half_period = self.blink_period_ms.max(2) / 2;
        (elapsed / half_period) % 2 == 0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionPrediction {
    pub committed: Option<TextRange>,
    pub drag_anchor: Option<usize>,
    pub drag_focus: Option<usize>,
    pub hover: Option<TextRange>,
}

impl SelectionPrediction {
    pub fn visible_range(&self) -> Option<TextRange> {
        match (self.drag_anchor, self.drag_focus) {
            (Some(anchor), Some(focus)) => Some(TextRange::new(anchor, focus)),
            _ => self.committed,
        }
    }

    pub fn begin_drag(&mut self, anchor: usize) {
        self.drag_anchor = Some(anchor);
        self.drag_focus = Some(anchor);
    }

    pub fn update_drag(&mut self, focus: usize) {
        if self.drag_anchor.is_some() {
            self.drag_focus = Some(focus);
        }
    }

    pub fn clear_drag(&mut self) {
        self.drag_anchor = None;
        self.drag_focus = None;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptimisticDocument {
    canonical: String,
    revision: Revision,
    next_mutation_id: MutationId,
    pending: VecDeque<PendingMutation>,
    timeout_ms: TimestampMillis,
    pub cursor: CursorPrediction,
    pub selection: SelectionPrediction,
}

impl OptimisticDocument {
    pub fn new(
        revision: Revision,
        canonical: impl Into<String>,
        now_ms: TimestampMillis,
    ) -> Self {
        let canonical = canonical.into();
        let cursor = CursorPrediction::new(canonical.len(), now_ms);
        Self {
            canonical,
            revision,
            next_mutation_id: 1,
            pending: VecDeque::new(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            cursor,
            selection: SelectionPrediction::default(),
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: TimestampMillis) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn canonical_text(&self) -> &str {
        &self.canonical
    }

    pub fn pending(&self) -> &VecDeque<PendingMutation> {
        &self.pending
    }

    pub fn pending_mut(&mut self) -> &mut VecDeque<PendingMutation> {
        &mut self.pending
    }

    pub fn view_text(&self) -> String {
        apply_pending_text(&self.canonical, self.pending.iter())
    }

    pub fn append_text(
        &mut self,
        text: impl Into<String>,
        now_ms: TimestampMillis,
    ) -> MutationId {
        self.push_text_mutation(TextMutation::append(text), now_ms)
    }

    pub fn insert_text(
        &mut self,
        byte_index: usize,
        text: impl Into<String>,
        now_ms: TimestampMillis,
    ) -> MutationId {
        self.push_text_mutation(TextMutation::insert(byte_index, text), now_ms)
    }

    pub fn delete_range(
        &mut self,
        start: usize,
        end: usize,
        now_ms: TimestampMillis,
    ) -> MutationId {
        self.push_text_mutation(TextMutation::delete(start, end), now_ms)
    }

    pub fn begin_selection_drag(&mut self, anchor: usize) {
        self.selection.begin_drag(anchor);
    }

    pub fn update_selection_drag(&mut self, focus: usize) {
        self.selection.update_drag(focus);
    }

    pub fn release_selection_drag(
        &mut self,
        now_ms: TimestampMillis,
    ) -> Option<MutationId> {
        let range = self.selection.visible_range()?;
        self.selection.committed = if range.is_collapsed() {
            None
        } else {
            Some(range)
        };
        self.selection.clear_drag();

        Some(self.push_mutation(
            OptimisticMutation::Selection(match self.selection.committed {
                Some(range) => SelectionMutation::Set {
                    anchor: range.start,
                    focus: range.end,
                },
                None => SelectionMutation::Clear,
            }),
            now_ms,
        ))
    }

    pub fn set_hover_range(&mut self, hover: Option<TextRange>) {
        self.selection.hover = hover;
    }

    pub fn acknowledge_text(&mut self, ack: TextAck) -> bool {
        let Some(position) = self.pending.iter().position(|pending| {
            pending.id == ack.mutation_id
                && matches!(pending.mutation, OptimisticMutation::Text(_))
        }) else {
            return false;
        };

        self.canonical = ack.snapshot.text;
        self.revision = ack.snapshot.revision;
        self.pending.drain(..=position);
        self.clamp_local_ranges_to_view();
        true
    }

    pub fn acknowledge_selection(
        &mut self,
        mutation_id: MutationId,
        revision: Revision,
    ) -> bool {
        let Some(position) = self.pending.iter().position(|pending| {
            pending.id == mutation_id
                && matches!(pending.mutation, OptimisticMutation::Selection(_))
        }) else {
            return false;
        };

        self.revision = self.revision.max(revision);
        self.pending.drain(..=position);
        true
    }

    pub fn apply_server_snapshot(&mut self, snapshot: ServerSnapshot) {
        if snapshot.revision < self.revision {
            return;
        }

        self.canonical = snapshot.text;
        self.revision = snapshot.revision;
        for pending in &mut self.pending {
            if pending.base_revision < self.revision {
                pending.status = PendingStatus::Conflicted;
            }
        }
        self.clamp_local_ranges_to_view();
    }

    pub fn mark_timeouts(&mut self, now_ms: TimestampMillis) -> Vec<MutationId> {
        let mut timed_out = Vec::new();
        for pending in &mut self.pending {
            if pending.should_timeout(now_ms, self.timeout_ms) {
                let retry_count = match pending.status {
                    PendingStatus::Uncommitted { retry_count } => retry_count + 1,
                    _ => 1,
                };
                pending.status = PendingStatus::Uncommitted { retry_count };
                timed_out.push(pending.id);
            }
        }
        timed_out
    }

    pub fn mark_sent(
        &mut self,
        mutation_id: MutationId,
        now_ms: TimestampMillis,
    ) -> bool {
        let Some(pending) = self
            .pending
            .iter_mut()
            .find(|pending| pending.id == mutation_id)
        else {
            return false;
        };

        pending.mark_sent(now_ms);
        true
    }

    fn push_text_mutation(
        &mut self,
        mutation: TextMutation,
        now_ms: TimestampMillis,
    ) -> MutationId {
        let current_text_len = self.view_text().len();
        let mutation_id =
            self.push_mutation(OptimisticMutation::Text(mutation.clone()), now_ms);
        self.cursor.byte_index = apply_cursor_text_mutation(
            self.cursor.byte_index,
            current_text_len,
            &mutation,
        );
        self.cursor.force_visible(now_ms);
        self.clamp_local_ranges_to_view();
        mutation_id
    }

    fn push_mutation(
        &mut self,
        mutation: OptimisticMutation,
        now_ms: TimestampMillis,
    ) -> MutationId {
        let id = self.next_mutation_id;
        self.next_mutation_id = self.next_mutation_id.saturating_add(1);
        self.pending.push_back(PendingMutation {
            id,
            base_revision: self.revision,
            created_at_ms: now_ms,
            last_sent_at_ms: now_ms,
            status: PendingStatus::InFlight,
            mutation,
        });
        id
    }

    fn clamp_local_ranges_to_view(&mut self) {
        let view_text = self.view_text();
        let len = view_text.len();
        self.cursor.byte_index =
            clamp_to_char_boundary(&view_text, self.cursor.byte_index);
        if self.cursor.byte_index > len {
            self.cursor.byte_index = len;
        }
        self.selection.committed = self.selection.committed.map(|range| range.clamp(len));
        self.selection.hover = self.selection.hover.map(|range| range.clamp(len));
        if let Some(anchor) = self.selection.drag_anchor {
            self.selection.drag_anchor = Some(anchor.min(len));
        }
        if let Some(focus) = self.selection.drag_focus {
            self.selection.drag_focus = Some(focus.min(len));
        }
    }
}

impl TextRange {
    fn clamp(self, len: usize) -> Self {
        Self::new(self.start.min(len), self.end.min(len))
    }
}

pub fn apply_pending_text<'a>(
    canonical: &str,
    pending: impl IntoIterator<Item = &'a PendingMutation>,
) -> String {
    let mut text = canonical.to_owned();
    for pending in pending {
        if let OptimisticMutation::Text(mutation) = &pending.mutation {
            apply_text_mutation(&mut text, mutation);
        }
    }
    text
}

pub fn apply_text_mutation(text: &mut String, mutation: &TextMutation) {
    match mutation {
        TextMutation::Append { text: appended } => text.push_str(appended),
        TextMutation::Insert {
            byte_index,
            text: inserted,
        } => {
            let index = clamp_to_char_boundary(text, *byte_index);
            text.insert_str(index, inserted);
        }
        TextMutation::Delete { start, end } => {
            let start = clamp_to_char_boundary(text, *start);
            let end = clamp_to_char_boundary(text, *end);
            let range = TextRange::new(start, end);
            text.replace_range(range.start..range.end, "");
        }
    }
}

pub fn apply_cursor_text_mutation(
    cursor: usize,
    current_text_len: usize,
    mutation: &TextMutation,
) -> usize {
    match mutation {
        TextMutation::Append { text } => current_text_len.saturating_add(text.len()),
        TextMutation::Insert { byte_index, text } => {
            if *byte_index <= cursor {
                cursor.saturating_add(text.len())
            } else {
                cursor
            }
        }
        TextMutation::Delete { start, end } => {
            let range = TextRange::new(*start, *end);
            if cursor <= range.start {
                cursor
            } else if cursor >= range.end {
                cursor.saturating_sub(range.end.saturating_sub(range.start))
            } else {
                range.start
            }
        }
    }
}

pub fn clamp_to_char_boundary(text: &str, byte_index: usize) -> usize {
    let mut index = byte_index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimistic_append_echoes_locally_until_ack() {
        let mut document = OptimisticDocument::new(7, "hello", 0);

        let mutation_id = document.append_text(" world", 10);

        assert_eq!(mutation_id, 1);
        assert_eq!(document.canonical_text(), "hello");
        assert_eq!(document.view_text(), "hello world");
        assert_eq!(document.pending().len(), 1);

        assert!(document.acknowledge_text(TextAck::new(1, 8, "hello world")));
        assert_eq!(document.canonical_text(), "hello world");
        assert_eq!(document.view_text(), "hello world");
        assert!(document.pending().is_empty());
    }

    #[test]
    fn optimistic_delete_echoes_and_ack_accepts_canonical_state() {
        let mut document = OptimisticDocument::new(1, "abcdef", 0);

        let mutation_id = document.delete_range(2, 4, 1);

        assert_eq!(mutation_id, 1);
        assert_eq!(document.view_text(), "abef");
        assert!(document.acknowledge_text(TextAck::new(1, 2, "abef")));
        assert_eq!(document.canonical_text(), "abef");
        assert_eq!(document.view_text(), "abef");
    }

    #[test]
    fn optimistic_timeout_marks_pending_uncommitted_for_retry() {
        let mut document = OptimisticDocument::new(1, "a", 0);
        let mutation_id = document.append_text("b", 0);

        assert!(document.mark_timeouts(1_000).is_empty());
        assert_eq!(document.mark_timeouts(1_001), vec![mutation_id]);
        assert_eq!(
            document.pending().front().map(|pending| pending.status),
            Some(PendingStatus::Uncommitted { retry_count: 1 })
        );

        assert!(document.mark_sent(mutation_id, 1_100));
        assert_eq!(
            document.pending().front().map(|pending| pending.status),
            Some(PendingStatus::InFlight)
        );
    }

    #[test]
    fn optimistic_conflict_keeps_local_view_over_new_server_snapshot() {
        let mut document = OptimisticDocument::new(1, "abc", 0);
        document.append_text(" local", 10);

        document.apply_server_snapshot(ServerSnapshot::new(2, "abc remote"));

        assert_eq!(document.canonical_text(), "abc remote");
        assert_eq!(document.view_text(), "abc remote local");
        assert_eq!(
            document.pending().front().map(|pending| pending.status),
            Some(PendingStatus::Conflicted)
        );
    }

    #[test]
    fn optimistic_cursor_blink_is_local_and_resets_on_input() {
        let mut document = OptimisticDocument::new(1, "", 100);

        assert!(document.cursor.visible_at(100));
        assert!(!document.cursor.visible_at(351));

        document.append_text("x", 400);

        assert_eq!(document.cursor.byte_index, 1);
        assert!(document.cursor.visible_at(650));
    }

    #[test]
    fn optimistic_selection_drag_is_local_until_release() {
        let mut document = OptimisticDocument::new(1, "abcdef", 0);

        document.begin_selection_drag(1);
        document.update_selection_drag(4);

        assert_eq!(
            document.selection.visible_range(),
            Some(TextRange::new(1, 4))
        );
        assert!(document.pending().is_empty());

        let mutation_id = document.release_selection_drag(12);

        assert_eq!(mutation_id, Some(1));
        assert_eq!(document.selection.committed, Some(TextRange::new(1, 4)));
        assert_eq!(document.pending().len(), 1);
        assert!(matches!(
            document.pending().front().map(|pending| &pending.mutation),
            Some(OptimisticMutation::Selection(SelectionMutation::Set {
                anchor: 1,
                focus: 4
            }))
        ));
    }

    #[test]
    fn optimistic_hover_highlight_is_local_only() {
        let mut document = OptimisticDocument::new(1, "abcdef", 0);

        document.set_hover_range(Some(TextRange::new(2, 5)));

        assert_eq!(document.selection.hover, Some(TextRange::new(2, 5)));
        assert!(document.pending().is_empty());
    }
}
