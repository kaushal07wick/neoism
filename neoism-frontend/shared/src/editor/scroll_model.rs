//! Shared editor/terminal scroll math.
//!
//! Host frontends still own input devices, nvim RPC, and renderer side
//! effects. This module keeps the portable viewport and autoscroll
//! rules in one place so desktop and web can make identical decisions.


mod editor;
mod terminal;
mod misc;

pub use editor::*;
pub use terminal::*;
pub use misc::*;

#[cfg(test)]
mod tests;
