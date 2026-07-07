//! `neoism-sync` — CRDT-backed collaborative documents for Neoism.
//!
//! The design goal: "live", "offline-merge", and "multiplayer" are one
//! mechanism. A [`SyncDoc`] is a thin, **document-agnostic** wrapper over
//! a Loro CRDT. It produces and consumes opaque update blobs; every
//! transport (LAN peer, the reMarkable bridge, a future cloud relay)
//! just shuttles those blobs. Nothing in the core knows what schema the
//! payload carries.
//!
//! On top of the core sit thin **schemas**:
//! - [`NoteDoc`] — markdown text + an ink layer whose strokes are
//!   anchored to relative text positions, so handwriting follows the
//!   words as the markdown reflows.
//! - A future `CodeDoc` (live collaborative source editing) would embed
//!   the very same [`SyncDoc`] and reuse discovery/transport/presence
//!   wholesale.
//!
//! Right now we wire the Neoism ↔ reMarkable path; the only
//! reMarkable-specific code lives in the bridge, never here.

pub mod bridge;
mod core;
#[cfg(feature = "lan")]
pub mod discovery;
pub mod export;
pub mod net;
mod note;
mod peer;
pub mod remarkable;
mod stroke;
pub mod sync_plan;

pub use bridge::{BridgeMsg, BridgeServer};
pub use core::{SyncDoc, SyncError};
pub use export::{
    folder_bundle, markdown_to_pdf, pdf_document_bundle, stable_uuid, DocBundle,
    RenderedPdf,
};
pub use net::{NetError, TcpPeer};
pub use note::{NoteDoc, TextEdit};
pub use peer::{PeerId, SyncPeer};
pub use remarkable::{detect_version, encode_rm_v6, parse_rm, RmError, RmVersion};
pub use stroke::{Color, Stroke, StrokePoint};
pub use sync_plan::{plan_sync, LocalNote, SyncManifest, SyncOp, SyncRecord};

/// Best-effort liveness check for the reMarkable: can we open a TCP
/// connection to its SSH port? Cheap enough to poll for auto-detecting
/// when the tablet is plugged in / on the network. `host` is `host:port`
/// or bare host (defaults to port 22).
pub fn is_remarkable_reachable(host: &str, timeout: std::time::Duration) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    // Accept `user@host` (the SSH form) — strip the user so the address
    // actually resolves, otherwise this always returns false.
    let host = host.rsplit('@').next().unwrap_or(host);
    let hostport = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:22")
    };
    let Ok(mut addrs) = hostport.to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
}

/// The shared coordinate frame both Neoism and the reMarkable agree on:
/// the reMarkable 2 portrait page in device pixels (~226 DPI). Markdown
/// is rendered into this frame and ink is stored in it, so a stroke
/// drawn over a word lands on that word on either device.
pub const PAGE_WIDTH: f32 = 1404.0;
pub const PAGE_HEIGHT: f32 = 1872.0;
