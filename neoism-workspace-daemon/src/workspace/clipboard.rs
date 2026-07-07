//! Clipboard tempdir IO: directory resolution, the startup + per-paste
//! GC sweeps, and clipboard-image materialisation. Pure code-move out of
//! the former monolithic `workspace.rs`.

use super::*;

/// Subdirectory under `TMPDIR` that owns every clipboard image the
/// daemon materialises. Centralised so the GC, the HTTP route, and
/// the materialiser agree on a single source of truth.
const CLIPBOARD_DIR_NAME: &str = "neoism-clipboard";

/// Soft cap on the number of materialised images we keep in the
/// clipboard tempdir before the per-paste opportunistic sweep starts
/// evicting the oldest entries. Picked so a heavy pasting session
/// (~50 screenshots/hour) coexists comfortably with the 24h TTL
/// without unbounded growth.
const CLIPBOARD_MAX_FILES: usize = 100;

/// Maximum age, in seconds, that a materialised image is allowed to
/// linger in the clipboard tempdir before the startup sweep deletes
/// it. 24h matches the user-visible "I pasted this yesterday" window
/// without leaking screenshots indefinitely.
const CLIPBOARD_TTL_SECS: u64 = 24 * 60 * 60;

/// Absolute path of the daemon's clipboard tempdir. Exposed so the
/// axum HTTP layer can serve `/clipboard-image/<filename>` from the
/// same directory `materialize_clipboard_image` writes to.
pub fn clipboard_dir() -> PathBuf {
    std::env::temp_dir().join(CLIPBOARD_DIR_NAME)
}

/// Resolve a request `filename` against the clipboard tempdir,
/// rejecting anything that isn't a `paste-*` entry of the expected
/// shape. Returns `None` for traversal attempts, hidden files, or
/// names with path separators.
pub fn resolve_clipboard_image_path(filename: &str) -> Option<PathBuf> {
    if filename.is_empty() || filename.contains('/') || filename.contains('\\') {
        return None;
    }
    if filename.starts_with('.') || filename.contains("..") {
        return None;
    }
    if !filename.starts_with("paste-") {
        return None;
    }
    let path = clipboard_dir().join(filename);
    if path.parent() != Some(clipboard_dir().as_path()) {
        return None;
    }
    Some(path)
}

/// Sweep the clipboard tempdir on daemon startup: drop entries older
/// than `CLIPBOARD_TTL_SECS` and trim the directory to
/// `CLIPBOARD_MAX_FILES` by evicting the oldest entries. Best-effort
/// — any I/O failure is logged but does not abort daemon boot.
pub fn sweep_clipboard_dir_on_startup() {
    let dir = clipboard_dir();
    if !dir.exists() {
        return;
    }
    let now = SystemTime::now();
    let entries = match collect_clipboard_entries(&dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(error = %e, path = %dir.display(), "clipboard GC: read_dir failed");
            return;
        }
    };

    let mut survivors: Vec<(PathBuf, SystemTime)> = Vec::with_capacity(entries.len());
    let mut ttl_evictions = 0usize;
    for (path, modified) in entries {
        let age = now
            .duration_since(modified)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if age > CLIPBOARD_TTL_SECS {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(error = %e, path = %path.display(), "clipboard GC: remove failed");
            } else {
                ttl_evictions += 1;
            }
        } else {
            survivors.push((path, modified));
        }
    }

    let lru_evictions = trim_to_cap(&mut survivors);
    if ttl_evictions + lru_evictions > 0 {
        tracing::info!(
            ttl_evictions,
            lru_evictions,
            remaining = survivors.len(),
            "clipboard GC: startup sweep complete"
        );
    }
}

/// Opportunistic sweep run after every paste. Only enforces the LRU
/// cap (the TTL is handled at startup) so the per-paste cost stays
/// proportional to the number of *excess* files rather than to the
/// directory's total size.
fn sweep_clipboard_dir_opportunistic() {
    let dir = clipboard_dir();
    let mut entries = match collect_clipboard_entries(&dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    if entries.len() <= CLIPBOARD_MAX_FILES {
        return;
    }
    let evicted = trim_to_cap(&mut entries);
    if evicted > 0 {
        tracing::debug!(
            evicted,
            remaining = entries.len(),
            "clipboard GC: opportunistic LRU sweep"
        );
    }
}

/// Read every regular file in `dir` and pair it with its mtime
/// (falling back to `UNIX_EPOCH` so files without a usable mtime sort
/// to the front of the LRU and are evicted first).
fn collect_clipboard_entries(dir: &Path) -> std::io::Result<Vec<(PathBuf, SystemTime)>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        out.push((path, mtime));
    }
    Ok(out)
}

/// Drop the oldest entries from `entries` until its length is at
/// most `CLIPBOARD_MAX_FILES`. Mutates `entries` in place and returns
/// the number of files actually removed from disk.
fn trim_to_cap(entries: &mut Vec<(PathBuf, SystemTime)>) -> usize {
    if entries.len() <= CLIPBOARD_MAX_FILES {
        return 0;
    }
    entries.sort_by(|a, b| a.1.cmp(&b.1));
    let excess = entries.len() - CLIPBOARD_MAX_FILES;
    let mut removed = 0usize;
    let mut victims = Vec::with_capacity(excess);
    for _ in 0..excess {
        if let Some(victim) = entries.first().cloned() {
            entries.remove(0);
            victims.push(victim);
        }
    }
    for (path, _) in &victims {
        if let Err(e) = std::fs::remove_file(path) {
            tracing::warn!(error = %e, path = %path.display(), "clipboard GC: LRU remove failed");
        } else {
            removed += 1;
        }
    }
    removed
}

/// Write a clipboard image payload to the daemon's temp directory so
/// frontends can drop it into nvim via `:edit <path>`. The temp file
/// lives until the daemon's GC sweeps it (24h TTL at startup, LRU
/// cap on every paste). We deliberately do *not* track the path on
/// the connection so subsequent pastes don't pile up writers.
///
/// `request_id` is opaque to the daemon — we echo it verbatim in the
/// reply so multi-pane frontends can route the materialisation to the
/// pane that initiated the paste.
pub(crate) fn materialize_clipboard_image(
    payload: ClipboardPayload,
    request_id: Option<String>,
) -> Vec<WorkspaceServerMessage> {
    if payload.bytes.is_empty() {
        return vec![err("clipboard image payload is empty".to_string())];
    }
    let dir = clipboard_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return vec![err(format!("temp dir {}: {e}", dir.display()))];
    }
    let ext = match payload.mime_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    };
    let name = format!("paste-{}.{ext}", Uuid::new_v4());
    let path = dir.join(&name);
    if let Err(e) = std::fs::write(&path, &payload.bytes) {
        return vec![err(format!("write {}: {e}", path.display()))];
    }
    // Opportunistic LRU trim: cheap when the directory is already
    // under cap, bounded by the (constant) excess count otherwise.
    sweep_clipboard_dir_opportunistic();
    vec![WorkspaceServerMessage::ClipboardImageMaterialized {
        path,
        mime_type: payload.mime_type,
        filename: payload.filename,
        request_id,
    }]
}
