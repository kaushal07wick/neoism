//! Renderer-independent session layout model.
//!
//! This module intentionally has no Sugarloaf, Taffy, PTY, or native-window
//! dependencies. Native and web frontends can adapt their pane/session state by
//! storing route ids or other host ids in [`SessionLeaf::external_id`] and then
//! applying split/focus/close decisions from this model to their renderer-owned
//! resources.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod layout_impl;
mod plans;
mod snapshot;

pub use plans::*;
pub use snapshot::*;

const MIN_SPLIT_RATIO: f32 = 0.10;
const MAX_SPLIT_RATIO: f32 = 0.90;

/// Default keyboard step (logical px) for nudging a vertical split divider
/// up/down. Lives in the shared crate so desktop and web bind the same
/// chord to the same nudge size.
pub const DIVIDER_KEYBOARD_STEP_VERTICAL: f32 = 20.0;

/// Default keyboard step (logical px) for nudging a horizontal split divider
/// left/right. Larger than the vertical step because horizontal panes are
/// typically wider than they are tall.
pub const DIVIDER_KEYBOARD_STEP_HORIZONTAL: f32 = 40.0;

/// Lines-to-pixels factor used when a mouse wheel reports `LineDelta`. The
/// buffer-tabs strip and similar narrow scrollers want a brisker step than
/// the default editor wheel so a single notch reveals roughly one tab.
pub const BUFFER_TABS_WHEEL_LINE_TO_PX: f32 = 60.0;

/// Tolerance (logical px in physical space) used to decide whether two pane
/// rectangles sit on the "same top row" for chrome-alignment purposes. Taffy
/// occasionally introduces 1-2 px of jitter on what should be the top edge;
/// 4 px absorbs the noise without merging a real second row.
pub const PANE_TOP_ALIGN_TOLERANCE_PX: f32 = 4.0;

/// Host-neutral mouse scroll delta.
///
/// Mirrors `winit::event::MouseScrollDelta` (which the native frontend
/// receives) so the policy helpers below can be unit tested without pulling
/// in the windowing crate. Web frontends synthesize the same variants from
/// their DOM `wheel` events.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SessionScrollDelta {
    /// Whole-line delta - `x` and `y` are typically integer-ish and the OS
    /// chooses the sign convention.
    Lines { x: f32, y: f32 },
    /// Pixel delta from high-resolution trackpads.
    Pixels { x: f32, y: f32 },
}

/// Geometry inputs for placing a per-pane tab strip.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PaneStripGeomInput {
    /// Physical-px `rect[0]` from the layout engine (taffy on desktop).
    pub rect_left_phys: f32,
    /// Physical-px `rect[1]` from the layout engine.
    pub rect_top_phys: f32,
    /// Physical-px width of the pane rect.
    pub rect_width_phys: f32,
    /// Physical-px `scaled_margin.left` for the grid root.
    pub scaled_margin_left_phys: f32,
    /// Physical-px `scaled_margin.top` for the grid root.
    pub scaled_margin_top_phys: f32,
    /// Logical-px y at which the workspace chrome (island/tab strip) row
    /// begins. Top-aligned panes share this row.
    pub chrome_top_logical: f32,
    /// Physical-px smallest `rect[1]` across all visible panes in the grid.
    /// Panes whose top is within [`PANE_TOP_ALIGN_TOLERANCE_PX`] of this are
    /// considered top-aligned and rendered in the chrome row.
    pub min_top_phys: f32,
    /// Device scale factor (physical-px per logical-px).
    pub scale_factor: f32,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct SessionTabId(pub u64);

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct SessionNodeId(pub u64);

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct SessionLeafId(pub u64);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitPlacement {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionLeafKind {
    Terminal,
    Editor,
    Agent,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLeaf {
    pub id: SessionLeafId,
    pub kind: SessionLeafKind,
    pub title: Option<String>,
    /// Optional host-owned identifier, such as a native route id or web pane id.
    ///
    /// The shared model never interprets this value; adapters use it as the
    /// conversion point back to renderer/process resources.
    pub external_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLeafSpec {
    pub kind: SessionLeafKind,
    pub title: Option<String>,
    pub external_id: Option<u64>,
}

impl SessionLeafSpec {
    pub fn new(kind: SessionLeafKind) -> Self {
        Self {
            kind,
            title: None,
            external_id: None,
        }
    }

    pub fn with_external_id(mut self, external_id: u64) -> Self {
        self.external_id = Some(external_id);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionSplit {
    pub axis: SplitAxis,
    pub first: SessionNodeId,
    pub second: SessionNodeId,
    /// Fraction of available space assigned to [`SessionSplit::first`].
    pub ratio: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SessionNode {
    Leaf(SessionLeaf),
    Split(SessionSplit),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionTab {
    pub id: SessionTabId,
    pub title: Option<String>,
    pub root: SessionNodeId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionLayout {
    tabs: Vec<SessionTab>,
    active_tab: usize,
    focused_leaf: SessionLeafId,
    nodes: BTreeMap<SessionNodeId, SessionNode>,
    next_tab: u64,
    next_node: u64,
    next_leaf: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSplitPreview {
    pub focused_external_id_before: Option<u64>,
    pub focused_external_id_after: Option<u64>,
    pub active_external_ids_after: Vec<u64>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SessionTabStripRef {
    Workspace,
    Pane(u64),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SessionMovableTabKind {
    FileLike,
    AgentRoute,
    AgentTerminal,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SessionTabMoveDestination {
    Workspace,
    ExistingPane(u64),
    NewSplit,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SessionTabMovePlan {
    pub source: SessionTabStripRef,
    pub destination: SessionTabMoveDestination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseUnfocusedTabsPlan {
    pub retained_index: usize,
    pub active_index_after: usize,
    pub remove_indices_desc: Vec<usize>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceTabClickPlan {
    Ignore,
    ToggleColorPicker {
        tab: usize,
    },
    BeginDrag {
        tab: usize,
        switch_to: Option<usize>,
        close_color_picker: bool,
    },
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SessionPaneRect<K> {
    pub id: K,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl<K> SessionPaneRect<K> {
    pub fn new(id: K, rect: [f32; 4]) -> Self {
        Self {
            id,
            left: rect[0],
            top: rect[1],
            width: rect[2],
            height: rect[3],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionLayoutError {
    EmptyTabs,
    InvalidActiveTab,
    MissingNode(SessionNodeId),
    MissingLeaf(SessionLeafId),
    FocusedLeafOutsideActiveTab(SessionLeafId),
    LastLeaf,
    Cycle(SessionNodeId),
    InvalidSplitRatio(SessionNodeId, f32),
    /// A [`PaneLayoutSnapshot`](neoism_protocol::workspace::PaneLayoutSnapshot)
    /// produced no panes to mirror (e.g. an empty `Split`/`Tabs` node).
    EmptySnapshot,
}

#[cfg(test)]
mod tests;
