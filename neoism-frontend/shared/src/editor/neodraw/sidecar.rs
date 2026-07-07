//! The hidden ink-layer sidecar for a markdown note.
//!
//! Draw-over-markdown stores its strokes in a dotfile next to the note so
//! it stays out of the file tree. These helpers live in the shared crate
//! (not the reMarkable plugin) because the *core* draw-over-markdown
//! feature owns them — the reMarkable sync just reads/writes the same file.

use super::scene::{Scene, ShapeKind};
use std::path::{Path, PathBuf};

/// The hidden ink-layer sidecar for a note (`.<stem>.ink.neodraw`). A
/// dotfile so it stays out of the file tree / vault view, and one place so
/// the overlay, draw mode, and any device pull all agree.
pub fn ink_sidecar_path(note: &Path) -> PathBuf {
    let stem = note.file_stem().and_then(|s| s.to_str()).unwrap_or("note");
    note.with_file_name(format!(".{stem}.ink.neodraw"))
}

/// The old *visible* sidecar name (pre-dotfile, possibly with baked text).
/// Used to migrate existing drawings into the hidden, strokes-only file.
pub fn legacy_ink_sidecar_path(note: &Path) -> PathBuf {
    let stem = note.file_stem().and_then(|s| s.to_str()).unwrap_or("note");
    note.with_file_name(format!("{stem} (reMarkable).neodraw"))
}

/// The ink layer is strokes only — never baked text (which belongs to the
/// markdown renderer). Drop any text shapes.
pub fn strokes_only(scene: &mut Scene) {
    scene
        .shapes
        .retain(|s| !matches!(s.kind, ShapeKind::Text { .. }));
}

/// One-time migration: move an old visible sidecar into the hidden,
/// strokes-only file and delete the visible one.
pub fn migrate_legacy_ink(note: &Path) {
    let hidden = ink_sidecar_path(note);
    if hidden.exists() {
        return;
    }
    let legacy = legacy_ink_sidecar_path(note);
    if !legacy.exists() {
        return;
    }
    if let Some(mut scene) = std::fs::read_to_string(&legacy)
        .ok()
        .and_then(|j| Scene::from_json(&j).ok())
    {
        strokes_only(&mut scene);
        let _ = std::fs::write(&hidden, scene.to_json());
    }
    let _ = std::fs::remove_file(&legacy);
}

/// Load a note's ink layer (hidden sidecar, strokes only), migrating any
/// old visible file first. Empty scene if none.
pub fn load_ink_layer(note: &Path) -> Scene {
    migrate_legacy_ink(note);
    std::fs::read_to_string(ink_sidecar_path(note))
        .ok()
        .and_then(|j| Scene::from_json(&j).ok())
        .map(|mut s| {
            strokes_only(&mut s);
            s
        })
        .unwrap_or_default()
}
