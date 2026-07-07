//! Renderer-independent session layout model.
//!
//! Hosts [`legacy`] (re-exported at the module root) — the production
//! policy helpers and the two-level `SessionLayout { tabs, root }` model
//! that the desktop fork's `context::manager`, `layout::grid::focus`, and
//! friends consume. Must stay byte-for-byte compatible.
//!
//! Renderer-neutral: no Sugarloaf, Taffy, PTY, or native-window
//! dependencies. Adapters thread their host ids through `external_id`
//! fields.

pub mod geometry;
mod legacy;
pub mod tree;

pub use legacy::*;
