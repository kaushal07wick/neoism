//! Native shim for the `.neodraw` sketch editor.
//!
//! The pure model + renderer live in the shared crate at
//! `neoism_ui::editor::neodraw`. This re-export lets call sites in this
//! crate reference `crate::editor::neodraw::*` consistently with the
//! markdown surface next door.

pub use neoism_ui::editor::neodraw::*;
