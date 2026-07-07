//! Robust sync planning — the "golden, not sloppy" core.
//!
//! Instead of blindly re-pushing everything, we keep a small per-vault
//! **manifest** of what was last synced (note path → content hash + device
//! uuid) and diff the current vault against it. That yields a minimal plan:
//! push only new/changed notes, and delete notes that were removed locally.
//! Combined with deterministic [`crate::stable_uuid`]s this means
//! re-syncing is cheap and never duplicates or orphans documents.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// What was last synced for one note.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRecord {
    pub device_uuid: String,
    pub hash: u64,
}

/// Persisted per-vault sync state. Lives as a hidden file in the vault so
/// it travels with the notes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncManifest {
    #[serde(default)]
    pub notes: HashMap<String, SyncRecord>,
}

impl SyncManifest {
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
}

/// A note currently on disk.
#[derive(Clone, Debug)]
pub struct LocalNote {
    pub path: String,
    pub title: String,
    pub markdown: String,
}

/// One planned sync action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncOp {
    /// New or changed note — render + push.
    Push {
        path: String,
        title: String,
        device_uuid: String,
    },
    /// Note removed locally — delete its document on the device.
    Delete { path: String, device_uuid: String },
}

/// FNV-1a hash of a note's content, to detect changes cheaply.
pub fn content_hash(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Diff `local` notes against the last-synced `manifest`. Returns the
/// minimal op list plus the manifest to persist **after** a successful run.
pub fn plan_sync(
    local: &[LocalNote],
    manifest: &SyncManifest,
) -> (Vec<SyncOp>, SyncManifest) {
    let mut ops = Vec::new();
    let mut next = SyncManifest::default();
    let mut seen = HashSet::new();

    for note in local {
        seen.insert(note.path.as_str());
        let device_uuid = crate::stable_uuid(&format!("neoism-note:{}", note.path));
        let hash = content_hash(&note.markdown);
        let changed = manifest
            .notes
            .get(&note.path)
            .map(|r| r.hash != hash)
            .unwrap_or(true);
        if changed {
            ops.push(SyncOp::Push {
                path: note.path.clone(),
                title: note.title.clone(),
                device_uuid: device_uuid.clone(),
            });
        }
        next.notes
            .insert(note.path.clone(), SyncRecord { device_uuid, hash });
    }

    // Anything in the manifest but no longer on disk was deleted locally.
    for (path, rec) in &manifest.notes {
        if !seen.contains(path.as_str()) {
            ops.push(SyncOp::Delete {
                path: path.clone(),
                device_uuid: rec.device_uuid.clone(),
            });
        }
    }

    (ops, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, md: &str) -> LocalNote {
        LocalNote {
            path: path.into(),
            title: path.into(),
            markdown: md.into(),
        }
    }

    #[test]
    fn first_sync_pushes_everything() {
        let local = vec![note("a.md", "x"), note("b.md", "y")];
        let (ops, next) = plan_sync(&local, &SyncManifest::default());
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().all(|o| matches!(o, SyncOp::Push { .. })));
        assert_eq!(next.notes.len(), 2);
    }

    #[test]
    fn unchanged_notes_are_skipped_changed_ones_pushed() {
        let local = vec![note("a.md", "x"), note("b.md", "y")];
        let (_, manifest) = plan_sync(&local, &SyncManifest::default());

        // b changes, a doesn't.
        let local2 = vec![note("a.md", "x"), note("b.md", "Y!")];
        let (ops, _) = plan_sync(&local2, &manifest);
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], SyncOp::Push { path, .. } if path == "b.md"));
    }

    #[test]
    fn removed_notes_are_deleted() {
        let local = vec![note("a.md", "x"), note("b.md", "y")];
        let (_, manifest) = plan_sync(&local, &SyncManifest::default());

        // a deleted locally.
        let local2 = vec![note("b.md", "y")];
        let (ops, next) = plan_sync(&local2, &manifest);
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], SyncOp::Delete { path, .. } if path == "a.md"));
        assert!(!next.notes.contains_key("a.md"));
    }

    #[test]
    fn manifest_roundtrips_json() {
        let local = vec![note("a.md", "x")];
        let (_, m) = plan_sync(&local, &SyncManifest::default());
        let back = SyncManifest::from_json(&m.to_json());
        assert_eq!(back.notes.get("a.md").unwrap().hash, content_hash("x"));
    }
}
