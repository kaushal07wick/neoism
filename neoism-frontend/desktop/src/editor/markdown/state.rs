//! Native shim for the markdown editor state.
//!
//! The pure state + helper logic now lives in
//! `neoism-ui/src/editor/markdown/`. This file simply re-exports the
//! lifted module so every existing call site in this crate
//! (`screen/bridges/markdown.rs`, `screen/panes.rs`,
//! `screen/selection.rs`, `chrome/panels/context_menu.rs`, plus the
//! render half of this module that still uses `IdeTheme` and lives
//! next door) keeps compiling against `crate::editor::markdown::state`
//! without code changes.
//!
//! When the render half of the markdown editor follows the buffer-tabs
//! precedent and lifts as well, this shim will disappear — the lifted
//! crate will own the whole markdown surface.

pub use neoism_ui::editor::markdown::*;
