//! Pure terminal engine for Neoism — the libghostty-equivalent.
//!
//! Builds for wasm32-unknown-unknown. Verified on CI; locally requires
//! `rustup target add wasm32-unknown-unknown` then
//! `cargo build --target wasm32-unknown-unknown -p neoism-terminal-core`.
//!
//! After phase 3b this crate is the *complete* embeddable terminal
//! engine: the `Crosswords` grid state machine, the `Selection`
//! engine, the ANSI `Handler` trait, the ANSI parser back-end
//! (Sixel, kitty graphics, iTerm2 inline images, charset/mode/control
//! tables), plus the dependency-clean leaf modules (UTF-8 validation,
//! batched parser, effect/snapshot ports from phases 2/5).
//!
//! It has no native dependencies — no winit, wgpu, sugarloaf,
//! copypasta, etc. The native renderer in `neoism-backend` consumes
//! these types through the `effects_adapter` boundary.

pub mod ansi;
pub mod batch_utf8;
pub mod batched_parser;
pub mod colors;
pub mod crosswords;
pub mod damage;
pub mod effects;
pub mod graphics;
pub mod handler;
pub mod selection;
pub mod simd_utf8;
pub mod snapshot;

pub use crosswords::Crosswords;
pub use damage::{LineDamage, TerminalDamage};
pub use effects::*;
pub use handler::Handler;
pub use selection::{Selection, SelectionRange, SelectionType};
pub use snapshot::*;

/// Stable identifier the host uses to associate a `Crosswords` terminal
/// with its route / tab / session.
///
/// This is the dependency-clean replacement for the historical
/// `route_id: usize` field on `Crosswords`. It is intentionally a
/// transparent `u64` newtype so:
///
/// * the on-the-wire / on-disk representation is identical to a plain
///   integer (snapshot/restore in later phases);
/// * the native adapter can convert it back to the legacy `route_id:
///   usize` it threads into `RioEvent`s at drain time, byte-for-byte;
/// * future hosts (wasm, daemon) can carry it through wire messages
///   without ever knowing what "route" means in the native app.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct TerminalId(pub u64);

impl TerminalId {
    /// Construct a `TerminalId` from a raw `u64`.
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw integer view, useful for the native adapter when re-emitting
    /// `RioEvent`s that still carry a `route_id: usize`.
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for TerminalId {
    #[inline]
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

impl From<usize> for TerminalId {
    #[inline]
    fn from(raw: usize) -> Self {
        Self(raw as u64)
    }
}

impl std::fmt::Display for TerminalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
