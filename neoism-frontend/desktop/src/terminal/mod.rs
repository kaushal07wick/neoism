pub mod blocks;
pub mod file_link;
pub mod grid_emit;
pub mod hints;
pub mod scroll;
pub mod watcher;

// `terminal::splash` now points at the lifted module in
// `neoism-ui/src/panels/terminal_splash.rs`. Call sites still use
// `crate::terminal::splash::splash_bytes` / `adapt_layout` /
// `WORDMARK_ASPECT` because all of those are scalar / string APIs
// with no native-flavored theme on them.
pub use neoism_ui::panels::terminal_splash as splash;
