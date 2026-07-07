//! Standalone vault ⇄ reMarkable sync — no UI state, so the same logic
//! powers the right-click action **and** the background auto-sync thread.
//!
//! Push only changed notes, delete removed ones, pull handwriting back.
//! Pure SSH + filesystem (auth via the installed key), safe to call off
//! the UI thread.

use std::path::Path;
use std::process::{Command, Stdio};

use neoism_sync::{
    folder_bundle, markdown_to_pdf, pdf_document_bundle, plan_sync, remarkable,
    stable_uuid, LocalNote, SyncManifest, SyncOp, PAGE_HEIGHT,
};

use crate::ink_interop::scene_from_strokes;

/// BatchMode + a short timeout so an asleep/absent tablet fails fast
/// instead of hanging on a password prompt we can't answer.
/// `UserKnownHostsFile=/dev/null` is deliberate: the tablet re-keys on
/// firmware updates / resets, and a stale `known_hosts` entry makes SSH
/// refuse with "HOST IDENTIFICATION HAS CHANGED" — which silently broke
/// sync. It's a local USB device, so we don't pin its key.
const SSH_OPTS: [&str; 8] = [
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=8",
    "-o",
    "StrictHostKeyChecking=no",
    "-o",
    "UserKnownHostsFile=/dev/null",
];

/// Result of one vault sync, for surfacing a notification.
#[derive(Clone, Debug, Default)]
pub struct SyncOutcome {
    pub vault: String,
    pub pushed: usize,
    pub deleted: usize,
    pub pulled: usize,
    pub ok: bool,
    pub error: Option<String>,
    /// Tablet wasn't reachable over SSH. The background poller skips these
    /// silently; an explicit force-sync turns it into a clear message.
    pub unreachable: bool,
}

impl SyncOutcome {
    /// True when anything actually changed — lets auto-sync stay quiet on
    /// no-op runs.
    pub fn changed_anything(&self) -> bool {
        self.pushed + self.deleted + self.pulled > 0
    }
}

/// The default device address (USB). Override with `NEOISM_RM_HOST`.
pub fn default_host() -> String {
    std::env::var("NEOISM_RM_HOST").unwrap_or_else(|_| "root@10.11.99.1".to_string())
}

/// Sync one vault both ways. Returns counts (and any error).
pub fn sync_vault(vault_dir: &Path, vault_name: &str, host: &str) -> SyncOutcome {
    let mut out = SyncOutcome {
        vault: vault_name.to_string(),
        ..Default::default()
    };
    if !vault_dir.is_dir() {
        out.error = Some(format!("vault '{vault_name}' not found"));
        return out;
    }

    // Tablet not reachable? Skip silently — no scp/ssh, no stderr spam in
    // the terminal, no scary notification on every background poll. `host`
    // is `user@addr`; the reachability probe wants just the address.
    if !neoism_sync::is_remarkable_reachable(host, std::time::Duration::from_millis(1200))
    {
        out.unreachable = true;
        out.ok = false;
        return out;
    }

    let folder_id = |p: &Path| stable_uuid(&format!("neoism-folder:{}", p.display()));

    // Always (re)assert the folder tree — cheap + idempotent.
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    files.extend(folder_bundle(&folder_id(vault_dir), vault_name, "").files);

    // Mirror sub-folders, collect notes + their parent folder.
    let mut notes: Vec<(LocalNote, String)> = Vec::new();
    let mut stack = vec![vault_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let parent = folder_id(&dir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Folder")
                    .to_string();
                files.extend(folder_bundle(&folder_id(&path), &name, &parent).files);
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let Ok(md) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let title = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Note")
                    .to_string();
                notes.push((
                    LocalNote {
                        path: path.display().to_string(),
                        title,
                        markdown: md,
                    },
                    parent.clone(),
                ));
            }
        }
    }

    // Diff against the manifest: push new/changed, delete removed.
    let manifest_path = vault_dir.join(".neoism-remarkable.json");
    let manifest = std::fs::read_to_string(&manifest_path)
        .map(|s| SyncManifest::from_json(&s))
        .unwrap_or_default();
    let local: Vec<LocalNote> = notes.iter().map(|(n, _)| n.clone()).collect();
    let parent_of: std::collections::HashMap<&str, &str> = notes
        .iter()
        .map(|(n, p)| (n.path.as_str(), p.as_str()))
        .collect();
    let (ops, next_manifest) = plan_sync(&local, &manifest);

    let mut deletes: Vec<String> = Vec::new();
    for op in &ops {
        match op {
            SyncOp::Push {
                path,
                title,
                device_uuid,
            } => {
                let Ok(md) = std::fs::read_to_string(path) else {
                    continue;
                };
                let pdf = markdown_to_pdf(&md);
                let parent = parent_of.get(path.as_str()).copied().unwrap_or("");
                files.extend(
                    pdf_document_bundle(
                        device_uuid,
                        title,
                        parent,
                        &pdf.bytes,
                        pdf.page_count,
                    )
                    .files,
                );
                out.pushed += 1;
            }
            SyncOp::Delete { device_uuid, .. } => deletes.push(device_uuid.clone()),
        }
    }

    // Stage + scp the push set into xochitl's store.
    let staging = std::env::temp_dir().join(format!("neoism-rm-{vault_name}"));
    let _ = std::fs::remove_dir_all(&staging);
    if let Err(err) = std::fs::create_dir_all(&staging) {
        out.error = Some(format!("could not stage bundle: {err}"));
        return out;
    }
    for (name, bytes) in &files {
        let _ = std::fs::write(staging.join(name), bytes);
    }
    let scp_ok = Command::new("scp")
        .arg("-r")
        .args(SSH_OPTS)
        .arg(format!("{}/.", staging.display()))
        .arg(format!("{host}:.local/share/remarkable/xochitl/"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_dir_all(&staging);
    if !scp_ok {
        out.error = Some(format!(
            "scp to {host} failed — is the tablet connected over SSH?"
        ));
        return out;
    }

    // Apply deletions, then make xochitl pick up the changes.
    for uuid in &deletes {
        let _ = Command::new("ssh")
            .args(SSH_OPTS)
            .arg(host)
            .arg(format!("rm -rf .local/share/remarkable/xochitl/{uuid}*"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        out.deleted += 1;
    }
    // Only nudge xochitl when something actually changed. Restarting it on
    // every (often no-op) sync "blinks" the tablet UI and can corrupt a doc
    // if it lands mid-write — which is exactly what was happening.
    if out.pushed > 0 || out.deleted > 0 {
        let _ = Command::new("ssh")
            .args(SSH_OPTS)
            .arg(host)
            .arg("systemctl restart xochitl")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    // Pull handwriting back so the note matches both ways.
    out.pulled = pull_vault_ink(vault_dir, host);
    // Persist sync state so the next run only touches changes.
    let _ = std::fs::write(&manifest_path, next_manifest.to_json());
    out.ok = true;
    out
}

/// Pull each note's `.rm` pages from the device, parse (v6), and write a
/// `"<note> (reMarkable).neodraw"` next to the note. Returns how many notes
/// had handwriting.
fn pull_vault_ink(vault_dir: &Path, host: &str) -> usize {
    let mut pulled = 0usize;
    let mut stack = vec![vault_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let note = entry.path();
            if note.is_dir() {
                stack.push(note);
                continue;
            }
            if note.extension().and_then(|x| x.to_str()) != Some("md") {
                continue;
            }
            let uuid = stable_uuid(&format!("neoism-note:{}", note.display()));
            let tmp = std::env::temp_dir().join(format!("neoism-pull-{uuid}"));
            let _ = std::fs::remove_dir_all(&tmp);
            if std::fs::create_dir_all(&tmp).is_err() {
                continue;
            }
            let ok = Command::new("scp")
                .arg("-r")
                .args(SSH_OPTS)
                .arg(format!("{host}:.local/share/remarkable/xochitl/{uuid}"))
                .arg(&tmp)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                let pages_dir = tmp.join(&uuid);
                let mut rm_files: Vec<_> = std::fs::read_dir(&pages_dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rm"))
                    .collect();
                rm_files.sort();
                let mut strokes = Vec::new();
                for (i, rm) in rm_files.iter().enumerate() {
                    let Ok(bytes) = std::fs::read(rm) else {
                        continue;
                    };
                    if let Ok(mut page) = remarkable::parse_rm(&bytes) {
                        let offset = i as f32 * PAGE_HEIGHT;
                        for s in &mut page {
                            for pt in &mut s.points {
                                pt.y += offset;
                            }
                        }
                        strokes.extend(page);
                    }
                }
                if !strokes.is_empty() {
                    let scene = scene_from_strokes(&strokes);
                    let _ = std::fs::write(
                        neoism_ui::editor::neodraw::ink_sidecar_path(&note),
                        scene.to_json(),
                    );
                    pulled += 1;
                }
            }
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
    pulled
}
