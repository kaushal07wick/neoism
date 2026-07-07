use std::collections::HashMap;
use std::path::PathBuf;

use web_time::Instant;

use crate::primitives::ide_theme::IdeTheme;
use crate::widgets::diff_card::DiffLine;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Staged,
    Mixed,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflict,
}

impl FileStatus {
    pub(super) fn marker(&self) -> &'static str {
        match self {
            FileStatus::Modified => "M",
            FileStatus::Staged => "S",
            FileStatus::Mixed => "M*",
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "?",
            FileStatus::Conflict => "!",
        }
    }

    pub(super) fn color(&self, theme: &IdeTheme) -> [u8; 4] {
        match self {
            FileStatus::Modified => theme.u8(theme.yellow),
            FileStatus::Staged => theme.u8(theme.green),
            FileStatus::Mixed => theme.u8(theme.magenta),
            FileStatus::Added => theme.u8(theme.green),
            FileStatus::Deleted | FileStatus::Conflict => theme.u8(theme.red),
            FileStatus::Renamed => theme.u8(theme.blue),
            FileStatus::Untracked => theme.u8(theme.cyan),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileChange {
    pub path: String,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Default)]
pub(super) struct PanelData {
    pub(super) branch: Option<String>,
    pub(super) repo_root: Option<PathBuf>,
    pub(super) files: Vec<FileChange>,
    pub(super) diffs: HashMap<String, Vec<DiffLine>>,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    pub(super) refresh_id: u64,
    pub(super) last_refresh: Option<Instant>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Rect {
    pub(super) const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };
    pub(super) fn contains(&self, mx: f32, my: f32) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && mx >= self.x
            && mx <= self.x + self.w
            && my >= self.y
            && my <= self.y + self.h
    }
    pub(super) fn as_array(&self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PanelHit {
    Outside,
    Inside,
    Close,
    /// Click landed on a file row in the top files card — caller
    /// promotes it to a selection move + focus.
    FileRow(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarKind {
    Files,
    Diff,
}
