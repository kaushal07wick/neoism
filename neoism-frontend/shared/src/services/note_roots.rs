//! Vault note-root traversal helpers shared by native + web.
//!
//! Notes live in configured vault directories, not workspace-local
//! `notes/` folders. Callers pass already-resolved vault roots.

use std::path::{Path, PathBuf};

/// Tells `collect_markdown_note_paths` whether a path is a markdown
/// file. Native passes a closure wrapping
/// `crate::editor::markdown::state::is_markdown_path`; web can pass
/// its own per-extension detector.
pub trait IsMarkdownPath {
    fn is_markdown(&self, path: &Path) -> bool;
}

impl<F: Fn(&Path) -> bool> IsMarkdownPath for F {
    fn is_markdown(&self, path: &Path) -> bool {
        (self)(path)
    }
}

/// `true` when `path` overlaps any entry in `note_roots` - either
/// descending into a root, or being an ancestor of a root.
pub fn intersects_note_roots(path: &Path, note_roots: &[PathBuf]) -> bool {
    note_roots
        .iter()
        .any(|root| path.starts_with(root) || root.starts_with(path))
}

/// Walk `path` recursively and push every markdown file (per
/// `is_markdown`) that lives inside any of `note_roots` into `out`.
/// Skips directories that don't intersect any root.
pub fn collect_markdown_note_paths<M: IsMarkdownPath>(
    path: &Path,
    note_roots: &[PathBuf],
    is_markdown: &M,
    out: &mut Vec<PathBuf>,
) {
    if path.is_file() {
        if is_markdown.is_markdown(path)
            && note_roots.iter().any(|root| path.starts_with(root))
        {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            if intersects_note_roots(&child, note_roots) {
                collect_markdown_note_paths(&child, note_roots, is_markdown, out);
            }
        } else if is_markdown.is_markdown(&child)
            && note_roots.iter().any(|root| child.starts_with(root))
        {
            out.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn is_md(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("md")
    }

    #[test]
    fn collect_markdown_only_inside_roots() {
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("Neoism/Vaults/Personal");
        let other = dir.path().join("src");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(vault.join("A.md"), "# A").unwrap();
        std::fs::write(vault.join("A.txt"), "A").unwrap();
        std::fs::write(other.join("B.md"), "# B").unwrap();

        let mut out = Vec::new();
        collect_markdown_note_paths(
            dir.path(),
            std::slice::from_ref(&vault),
            &is_md,
            &mut out,
        );
        assert_eq!(out, vec![vault.join("A.md")]);
    }

    #[test]
    fn intersects_roots_both_directions() {
        let roots = vec![PathBuf::from("/vault")];
        assert!(intersects_note_roots(&PathBuf::from("/vault/x"), &roots));
        assert!(intersects_note_roots(&PathBuf::from("/"), &roots));
        assert!(!intersects_note_roots(&PathBuf::from("/other"), &roots));
    }
}
