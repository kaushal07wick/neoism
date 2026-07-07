// Platform-specific default key bindings.
//
// Selects the appropriate `platform_key_bindings` implementation based on
// the target OS. Each backend is gated by `cfg` so only one is compiled in
// per build.

#[cfg(all(target_os = "macos", not(test)))]
pub use macos::platform_key_bindings;

// The real macOS table is `cfg(not(test))` (pre-existing); give the
// test cfg an empty stub so the bin's unit tests compile and default
// bindings simply omit platform extras.
#[cfg(all(target_os = "macos", test))]
pub fn platform_key_bindings(
    _use_navigation_key_bindings: bool,
    _use_splits: bool,
    _config_keyboard: neoism_backend::config::keyboard::Keyboard,
) -> Vec<crate::bindings::action::KeyBinding> {
    Vec::new()
}

#[cfg(target_os = "linux")]
pub use linux::platform_key_bindings;

#[cfg(target_os = "windows")]
pub use windows::platform_key_bindings;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub use bsd::platform_key_bindings;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub mod bsd;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
