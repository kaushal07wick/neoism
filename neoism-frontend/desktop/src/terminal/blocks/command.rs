//! Desktop re-export of the shared `terminal_blocks::command` module.
//! The actual `TerminalCommandBlock` / `CommandBlockSnapshot` /
//! `BlockStatusKind` definitions live in `neoism-ui` so the web
//! frontend reads the same shape.

pub use neoism_ui::terminal_blocks::command::*;
