//! Pure render-policy helpers shared by native + web hosts.
//!
//! Everything in this module takes POD inputs and returns a POD
//! decision: row plans, pixel offsets, layout deltas. The host
//! (native sugarloaf, web canvas) decides HOW to paint; this module
//! decides WHAT/WHEN to paint.
//!
//! The native renderer historically computed these helpers inline
//! inside `frontends/neoism/src/screen/mod.rs` and `screen/render/`,
//! which forced the web frontend to either re-implement them or skip
//! features that depend on smooth-scroll geometry. Lifting them here
//! removes that fork.

mod blocks;
mod editor_scroll;
mod frame;

pub use blocks::*;
pub use editor_scroll::*;
pub use frame::*;

#[cfg(test)]
mod tests;
