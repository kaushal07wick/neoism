use serde::{Deserialize, Serialize};

#[inline]
fn default_true() -> bool {
    true
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Effects {
    #[serde(default = "bool::default", rename = "custom-mouse-cursor")]
    pub custom_mouse_cursor: bool,
    /// Neovide-style critically-damped cursor trail.
    /// Default ON for the IDE fork — set `effects.trail-cursor = false` to disable.
    #[serde(default = "default_true", rename = "trail-cursor")]
    pub trail_cursor: bool,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            custom_mouse_cursor: false,
            trail_cursor: true,
        }
    }
}
