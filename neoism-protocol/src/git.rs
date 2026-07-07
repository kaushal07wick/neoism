//! Git-related wire messages.
//!
//! Phase 9: status / diff / log. Paths inside hunks and status entries are
//! repository-relative.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GitClientMessage {
    Status,
    Diff { path: Option<String> },
    Log { max_count: Option<u32> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GitServerMessage {
    Status {
        entries: Vec<GitStatusEntry>,
    },
    Diff {
        hunks: Vec<DiffHunk>,
    },
    Log {
        commits: Vec<CommitSummary>,
    },
    /// Current branch name of the workspace root repo, or `None` when the
    /// workspace is not a git repository / detached HEAD. The daemon pushes
    /// this unsolicited on WebSocket connect so the chrome status line can
    /// paint a real branch instead of a stub.
    Branch {
        name: Option<String>,
    },
    /// Aggregate working-tree change counts derived from
    /// `git status --porcelain=v1`. `added` covers untracked files (`??`);
    /// `deleted` covers index/worktree deletions (`D ` or ` D`). Everything
    /// else is folded into either bucket depending on whether the entry
    /// introduces or removes content. Pushed by the daemon on a poll
    /// interval so the chrome status pill can stay live with the disk.
    Changes {
        added: u64,
        deleted: u64,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusEntry {
    pub path: String,
    pub status: GitFileStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub path: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub patch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSummary {
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub message: String,
    pub timestamp: i64,
}
