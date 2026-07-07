//! Smoke tests that exercise the file handlers against a real temp workspace.
//!
//! These call the async handlers directly (no WebSocket) so they don't need
//! to spin up the daemon.
//!
//! IMPORTANT: tests in this file all read `NEOISM_WORKSPACE_ROOT`. Because
//! tests in the same process share env vars, they must run serially. We
//! achieve that by funneling them through a single `#[tokio::test]` that
//! sequences the sub-cases.

use std::fs;
use std::sync::Mutex;

use neoism_protocol::files::{FilesClientMessage, FilesServerMessage};
use neoism_workspace_daemon::files;
use tempfile::TempDir;

/// Guard to serialize tests that touch `NEOISM_WORKSPACE_ROOT`.
static WS_ENV_LOCK: Mutex<()> = Mutex::new(());

struct WorkspaceGuard<'a> {
    _guard: std::sync::MutexGuard<'a, ()>,
    previous: Option<String>,
}

impl<'a> WorkspaceGuard<'a> {
    fn new(dir: &TempDir) -> Self {
        let guard = WS_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var("NEOISM_WORKSPACE_ROOT").ok();
        std::env::set_var("NEOISM_WORKSPACE_ROOT", dir.path());
        Self {
            _guard: guard,
            previous,
        }
    }
}

impl Drop for WorkspaceGuard<'_> {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => std::env::set_var("NEOISM_WORKSPACE_ROOT", v),
            None => std::env::remove_var("NEOISM_WORKSPACE_ROOT"),
        }
    }
}

fn make_workspace() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("a.txt"), b"alpha\n").expect("write a.txt");
    fs::create_dir_all(dir.path().join("dir")).expect("mkdir dir");
    fs::write(dir.path().join("dir/b.txt"), b"bravo").expect("write dir/b.txt");
    dir
}

#[tokio::test]
async fn files_handlers_smoke() {
    let dir = make_workspace();
    let _g = WorkspaceGuard::new(&dir);

    // ListDir of the workspace root.
    let resp = files::handle(FilesClientMessage::ListDir { path: "".into() }).await;
    match resp.first() {
        Some(FilesServerMessage::DirListing { path, entries }) => {
            assert_eq!(path, "");
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"a.txt"), "missing a.txt: {names:?}");
            assert!(names.contains(&"dir"), "missing dir: {names:?}");
            let a = entries.iter().find(|e| e.name == "a.txt").unwrap();
            assert!(!a.is_dir);
            assert_eq!(a.size, Some(6));
            let d = entries.iter().find(|e| e.name == "dir").unwrap();
            assert!(d.is_dir);
        }
        other => panic!("expected DirListing, got {other:?}"),
    }

    // ListDir of a subdir.
    let resp = files::handle(FilesClientMessage::ListDir { path: "dir".into() }).await;
    match resp.first() {
        Some(FilesServerMessage::DirListing { entries, .. }) => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "b.txt");
        }
        other => panic!("expected DirListing, got {other:?}"),
    }

    // ReadFile of an existing file.
    let resp = files::handle(FilesClientMessage::ReadFile {
        path: "a.txt".into(),
    })
    .await;
    match resp.first() {
        Some(FilesServerMessage::FileContent { path, bytes }) => {
            assert_eq!(path, "a.txt");
            assert_eq!(bytes, b"alpha\n");
        }
        other => panic!("expected FileContent, got {other:?}"),
    }

    // Stat returns real daemon-host metadata for file panels and tree code.
    let resp = files::handle(FilesClientMessage::Stat { path: "dir".into() }).await;
    match resp.first() {
        Some(FilesServerMessage::Stat { path, entry }) => {
            assert_eq!(path, "dir");
            assert_eq!(entry.name, "dir");
            assert!(entry.is_dir);
            assert_eq!(entry.size, None);
        }
        other => panic!("expected Stat, got {other:?}"),
    }

    // ReadFile of a nested file.
    let resp = files::handle(FilesClientMessage::ReadFile {
        path: "dir/b.txt".into(),
    })
    .await;
    match resp.first() {
        Some(FilesServerMessage::FileContent { bytes, .. }) => {
            assert_eq!(bytes, b"bravo");
        }
        other => panic!("expected FileContent, got {other:?}"),
    }

    // WriteFile creates new files including parent dirs.
    let resp = files::handle(FilesClientMessage::WriteFile {
        path: "new/c.txt".into(),
        bytes: b"charlie".to_vec(),
    })
    .await;
    match resp.first() {
        Some(FilesServerMessage::FileWritten {
            path,
            bytes_written,
        }) => {
            assert_eq!(path, "new/c.txt");
            assert_eq!(*bytes_written, 7);
        }
        other => panic!("expected FileWritten, got {other:?}"),
    }
    assert_eq!(fs::read(dir.path().join("new/c.txt")).unwrap(), b"charlie");

    // WalkTree returns every entry.
    let resp = files::handle(FilesClientMessage::WalkTree {
        path: "".into(),
        max_depth: None,
    })
    .await;
    match resp.first() {
        Some(FilesServerMessage::TreeListing { entries, .. }) => {
            let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
            assert!(paths.contains(&"a.txt"), "tree missing a.txt: {paths:?}");
            assert!(paths.iter().any(|p| p.ends_with("b.txt")));
            assert!(paths.iter().any(|p| p.ends_with("c.txt")));
            let dir_entry = entries.iter().find(|e| e.path == "dir").unwrap();
            assert!(dir_entry.is_dir);
        }
        other => panic!("expected TreeListing, got {other:?}"),
    }

    // WalkTree with max_depth=1 doesn't descend.
    let resp = files::handle(FilesClientMessage::WalkTree {
        path: "".into(),
        max_depth: Some(1),
    })
    .await;
    match resp.first() {
        Some(FilesServerMessage::TreeListing { entries, .. }) => {
            for e in entries {
                assert_eq!(e.depth, 1, "depth>1 entry leaked: {}", e.path);
            }
        }
        other => panic!("expected TreeListing, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_absolute_path() {
    let dir = make_workspace();
    let _g = WorkspaceGuard::new(&dir);

    let resp = files::handle(FilesClientMessage::ReadFile {
        path: "/etc/passwd".into(),
    })
    .await;
    match resp.first() {
        Some(FilesServerMessage::Error { message }) => {
            assert!(
                message.contains("absolute"),
                "expected absolute-path error, got: {message}"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_parent_traversal() {
    let dir = make_workspace();
    let _g = WorkspaceGuard::new(&dir);

    let cases = ["../etc/passwd", "dir/../../escape", ".."];
    for path in cases {
        let resp = files::handle(FilesClientMessage::ReadFile {
            path: path.to_string(),
        })
        .await;
        match resp.first() {
            Some(FilesServerMessage::Error { message }) => {
                assert!(
                    message.contains(".."),
                    "expected traversal error for {path}, got: {message}"
                );
            }
            other => panic!("expected Error for {path}, got {other:?}"),
        }
    }
}
