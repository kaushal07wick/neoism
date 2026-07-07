// Binding<T>, MouseAction, BindingMode, Action, default_key_bindings and
// including their comments was originally taken from
// https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/config/bindings.rs
// which is licensed under Apache 2.0 license.
//
// This module is split into:
// * `action`    - core types (`Action`, `Binding<T>`, `BindingMode`, ...).
// * `defaults`  - default key/mouse bindings + user-config conversion.
// * `macros`    - the `bindings!` / `trigger!` DSL macros.
// * `platform`  - per-OS `platform_key_bindings` selector.
// * `tests`     - moved test cases (test-cfg only).

pub mod action;
pub mod defaults;
pub mod macros;
pub mod platform;

#[cfg(test)]
mod tests;

pub use action::*;
pub use defaults::*;

// Re-export the binding-construction macros for use by tests and any other
// consumers inside this crate.
#[cfg(test)]
pub(crate) use macros::{bindings, trigger};

// Re-export the small handful of upstream types/values the test module
// references via `use super::*;`. Kept narrow on purpose.
#[cfg(test)]
pub use neoism_backend::config::bindings::KeyBinding as ConfigKeyBinding;
#[cfg(test)]
pub use neoism_window::keyboard::Key::*;
#[cfg(test)]
pub use neoism_window::keyboard::NamedKey::*;
#[cfg(test)]
pub use neoism_window::keyboard::{Key, KeyLocation};
