pub mod state;

/// Re-export the shared markdown render surface so existing call sites
/// in this crate (`screen/bridges/markdown.rs` etc.) keep referencing
/// `crate::editor::markdown::render::*` after the render lift.
pub use neoism_ui::editor::markdown::render;

pub use state::*;
