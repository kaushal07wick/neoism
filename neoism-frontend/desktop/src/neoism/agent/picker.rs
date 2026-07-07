//! Desktop re-export shim for the agent inline picker types.
//!
//! The struct/enum/impls now live in the shared crate at
//! `neoism_ui::panels::agent_pane::state::picker`. This file re-exports
//! them so existing call sites in this crate
//! (`crate::neoism::agent::picker::NeoismAgentPicker`, ...) keep
//! resolving without code changes. The desktop and web frontends now
//! share a single picker implementation.

pub use neoism_ui::panels::agent_pane::state::picker::{
    NeoismAgentPicker, NeoismAgentPickerKind, NeoismAgentPickerOption,
};
