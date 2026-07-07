//! Desktop re-export of the shared `terminal_blocks::chrome` module.
//!
//! All chrome composition (the `compose_block_chrome*` family, block
//! header spans, source-row mapping) lives in `neoism-ui` so the web
//! frontend renders identical Warp-style blocks. Desktop keeps this
//! file purely for path-compatibility with code that still spells
//! `crate::terminal::blocks::chrome::*`.

#[allow(unused_imports)]
pub use neoism_ui::terminal_blocks::chrome::{
    compose_block_chrome, compose_block_chrome_window,
    compose_block_chrome_window_pinned_bottom, row_is_empty, row_text, BlockChromeFrame,
    BlockChromeWindow, BlockHeaderSpan, COMMAND_BLOCK_CHROME_ROWS,
    COMMAND_BLOCK_COMMAND_ROW, COMMAND_BLOCK_META_ROW,
};
