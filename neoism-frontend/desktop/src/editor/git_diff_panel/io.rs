use std::path::Path;
use std::process::Command;

use neoism_ui::panels::git_diff::parse_numstat;

use super::{FileChange, FileStatus};

pub(super) fn collect_files(repo_root: &Path) -> Vec<FileChange> {
    let status = match Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };

    let numstat = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "HEAD", "--numstat", "-z", "--no-color"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_numstat(&o.stdout))
        .unwrap_or_default();

    let mut files = Vec::new();
    let mut i = 0usize;
    while i < status.len() {
        let start = i;
        while i < status.len() && status[i] != 0 {
            i += 1;
        }
        let record = &status[start..i];
        i = i.saturating_add(1);
        if record.len() < 4 {
            continue;
        }
        let x = record[0] as char;
        let y = record[1] as char;
        let path = String::from_utf8_lossy(&record[3..]).into_owned();

        let status_kind = if x == '?' || y == '?' {
            FileStatus::Untracked
        } else if matches!((x, y), ('A', 'A') | ('D', 'D')) || x == 'U' || y == 'U' {
            FileStatus::Conflict
        } else if !matches!(x, ' ' | '?') && !matches!(y, ' ' | '?') {
            FileStatus::Mixed
        } else if x == 'D' || y == 'D' {
            FileStatus::Deleted
        } else if x == 'A' || y == 'A' {
            FileStatus::Added
        } else if x == 'R' || y == 'R' {
            FileStatus::Renamed
        } else if matches!(x, 'M' | 'T') {
            FileStatus::Staged
        } else {
            FileStatus::Modified
        };

        if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
            while i < status.len() && status[i] != 0 {
                i += 1;
            }
            i = i.saturating_add(1);
        }

        let (additions, deletions) = if matches!(status_kind, FileStatus::Untracked) {
            let line_count = count_lines(&repo_root.join(&path));
            (line_count, 0)
        } else {
            numstat.get(&path).copied().unwrap_or((0, 0))
        };
        files.push(FileChange {
            path,
            status: status_kind,
            additions,
            deletions,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

pub(super) fn count_lines(path: &Path) -> u32 {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    if bytes.is_empty() {
        return 0;
    }
    let mut count = bytes.iter().filter(|b| **b == b'\n').count();
    if !bytes.ends_with(b"\n") {
        count += 1;
    }
    count.min(u32::MAX as usize) as u32
}
