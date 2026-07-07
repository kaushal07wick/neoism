//! Background auto-sync: watch for the reMarkable and keep every vault in
//! sync without the user lifting a finger.
//!
//! A dedicated thread polls [`is_remarkable_reachable`]; when the tablet
//! appears (and periodically while it's there) it runs [`sync_vault`] for
//! each vault and posts the result back over a channel. The UI drains the
//! channel each frame via [`RemarkableAutoSync::drain`] to show a quiet
//! notification. All the heavy work is off the UI thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use neoism_sync::is_remarkable_reachable;

use crate::vault_sync::{default_host, sync_vault, SyncOutcome};

/// How often to check whether the tablet is reachable.
const POLL: Duration = Duration::from_secs(8);
/// Re-sync everything at most this often while connected.
const RESYNC_INTERVAL: Duration = Duration::from_secs(60);
/// Reachability probe timeout — short so a missing tablet doesn't stall.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

pub struct RemarkableAutoSync {
    running: Arc<AtomicBool>,
    rx: Receiver<SyncOutcome>,
    handle: Option<JoinHandle<()>>,
}

impl RemarkableAutoSync {
    /// Spawn the poller. Syncs every vault under the notes root the moment
    /// the tablet appears, and every ~60s while it stays connected.
    pub fn start() -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let (tx, rx) = std::sync::mpsc::channel();
        let flag = running.clone();
        let handle = std::thread::Builder::new()
            .name("neoism-rm-autosync".into())
            .spawn(move || run(flag, tx))
            .ok();
        Self {
            running,
            rx,
            handle,
        }
    }

    /// Collect any sync results since the last call — drain this each frame.
    pub fn drain(&self) -> Vec<SyncOutcome> {
        self.rx.try_iter().collect()
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for RemarkableAutoSync {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run(running: Arc<AtomicBool>, tx: Sender<SyncOutcome>) {
    let host = default_host();
    let mut was_connected = false;
    // Force a sync on the first connection.
    let mut last_sync = Instant::now()
        .checked_sub(RESYNC_INTERVAL)
        .unwrap_or_else(Instant::now);

    while running.load(Ordering::Relaxed) {
        let connected = is_remarkable_reachable(&host, PROBE_TIMEOUT);
        let just_connected = connected && !was_connected;
        let due = connected && last_sync.elapsed() >= RESYNC_INTERVAL;

        if just_connected || due {
            for (dir, name) in vaults() {
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                let outcome = sync_vault(&dir, &name, &host);
                // Stay quiet on no-op runs and on a flapping-link race
                // (unreachable); report real changes and real failures.
                if outcome.unreachable {
                    continue;
                }
                if !outcome.ok || outcome.changed_anything() {
                    let _ = tx.send(outcome);
                }
            }
            last_sync = Instant::now();
        }
        was_connected = connected;

        // Sleep in small steps so stop() stays responsive.
        let mut slept = Duration::ZERO;
        while slept < POLL && running.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(200));
            slept += Duration::from_millis(200);
        }
    }
}

/// Every vault (immediate sub-directory of the notes root) as `(dir, name)`.
fn vaults() -> Vec<(PathBuf, String)> {
    let root = neoism_workspace_index::notes_vaults_dir();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                out.push((path.clone(), name.to_string()));
            }
        }
    }
    out
}
