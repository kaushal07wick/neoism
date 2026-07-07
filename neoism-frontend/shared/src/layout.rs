//! Rect-based layout primitives shared by every panel.
//!
//! `Rect` is the universal coordinate type (logical pixels, top-left
//! origin). `PanelLayout` packages the rect a single panel was given
//! along with the active DPI scale. `ChromeLayout` is the parent
//! frame's view of which panels exist and where they go this frame.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelLayout {
    pub bounds: Rect,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChromeLayout {
    /// Top-of-window chrome strip (panel toggle + hamburger menu).
    /// Sits above `buffer_tabs` and spans the full viewport width.
    /// `None` when the host has hidden the bar entirely.
    #[serde(default)]
    pub top_bar: Option<Rect>,
    pub file_tree: Option<Rect>,
    pub buffer_tabs: Rect,
    #[serde(default)]
    pub breadcrumbs: Option<Rect>,
    pub status_line: Rect,
    pub terminal: Rect,
    pub command_palette: Option<Rect>,
    /// Modal finder rect. `None` while the finder is hidden so callers
    /// can distinguish "panel exists but invisible" from "no slot at
    /// all" without consulting the panel state.
    #[serde(default)]
    pub finder: Option<Rect>,
    /// Modal git-diff overlay rect. Same `Option` semantics as
    /// `finder` — `None` while the diff panel is hidden.
    #[serde(default)]
    pub git_diff: Option<Rect>,
    /// Sticky command composer rect. `None` while the composer is
    /// hidden (it docks above the status line when shown).
    #[serde(default)]
    pub command_composer: Option<Rect>,
}
