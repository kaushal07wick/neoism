use loro::{
    cursor::{Cursor, Side},
    LoroValue,
};

use crate::core::{SyncDoc, SyncError};
use crate::stroke::Stroke;

/// Container ids inside the CRDT. Stable strings — never rename, they're
/// part of the on-the-wire schema.
const MARKDOWN: &str = "markdown";
const INK: &str = "ink";

/// An incremental edit to the markdown text. The editor feeds these so we
/// mutate the CRDT in place (preserving everyone's anchors and merging
/// concurrent edits) rather than replacing the whole buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEdit {
    Insert { pos: usize, text: String },
    Delete { pos: usize, len: usize },
}

/// A collaborative note: markdown text + an ink layer.
///
/// The two layers share one coordinate frame ([`crate::PAGE_WIDTH`] ×
/// [`crate::PAGE_HEIGHT`]). Each ink stroke can carry a text *anchor* — a
/// Loro cursor pinned to the character it was drawn beside — so the
/// handwriting follows the words as the markdown reflows on either
/// device. This is a thin schema over the reusable [`SyncDoc`].
pub struct NoteDoc {
    inner: SyncDoc,
}

impl NoteDoc {
    pub fn new() -> Self {
        Self {
            inner: SyncDoc::new(),
        }
    }

    pub fn with_peer_id(peer: u64) -> Self {
        Self {
            inner: SyncDoc::with_peer_id(peer),
        }
    }

    /// Seed a fresh note from existing markdown (e.g. an on-disk `.md`).
    pub fn from_markdown(md: &str) -> Self {
        let me = Self::new();
        me.set_markdown(md);
        me
    }

    /// The reusable sync core — transports drain this for updates and feed
    /// it remote ones.
    pub fn sync(&self) -> &SyncDoc {
        &self.inner
    }

    // ---- markdown layer -------------------------------------------------

    pub fn markdown(&self) -> String {
        self.inner.doc().get_text(MARKDOWN).to_string()
    }

    /// Replace the whole markdown body. Convenience for first load; prefer
    /// [`apply_text_edit`](Self::apply_text_edit) for live editing so
    /// anchors and concurrent edits survive.
    pub fn set_markdown(&self, md: &str) {
        let text = self.inner.doc().get_text(MARKDOWN);
        let len = text.len_unicode();
        if len > 0 {
            let _ = text.delete(0, len);
        }
        let _ = text.insert(0, md);
        self.inner.commit();
    }

    /// Apply one incremental edit (positions are in unicode scalar values,
    /// matching Loro's default text indexing).
    pub fn apply_text_edit(&self, edit: &TextEdit) -> Result<(), SyncError> {
        let text = self.inner.doc().get_text(MARKDOWN);
        match edit {
            TextEdit::Insert { pos, text: s } => {
                text.insert(*pos, s)
                    .map_err(|e| SyncError::Import(e.to_string()))?;
            }
            TextEdit::Delete { pos, len } => {
                text.delete(*pos, *len)
                    .map_err(|e| SyncError::Import(e.to_string()))?;
            }
        }
        self.inner.commit();
        Ok(())
    }

    // ---- ink layer ------------------------------------------------------

    /// Append a stroke. Atomic — concurrent strokes from two devices just
    /// union with no conflict.
    pub fn add_stroke(&self, stroke: &Stroke) -> Result<(), SyncError> {
        let bytes = serde_json::to_vec(stroke)?;
        let ink = self.inner.doc().get_list(INK);
        ink.push(LoroValue::Binary(bytes.into()))
            .map_err(|e| SyncError::Import(e.to_string()))?;
        self.inner.commit();
        Ok(())
    }

    /// All strokes currently in the note, in insertion order.
    pub fn strokes(&self) -> Vec<Stroke> {
        let mut out = Vec::new();
        if let LoroValue::List(items) = self.inner.doc().get_list(INK).get_value() {
            for item in items.iter() {
                if let LoroValue::Binary(bytes) = item {
                    if let Ok(stroke) = serde_json::from_slice::<Stroke>(bytes) {
                        out.push(stroke);
                    }
                }
            }
        }
        out
    }

    /// Strokes belonging to one reMarkable page.
    pub fn page_strokes(&self, page_id: &str) -> Vec<Stroke> {
        self.strokes()
            .into_iter()
            .filter(|s| s.page.as_deref() == Some(page_id))
            .collect()
    }

    /// Make the stored strokes for `page_id` exactly match `incoming`
    /// (matched by stable id): add new strokes, drop erased ones, and
    /// leave unchanged strokes untouched. Idempotent — re-applying the
    /// same page is a no-op — and CRDT-friendly, since only real changes
    /// mutate the document (so it merges cleanly with concurrent edits).
    pub fn sync_page(&self, page_id: &str, incoming: &[Stroke]) -> Result<(), SyncError> {
        use std::collections::HashSet;
        let ink = self.inner.doc().get_list(INK);

        let mut existing: Vec<(usize, u64)> = Vec::new();
        if let LoroValue::List(items) = ink.get_value() {
            for (i, item) in items.iter().enumerate() {
                if let LoroValue::Binary(bytes) = item {
                    if let Ok(s) = serde_json::from_slice::<Stroke>(bytes) {
                        if s.page.as_deref() == Some(page_id) {
                            existing.push((i, s.id));
                        }
                    }
                }
            }
        }

        let incoming_ids: HashSet<u64> = incoming.iter().map(|s| s.id).collect();
        // Remove erased strokes (reverse order keeps earlier indices valid).
        for (idx, id) in existing.iter().rev() {
            if !incoming_ids.contains(id) {
                ink.delete(*idx, 1)
                    .map_err(|e| SyncError::Import(e.to_string()))?;
            }
        }

        let existing_ids: HashSet<u64> = existing.iter().map(|(_, id)| *id).collect();
        for stroke in incoming {
            if !existing_ids.contains(&stroke.id) {
                let mut tagged = stroke.clone();
                tagged.page = Some(page_id.to_string());
                let bytes = serde_json::to_vec(&tagged)?;
                ink.push(LoroValue::Binary(bytes.into()))
                    .map_err(|e| SyncError::Import(e.to_string()))?;
            }
        }
        self.inner.commit();
        Ok(())
    }

    /// Apply one message from the reMarkable bridge. `PageInk` merges that
    /// page's strokes; `Hello` is informational.
    pub fn apply_bridge(&self, msg: &crate::bridge::BridgeMsg) -> Result<(), SyncError> {
        match msg {
            crate::bridge::BridgeMsg::PageInk { page_id, strokes } => {
                self.sync_page(page_id, strokes)
            }
            crate::bridge::BridgeMsg::Hello { .. } => Ok(()),
        }
    }

    // ---- anchoring ------------------------------------------------------

    /// Encode a text-position anchor for a stroke about to be drawn at
    /// `text_pos`. Store the result on [`Stroke::anchor`].
    pub fn anchor_at(&self, text_pos: usize) -> Option<Vec<u8>> {
        self.inner
            .doc()
            .get_text(MARKDOWN)
            .get_cursor(text_pos, Side::Left)
            .map(|c| c.encode())
    }

    /// Resolve a stored anchor to the text position it now points at after
    /// any reflow. `None` if the anchor can't be decoded/located.
    pub fn resolve_anchor(&self, anchor: &[u8]) -> Option<usize> {
        let cursor = Cursor::decode(anchor).ok()?;
        self.inner
            .doc()
            .get_cursor_pos(&cursor)
            .ok()
            .map(|r| r.current.pos)
    }
}

impl Default for NoteDoc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stroke::{Color, Stroke, StrokePoint};

    fn pt(x: f32, y: f32) -> StrokePoint {
        StrokePoint {
            x,
            y,
            pressure: 1.0,
        }
    }

    #[test]
    fn markdown_roundtrips() {
        let doc = NoteDoc::from_markdown("# Title\n\nhello");
        assert_eq!(doc.markdown(), "# Title\n\nhello");
    }

    #[test]
    fn ink_roundtrips() {
        let doc = NoteDoc::new();
        let s = Stroke::new(1, vec![pt(10.0, 10.0), pt(20.0, 30.0)], 2.0, Color::BLACK);
        doc.add_stroke(&s).unwrap();
        let got = doc.strokes();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], s);
    }

    #[test]
    fn sync_page_adds_removes_and_is_idempotent() {
        use std::collections::HashSet;
        let doc = NoteDoc::new();
        let s = |id| Stroke::new(id, vec![pt(0.0, 0.0)], 1.0, Color::BLACK);

        doc.sync_page("p1", &[s(1), s(2), s(3)]).unwrap();
        assert_eq!(doc.page_strokes("p1").len(), 3);

        // Re-applying the identical page changes nothing.
        doc.sync_page("p1", &[s(1), s(2), s(3)]).unwrap();
        assert_eq!(doc.page_strokes("p1").len(), 3);

        // Erase 2, add 4 → {1,3,4}.
        doc.sync_page("p1", &[s(1), s(3), s(4)]).unwrap();
        let ids: HashSet<u64> = doc.page_strokes("p1").iter().map(|s| s.id).collect();
        assert_eq!(ids, HashSet::from([1, 3, 4]));

        // A different page is independent.
        doc.sync_page("p2", &[s(9)]).unwrap();
        assert_eq!(doc.page_strokes("p1").len(), 3);
        assert_eq!(doc.page_strokes("p2").len(), 1);
    }

    #[test]
    fn concurrent_strokes_union_on_merge() {
        // Two devices each draw a stroke offline, then sync. CRDT unions
        // them with no conflict — this is the core promise.
        let a = NoteDoc::with_peer_id(1);
        let b = NoteDoc::with_peer_id(2);
        a.add_stroke(&Stroke::new(1, vec![pt(0.0, 0.0)], 1.0, Color::BLACK))
            .unwrap();
        b.add_stroke(&Stroke::new(2, vec![pt(5.0, 5.0)], 1.0, Color::BLACK))
            .unwrap();
        let snap_a = a.sync().snapshot().unwrap();
        let snap_b = b.sync().snapshot().unwrap();
        a.sync().import(&snap_b).unwrap();
        b.sync().import(&snap_a).unwrap();
        assert_eq!(a.strokes().len(), 2);
        assert_eq!(b.strokes().len(), 2);
    }

    #[test]
    fn delta_export_from_version_catches_peer_up() {
        let a = NoteDoc::with_peer_id(1);
        let b = NoteDoc::with_peer_id(2);
        let b_version = b.sync().version();
        a.set_markdown("hello");
        let delta = a.sync().export_from(&b_version).unwrap();
        b.sync().import(&delta).unwrap();
        assert_eq!(b.markdown(), "hello");
    }

    #[test]
    fn anchor_follows_reflow() {
        // The golden-standard ink overlay: a stroke pinned beside a word
        // must ride along when text before it changes.
        let doc = NoteDoc::from_markdown("hello world");
        let anchor = doc.anchor_at(6).expect("anchor at 'w'");
        assert_eq!(doc.resolve_anchor(&anchor), Some(6));
        doc.apply_text_edit(&TextEdit::Insert {
            pos: 0,
            text: "big ".into(),
        })
        .unwrap();
        // 4 chars inserted ahead of it → anchor now resolves to 10.
        assert_eq!(doc.resolve_anchor(&anchor), Some(10));
    }

    #[test]
    fn concurrent_text_edits_merge_deterministically() {
        let a = NoteDoc::with_peer_id(1);
        let b = NoteDoc::with_peer_id(2);
        a.set_markdown("base");
        b.sync().import(&a.sync().snapshot().unwrap()).unwrap();
        a.apply_text_edit(&TextEdit::Insert {
            pos: 0,
            text: "A".into(),
        })
        .unwrap();
        b.apply_text_edit(&TextEdit::Insert {
            pos: 4,
            text: "B".into(),
        })
        .unwrap();
        let da = a.sync().snapshot().unwrap();
        let db = b.sync().snapshot().unwrap();
        a.sync().import(&db).unwrap();
        b.sync().import(&da).unwrap();
        assert_eq!(a.markdown(), b.markdown());
        assert_eq!(a.markdown(), "AbaseB");
    }
}
