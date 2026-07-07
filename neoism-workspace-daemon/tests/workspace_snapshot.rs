//! Integration tests for the portable workspace-snapshot primitive.
//!
//! These live in `tests/` (not `--lib`) because the daemon's unit tests are
//! currently broken by an unrelated in-flight rename; the integration target
//! compiles against the green production lib.
//!
//! Each test builds a real temp git repo via the `git` CLI (skipping if `git`
//! is unavailable, matching the existing provision tests), captures a
//! snapshot, replays it onto a second clone at the same base, and asserts the
//! working tree converged.

use std::path::Path;
use std::process::Command;

use neoism_workspace_daemon::workspace_snapshot::{
    apply_snapshot, capture_uncommitted, WorkspaceSnapshot,
};
use tempfile::TempDir;

/// Run `git -C <dir> <args...>`, panicking on failure with stderr attached.
fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// True when a usable `git` is on PATH; tests no-op otherwise.
fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Initialise a repo with deterministic identity + one committed file.
fn init_repo(dir: &Path, file: &str, contents: &str) {
    git(dir, &["init"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Neoism Test"]);
    std::fs::write(dir.join(file), contents).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", "initial"]);
}

/// Read a working-tree file as a UTF-8 string.
fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// Clone `src` to a fresh temp dir at the same base commit, with the working
/// tree reset to HEAD (i.e. *without* the source's uncommitted changes) — the
/// state a freshly `provision_from_git`'d target would be in.
fn clone_at_base(src: &Path) -> TempDir {
    let dst = TempDir::new().unwrap();
    let out = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(src)
        .arg(dst.path())
        .output()
        .expect("spawn git clone");
    assert!(
        out.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dst
}

#[test]
fn capture_and_apply_round_trips_tracked_and_untracked() {
    if !git_available() {
        eprintln!("git unavailable; skipping");
        return;
    }

    // --- source: commit a file, then make uncommitted changes ---
    let src = TempDir::new().unwrap();
    init_repo(src.path(), "tracked.txt", "line1\nline2\nline3\n");

    // Tracked modification.
    std::fs::write(src.path().join("tracked.txt"), "line1\nCHANGED\nline3\n").unwrap();
    // Untracked, non-ignored file (incl. a nested path to exercise mkdir).
    std::fs::write(src.path().join("new.txt"), "brand new\n").unwrap();
    std::fs::create_dir_all(src.path().join("nested")).unwrap();
    std::fs::write(src.path().join("nested/deep.txt"), "deep\n").unwrap();
    // Untracked but ignored — must NOT travel.
    std::fs::write(src.path().join(".gitignore"), "secret.key\n").unwrap();
    std::fs::write(src.path().join("secret.key"), "SHOULD NOT SHIP\n").unwrap();
    // Commit the .gitignore so it is tracked (else it'd be an untracked file
    // that ships — fine either way, but keep the assertion crisp).
    git(src.path(), &["add", ".gitignore"]);
    git(src.path(), &["commit", "-m", "add gitignore"]);

    // --- capture ---
    let snapshot = capture_uncommitted(src.path()).expect("capture");
    assert!(
        snapshot.base_commit.is_some(),
        "expected a base commit for a repo with history"
    );
    assert!(
        !snapshot.tracked_patch.is_empty(),
        "tracked patch should be non-empty"
    );
    // Untracked must include new.txt + nested/deep.txt, and exclude the
    // gitignored secret.key.
    let untracked_paths: Vec<String> = snapshot
        .untracked
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        untracked_paths.iter().any(|p| p == "new.txt"),
        "new.txt missing from {untracked_paths:?}"
    );
    assert!(
        untracked_paths.iter().any(|p| p == "nested/deep.txt"),
        "nested/deep.txt missing from {untracked_paths:?}"
    );
    assert!(
        !untracked_paths.iter().any(|p| p.contains("secret.key")),
        "gitignored secret.key leaked into {untracked_paths:?}"
    );

    // --- apply onto a fresh clone at the same base ---
    let dst = clone_at_base(src.path());
    // Sanity: the clone is at base (no uncommitted changes yet).
    assert_eq!(read(dst.path(), "tracked.txt"), "line1\nline2\nline3\n");

    let report = apply_snapshot(dst.path(), &snapshot);

    assert!(
        report.is_clean(),
        "expected clean apply, got failures: {:?}",
        report.failed_hunks
    );
    // Tracked patch landed.
    assert_eq!(read(dst.path(), "tracked.txt"), "line1\nCHANGED\nline3\n");
    // Untracked files written.
    assert_eq!(read(dst.path(), "new.txt"), "brand new\n");
    assert_eq!(read(dst.path(), "nested/deep.txt"), "deep\n");
    // Ignored file must NOT exist on the target.
    assert!(
        !dst.path().join("secret.key").exists(),
        "secret.key should not have been shipped"
    );
    // Report bookkeeping.
    assert!(report
        .applied_files
        .iter()
        .any(|p| p.to_string_lossy() == "tracked.txt"));
    assert_eq!(report.wrote_untracked.len(), 2);
}

#[test]
fn partial_apply_reports_failed_hunks_and_keeps_going() {
    if !git_available() {
        eprintln!("git unavailable; skipping");
        return;
    }

    // Source: two tracked files, both modified.
    let src = TempDir::new().unwrap();
    git(src.path(), &["init"]);
    git(src.path(), &["config", "user.email", "test@example.com"]);
    git(src.path(), &["config", "user.name", "Neoism Test"]);
    std::fs::write(src.path().join("fileA.txt"), "a1\na2\na3\n").unwrap();
    std::fs::write(src.path().join("fileB.txt"), "b1\nb2\nb3\n").unwrap();
    git(src.path(), &["add", "fileA.txt", "fileB.txt"]);
    git(src.path(), &["commit", "-m", "init"]);
    std::fs::write(src.path().join("fileA.txt"), "a1\nA-CHANGED\na3\n").unwrap();
    std::fs::write(src.path().join("fileB.txt"), "b1\nB-CHANGED\nb3\n").unwrap();
    std::fs::write(src.path().join("extra.txt"), "extra\n").unwrap();

    let snapshot = capture_uncommitted(src.path()).expect("capture");

    // Target: clone at base, then diverge fileA so its hunk context no longer
    // matches (fileB stays at base and will apply cleanly).
    let dst = clone_at_base(src.path());
    std::fs::write(dst.path().join("fileA.txt"), "X1\nX2\nX3\n").unwrap();

    let report = apply_snapshot(dst.path(), &snapshot);

    // fileB applied cleanly.
    assert_eq!(read(dst.path(), "fileB.txt"), "b1\nB-CHANGED\nb3\n");
    // fileA's hunk was rejected — fileA keeps the diverged content.
    assert_eq!(read(dst.path(), "fileA.txt"), "X1\nX2\nX3\n");
    // The failure was reported (not silently swallowed, not aborting fileB).
    assert!(
        !report.is_clean(),
        "expected a failed hunk, report was clean"
    );
    assert!(
        report
            .failed_hunks
            .iter()
            .any(|(p, _)| p.to_string_lossy() == "fileA.txt"),
        "fileA.txt should be in failed_hunks: {:?}",
        report.failed_hunks
    );
    // A .rej file was left behind, mirroring manual `git apply --reject`.
    assert!(
        dst.path().join("fileA.txt.rej").exists(),
        "expected a .rej file for fileA.txt"
    );
    // Untracked files still get written despite the tracked failure.
    assert_eq!(read(dst.path(), "extra.txt"), "extra\n");
    assert!(report
        .wrote_untracked
        .iter()
        .any(|p| p.to_string_lossy() == "extra.txt"));
}

#[test]
fn empty_working_tree_yields_empty_snapshot() {
    if !git_available() {
        eprintln!("git unavailable; skipping");
        return;
    }
    let src = TempDir::new().unwrap();
    init_repo(src.path(), "only.txt", "stable\n");

    let snapshot = capture_uncommitted(src.path()).expect("capture");
    assert!(snapshot.is_empty(), "clean repo should snapshot to empty");
    assert_eq!(snapshot.tracked_patch, "");
    assert!(snapshot.untracked.is_empty());

    // Applying an empty snapshot is a no-op that reports nothing.
    let dst = clone_at_base(src.path());
    let report = apply_snapshot(dst.path(), &snapshot);
    assert!(report.is_clean());
    assert!(report.applied_files.is_empty());
    assert!(report.wrote_untracked.is_empty());
}

#[test]
fn snapshot_is_serializable_round_trip() {
    // The snapshot must survive a JSON round trip (it ships over the daemon ws
    // transport). Untracked bytes are exercised via serde's default Vec<u8>
    // encoding.
    let snapshot = WorkspaceSnapshot {
        tracked_patch: "diff --git a/x b/x\n".to_string(),
        untracked: vec![(std::path::PathBuf::from("a/b.txt"), vec![1, 2, 3, 0, 255])],
        base_commit: Some("deadbeef".to_string()),
    };
    let json = serde_json::to_string(&snapshot).expect("serialize");
    let back: WorkspaceSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.tracked_patch, snapshot.tracked_patch);
    assert_eq!(back.untracked, snapshot.untracked);
    assert_eq!(back.base_commit, snapshot.base_commit);
}
