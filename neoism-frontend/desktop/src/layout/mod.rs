pub mod border;
pub mod dimensions;
pub mod grid;
pub mod resize;

#[cfg(test)]
mod compute_tests;

// Re-exports preserve the original `layout::Foo` paths used elsewhere in
// the crate and keep `use super::*;` in compute_tests.rs working.
#[cfg(test)]
pub(crate) use border::compute;
pub use border::BorderDirection;
#[cfg(test)]
pub use border::{MIN_COLS, MIN_LINES};
pub use dimensions::ContextDimension;
pub use grid::{ContextGrid, ContextGridItem};
pub use resize::ResizeState;
