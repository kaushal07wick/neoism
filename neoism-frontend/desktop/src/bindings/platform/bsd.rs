// BSD / fallback default key bindings.
//
// Used on platforms that aren't explicitly handled (macOS, Linux, Windows).
// Mirrors the historical fallback that lived at the tail of
// `bindings/mod.rs` with all arguments underscored.

use crate::bindings::action::KeyBinding;
use neoism_backend::config::keyboard::Keyboard as ConfigKeyboard;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows", test)))]
pub fn platform_key_bindings(_: bool, _: bool, _: ConfigKeyboard) -> Vec<KeyBinding> {
    vec![]
}

#[cfg(test)]
pub fn platform_key_bindings(_: bool, _: bool, _: ConfigKeyboard) -> Vec<KeyBinding> {
    vec![]
}
