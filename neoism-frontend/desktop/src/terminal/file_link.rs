// Detect clickable file/dir tokens in terminal output and route hover
// + click events to the editor pane.
//
// Scope: lightweight, on-demand. We don't pre-scan every visible row
// each frame — instead, callers invoke `detect_at` with the row text
// + the column the user is pointing at. That's at most one tokenize
// pass + one filesystem stat per mouse-move, which keeps the cost
// bounded even on dense `find` / `rg` output.

use std::path::{Path, PathBuf};

/// One detected clickable token in a terminal row. `byte_start`/`byte_end`
/// are byte offsets into the source row's UTF-8 text and are converted
/// to display columns by the renderer (display_width).
#[derive(Debug, Clone)]
pub struct FileLink {
    /// Absolute row index (`visible_row_absolute_indices`) the link
    /// sits in — used for hover persistence across redraws.
    pub abs_row: usize,
    /// Display-column boundaries (inclusive .. exclusive) of the
    /// underlined region.
    pub col_start: usize,
    pub col_end: usize,
    /// Resolved path the click should open.
    pub path: PathBuf,
}

impl FileLink {
    #[allow(dead_code)]
    pub fn covers(&self, abs_row: usize, col: usize) -> bool {
        self.abs_row == abs_row && col >= self.col_start && col < self.col_end
    }
}

/// Find a clickable file/dir token at column `col` in `row_text`. Walks
/// the row left+right from `col` until a delimiter, then resolves the
/// extracted token relative to `cwd`. Returns `None` if the token
/// doesn't resolve to an existing path.
///
/// `delimiter` chars stop the scan: whitespace, plus shell-special
/// characters that almost never appear inside a path (`'`, `"`, `(`,
/// `)`, `[`, `]`, `<`, `>`, `|`, `*`, `?`, ``;``, `,`).
pub fn detect_at(
    row_text: &str,
    col: usize,
    cwd: Option<&Path>,
    abs_row: usize,
) -> Option<FileLink> {
    let chars: Vec<char> = row_text.chars().collect();
    if col >= chars.len() {
        return None;
    }
    if is_path_delimiter(chars[col]) {
        return None;
    }

    let mut start = col;
    while start > 0 && !is_path_delimiter(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < chars.len() && !is_path_delimiter(chars[end]) {
        end += 1;
    }

    let token: String = chars[start..end].iter().collect();
    let token = token.trim_end_matches(|c: char| matches!(c, ':' | '.' | ','));
    if token.is_empty() {
        return None;
    }

    let resolved = resolve_token(token, cwd)?;

    Some(FileLink {
        abs_row,
        col_start: start,
        col_end: end,
        path: resolved,
    })
}

/// Resolve a token to an existing path on disk. Handles:
///   - Absolute paths (`/foo/bar`)
///   - Home-relative (`~/foo`)
///   - cwd-relative (`./file`, `../file`, `subdir/file`, `file`)
fn resolve_token(token: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let candidate: PathBuf = if token.starts_with('/') {
        PathBuf::from(token)
    } else if let Some(rest) = token.strip_prefix("~/") {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        home.join(rest)
    } else {
        cwd?.join(token)
    };
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

#[inline]
fn is_path_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '\'' | '"' | '(' | ')' | '[' | ']' | '<' | '>' | '|' | '*' | '?' | ';' | ','
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_existing_file_in_cwd() {
        let dir =
            std::env::temp_dir().join(format!("neoism-link-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("example.txt");
        std::fs::write(&target, b"hi").unwrap();

        let row = "ls   example.txt   other";
        let link = detect_at(row, 8, Some(&dir), 42).unwrap();
        assert_eq!(link.abs_row, 42);
        assert_eq!(&row[link.col_start..link.col_end], "example.txt");
        assert_eq!(link.path, target);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn returns_none_for_nonexistent() {
        let dir = std::env::temp_dir();
        let row = "nope.xyz";
        assert!(detect_at(row, 0, Some(&dir), 0).is_none());
    }

    #[test]
    fn handles_absolute_path() {
        let row = "see /tmp for stuff";
        let link = detect_at(row, 5, None, 0);
        assert!(link.is_some(), "expected /tmp to resolve");
    }

    #[test]
    fn covers_check() {
        let link = FileLink {
            abs_row: 5,
            col_start: 10,
            col_end: 14,
            path: PathBuf::from("/tmp"),
        };
        assert!(link.covers(5, 10));
        assert!(link.covers(5, 13));
        assert!(!link.covers(5, 14));
        assert!(!link.covers(5, 9));
        assert!(!link.covers(6, 12));
    }
}
