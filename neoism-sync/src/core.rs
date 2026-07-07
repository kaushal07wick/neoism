use loro::{ExportMode, LoroDoc, Subscription, VersionVector};

/// Errors from the CRDT core. Thin wrappers so callers don't depend on
/// Loro's error types directly.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("crdt import failed: {0}")]
    Import(String),
    #[error("crdt export failed: {0}")]
    Export(String),
    #[error("version decode failed: {0}")]
    Version(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// A document-agnostic CRDT document.
///
/// This is the reusable heart of cross-device Neoism: it knows how to
/// produce update blobs from local edits, apply remote update blobs, and
/// answer "what version am I at" — and nothing about *what* is stored.
/// Schemas like [`crate::NoteDoc`] (and a future `CodeDoc`) embed one of
/// these and add typed accessors over its containers.
pub struct SyncDoc {
    doc: LoroDoc,
}

impl SyncDoc {
    pub fn new() -> Self {
        Self {
            doc: LoroDoc::new(),
        }
    }

    /// Construct with an explicit peer id. Each device/replica should use
    /// a stable, distinct id so concurrent edits are attributed and
    /// ordered deterministically.
    pub fn with_peer_id(peer: u64) -> Self {
        let doc = LoroDoc::new();
        let _ = doc.set_peer_id(peer);
        Self { doc }
    }

    /// Borrow the underlying Loro document so a schema can reach its
    /// containers (`get_text`, `get_list`, …).
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// Seal the current batch of edits into a commit. Call after a set of
    /// mutations so they export as one update.
    pub fn commit(&self) {
        self.doc.commit();
    }

    /// A full, self-contained snapshot — the whole document state. Use
    /// this for first contact with a peer or to persist to disk.
    pub fn snapshot(&self) -> Result<Vec<u8>, SyncError> {
        self.doc
            .export(ExportMode::Snapshot)
            .map_err(|e| SyncError::Export(e.to_string()))
    }

    /// Only the operations this doc has that the given peer version is
    /// missing — the compact delta you stream while live.
    pub fn export_from(&self, since: &[u8]) -> Result<Vec<u8>, SyncError> {
        let vv = VersionVector::decode(since)
            .map_err(|e| SyncError::Version(e.to_string()))?;
        self.doc
            .export(ExportMode::updates(&vv))
            .map_err(|e| SyncError::Export(e.to_string()))
    }

    /// This doc's current version vector, encoded — hand it to a peer so
    /// it can [`export_from`](Self::export_from) just the delta you need.
    pub fn version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }

    /// Apply a remote snapshot or update blob. Idempotent and
    /// order-independent: applying the same or overlapping updates twice
    /// is safe, which is what makes flaky links and offline merges work.
    pub fn import(&self, blob: &[u8]) -> Result<(), SyncError> {
        self.doc
            .import(blob)
            .map(|_| ())
            .map_err(|e| SyncError::Import(e.to_string()))
    }

    /// Fire `on_update` with an encoded delta every time this doc commits
    /// a *local* change. This is the spout transports drain to stream
    /// edits live. Keep the returned [`Subscription`] alive — dropping it
    /// unsubscribes.
    pub fn on_local_update<F>(&self, on_update: F) -> Subscription
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.doc.subscribe_local_update(Box::new(move |update| {
            on_update(update);
            true
        }))
    }
}

impl Default for SyncDoc {
    fn default() -> Self {
        Self::new()
    }
}
