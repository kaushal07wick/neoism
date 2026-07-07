//! `neoism-remarkable` — the reMarkable sync **plugin** (opt-in, WIP).
//!
//! Separated out of Neoism core: the desktop app pulls this in only behind
//! its `remarkable` cargo feature, so a default build contains **no**
//! reMarkable code. Core draw-over-markdown (the ink overlay + its hidden
//! sidecar) lives in `neoism_ui::editor::neodraw` and does not depend on
//! this crate.
//!
//! End-state (next): this becomes a daemon-client process that speaks
//! `neoism-protocol` (files + crdt) to `neoism-workspace-daemon`, so it
//! runs out-of-process and shares the multiplayer/CRDT seam.
#![allow(dead_code)]

pub mod auto;
pub mod controller;
pub mod ink_interop;
pub mod vault_sync;

pub use auto::RemarkableAutoSync;
pub use controller::{RemarkableSync, DEFAULT_BRIDGE_PORT};
pub use vault_sync::{default_host, sync_vault, SyncOutcome};
