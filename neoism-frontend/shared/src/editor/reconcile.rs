//! Server-confirm reconcile policy for optimistic editor state.
//!
//! The reconcile loop treats server snapshots as authoritative canonical
//! state, then reapplies pending local text mutations to keep the user's view
//! responsive. Layout is intentionally server-wins; richer CRDT behavior can
//! replace the text replay rule later without changing frontend transport.

use crate::editor::optimistic::{
    apply_pending_text, MutationId, OptimisticMutation, PendingMutation, PendingStatus,
    Revision, ServerSnapshot, TextRange,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutSnapshot<T> {
    pub revision: Revision,
    pub value: T,
}

impl<T> LayoutSnapshot<T> {
    pub fn new(revision: Revision, value: T) -> Self {
        Self { revision, value }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPrediction {
    pub cursor_byte_index: usize,
    pub selection: Option<TextRange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconcileConflict {
    PendingTextOverNewerServer {
        mutation_id: MutationId,
        base_revision: Revision,
        server_revision: Revision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcileOutput<L> {
    pub canonical_text: String,
    pub view_text: String,
    pub revision: Revision,
    pub pending: Vec<PendingMutation>,
    pub conflicts: Vec<ReconcileConflict>,
    pub layout: Option<LayoutSnapshot<L>>,
    pub local_prediction: Option<LocalPrediction>,
}

pub fn reconcile_text_view<'a>(
    snapshot: ServerSnapshot,
    pending: impl IntoIterator<Item = &'a PendingMutation>,
) -> ReconcileOutput<()> {
    reconcile(snapshot, pending, None::<LayoutSnapshot<()>>, None)
}

pub fn reconcile<'a, L>(
    snapshot: ServerSnapshot,
    pending: impl IntoIterator<Item = &'a PendingMutation>,
    layout: Option<LayoutSnapshot<L>>,
    local_prediction: Option<LocalPrediction>,
) -> ReconcileOutput<L> {
    let mut pending: Vec<_> = pending.into_iter().cloned().collect();
    let mut conflicts = Vec::new();

    for mutation in &mut pending {
        if matches!(mutation.mutation, OptimisticMutation::Text(_))
            && mutation.base_revision < snapshot.revision
        {
            conflicts.push(ReconcileConflict::PendingTextOverNewerServer {
                mutation_id: mutation.id,
                base_revision: mutation.base_revision,
                server_revision: snapshot.revision,
            });
            mutation.status = PendingStatus::Conflicted;
        }
    }

    let view_text = apply_pending_text(&snapshot.text, pending.iter());

    ReconcileOutput {
        canonical_text: snapshot.text,
        view_text,
        revision: snapshot.revision,
        pending,
        conflicts,
        layout,
        local_prediction,
    }
}

pub fn server_confirm_ack<'a>(
    snapshot: ServerSnapshot,
    pending: impl IntoIterator<Item = &'a PendingMutation>,
    acked_mutation_id: MutationId,
) -> ReconcileOutput<()> {
    let remaining: Vec<_> = pending
        .into_iter()
        .skip_while(|mutation| mutation.id != acked_mutation_id)
        .skip(1)
        .cloned()
        .collect();
    reconcile_text_view(snapshot, &remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::optimistic::{OptimisticDocument, TextAck};

    #[test]
    fn reconcile_replays_pending_text_over_server_snapshot() {
        let mut document = OptimisticDocument::new(1, "abc", 0);
        document.append_text("d", 1);
        document.insert_text(0, ">", 2);

        let output =
            reconcile_text_view(ServerSnapshot::new(2, "abc remote"), document.pending());

        assert_eq!(output.canonical_text, "abc remote");
        assert_eq!(output.view_text, ">abc remoted");
        assert_eq!(output.conflicts.len(), 2);
        assert!(output
            .pending
            .iter()
            .all(|pending| pending.status == PendingStatus::Conflicted));
    }

    #[test]
    fn reconcile_server_confirm_drops_acked_and_older_pending_mutations() {
        let mut document = OptimisticDocument::new(1, "a", 0);
        document.append_text("b", 1);
        document.append_text("c", 2);

        let output =
            server_confirm_ack(ServerSnapshot::new(2, "ab"), document.pending(), 1);

        assert_eq!(output.canonical_text, "ab");
        assert_eq!(output.view_text, "abc");
        assert_eq!(output.pending.len(), 1);
        assert_eq!(output.pending[0].id, 2);
    }

    #[test]
    fn reconcile_layout_is_server_wins() {
        let output = reconcile(
            ServerSnapshot::new(3, "text"),
            std::iter::empty(),
            Some(LayoutSnapshot::new(99, "server-layout")),
            None,
        );

        assert_eq!(output.view_text, "text");
        assert_eq!(
            output.layout,
            Some(LayoutSnapshot::new(99, "server-layout"))
        );
    }

    #[test]
    fn reconcile_preserves_local_cursor_and_selection_prediction() {
        let prediction = LocalPrediction {
            cursor_byte_index: 4,
            selection: Some(TextRange::new(1, 3)),
        };

        let output = reconcile(
            ServerSnapshot::new(1, "abcd"),
            std::iter::empty(),
            None::<LayoutSnapshot<()>>,
            Some(prediction.clone()),
        );

        assert_eq!(output.local_prediction, Some(prediction));
    }

    #[test]
    fn reconcile_document_ack_keeps_later_pending_local() {
        let mut document = OptimisticDocument::new(1, "a", 0);
        document.append_text("b", 1);
        document.append_text("c", 2);

        assert!(document.acknowledge_text(TextAck::new(1, 2, "ab")));

        assert_eq!(document.canonical_text(), "ab");
        assert_eq!(document.view_text(), "abc");
        assert_eq!(document.pending().len(), 1);
    }
}
