//! Transport-agnostic peer abstraction.
//!
//! Every transport — the LAN socket, the reMarkable bridge, a future
//! cloud relay — implements [`SyncPeer`]. The sync loop is identical for
//! all of them: drain local CRDT updates and `send` them, then `poll` for
//! remote blobs and import them. The transport only moves bytes.

/// A stable per-replica identifier. Derived once per device/install and
/// reused as the Loro peer id so concurrent edits are attributed
/// consistently across reconnects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PeerId(pub u64);

impl PeerId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// A connected counterpart we exchange CRDT update blobs with.
///
/// Implementations are free to be live (a streaming socket) or polled (a
/// file-watching bridge); the contract is the same either way.
pub trait SyncPeer {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Best-effort hint of who's on the other end, once known.
    fn remote_id(&self) -> Option<PeerId> {
        None
    }

    /// Push one CRDT update blob toward the peer.
    fn send(&mut self, update: &[u8]) -> Result<(), Self::Error>;

    /// Collect any update blobs that have arrived since the last call.
    /// Non-blocking: returns an empty vec when nothing is waiting.
    fn poll(&mut self) -> Result<Vec<Vec<u8>>, Self::Error>;

    /// Whether the peer is still reachable.
    fn is_connected(&self) -> bool {
        true
    }
}
