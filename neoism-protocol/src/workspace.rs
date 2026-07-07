//! Workspace / project / session control messages.
//!
//! Phase 11: replace the Ping/Pong stub with the real workspace command
//! surface so the web frontend can manage projects (open, close, switch),
//! enumerate sessions, and track per-session cwd state.
//!
//! Path-flavoured file/dir I/O (`ListDir`, `ReadFile`, `WriteFile`,
//! `WalkTree`) lives in [`crate::files`]; git-flavoured operations
//! (`Status`, `Diff`, `Log`, `Branch`, `Changes`) live in [`crate::git`].
//! Editor (nvim proxy) buffer state lives in [`crate::editor`]. This
//! module is intentionally about **workspace identity**, **project
//! registry**, **per-session cwd**, and the **active-session pointer**
//! — anything coarser than a single file or repo operation.
//!
//! Both enums are externally tagged (the default for serde) — same
//! serialization shape as `files.rs` / `git.rs`, so the chrome's
//! `coerceServerMessage` switch in `ProtocolClient.ts` can extend its
//! single-tag-key dispatch with `WorkspaceReply` without touching its
//! parser.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::diagnostics::RouteId;

mod client_message;
mod pane_layout;
mod server_message;
mod summary;
mod tasks;

pub use client_message::*;
pub use pane_layout::*;
pub use server_message::*;
pub use summary::*;
pub use tasks::*;

#[cfg(test)]
mod tests;
