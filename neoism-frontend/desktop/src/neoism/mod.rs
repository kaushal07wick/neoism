#[cfg(not(target_arch = "wasm32"))]
pub mod acp;
pub mod agent;
pub mod assistant_overlay;
pub mod icon;
pub mod ide_tools;
pub use neoism_ui::panels::splash_overlay;
pub mod view;
