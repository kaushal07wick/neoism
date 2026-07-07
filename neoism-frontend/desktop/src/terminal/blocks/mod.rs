//! Desktop-side terminal blocks layer.
//!
//! Most of the surface — `TerminalInputBuffer`, `TerminalCommandBlock`,
//! chrome composition, completion, history, the
//! `compose_block_chrome*` family — has been lifted into the shared
//! `neoism_ui::terminal_blocks` crate so the web and native frontends
//! produce identical Warp-style block layouts. Each desktop sub-module
//! is a thin re-export shim that points at the shared crate; the
//! desktop-only files (`shell_detect.rs` for `libc::tcgetpgrp` +
//! `/proc/<pid>/comm`, and the host-only parts of `shell.rs` that need
//! `dirs::data_local_dir()`) live alongside.

pub mod chrome;
pub mod command;
pub mod completion;
pub mod echo;
pub mod history;
pub mod input;
pub mod shell;
pub mod shell_detect;

pub use chrome::*;
pub use command::*;
#[allow(unused_imports)]
pub use completion::*;
pub use input::*;
pub use shell::*;
