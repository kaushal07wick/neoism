//! Filesystem watch → live tree pushes for the files plane.
//!
//! A guest browsing a joined workspace lists directories through
//! `FilesClientMessage::ListDir` with a `workspace_root` override.
//! Without a watch, its tree only updates when the guest re-lists —
//! everything else in a shared workspace (shells, CRDT docs) is push
//! driven, and the tree read as frozen next to them. This hub puts a
//! recursive watcher on every files root a client has actually
//! touched and broadcasts debounced `FilesServerMessage::Changed`
//! pushes (request_id 0) that each `/session` socket forwards to its
//! client.
//!
//! Process-global (`hub()`), like the tailnet module: the daemon has
//! several `AppState` construction sites (binary, embedded, tests)
//! and the watch set is genuinely per-process.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use tokio::sync::broadcast;

/// One debounced change burst under a watched root. Both paths are
/// absolute; `paths` is deduped and capped so a huge build doesn't
/// ship a megabyte of paths (clients re-list directories anyway).
#[derive(Debug, Clone)]
pub struct FsChanged {
    pub root: String,
    pub paths: Vec<String>,
}

const DEBOUNCE: Duration = Duration::from_millis(300);
const MAX_PATHS_PER_BURST: usize = 64;
const BROADCAST_CAPACITY: usize = 256;

pub struct FsWatchHub {
    tx: broadcast::Sender<FsChanged>,
    /// Root → live watcher. Keeping the watcher alive keeps the watch.
    watchers: Mutex<HashMap<PathBuf, notify::RecommendedWatcher>>,
    /// Raw (root, path) events from every watcher thread, drained by
    /// the debouncer thread.
    event_tx: std::sync::mpsc::Sender<(PathBuf, PathBuf)>,
}

pub fn hub() -> &'static FsWatchHub {
    static HUB: OnceLock<FsWatchHub> = OnceLock::new();
    HUB.get_or_init(|| {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (event_tx, event_rx) = std::sync::mpsc::channel::<(PathBuf, PathBuf)>();
        let broadcast_tx = tx.clone();
        // Debouncer: collect raw events per root for DEBOUNCE, then
        // flush one Changed burst per root.
        let spawned = std::thread::Builder::new()
            .name("neoism-fs-watch-debounce".into())
            .spawn(move || {
                let mut pending: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
                let mut window_started: Option<Instant> = None;
                loop {
                    let timeout = match window_started {
                        Some(started) => DEBOUNCE
                            .checked_sub(started.elapsed())
                            .unwrap_or(Duration::ZERO),
                        None => Duration::from_secs(3600),
                    };
                    match event_rx.recv_timeout(timeout) {
                        Ok((root, path)) => {
                            let paths = pending.entry(root).or_default();
                            if paths.len() < MAX_PATHS_PER_BURST && !paths.contains(&path)
                            {
                                paths.push(path);
                            }
                            window_started.get_or_insert_with(Instant::now);
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            for (root, paths) in pending.drain() {
                                let _ = broadcast_tx.send(FsChanged {
                                    root: root.to_string_lossy().into_owned(),
                                    paths: paths
                                        .iter()
                                        .map(|p| p.to_string_lossy().into_owned())
                                        .collect(),
                                });
                            }
                            window_started = None;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .is_ok();
        if !spawned {
            tracing::warn!(
                target: "neoism::fs_watch",
                "fs watch debouncer thread failed to spawn; live tree pushes disabled"
            );
        }
        FsWatchHub {
            tx,
            watchers: Mutex::new(HashMap::new()),
            event_tx,
        }
    })
}

impl FsWatchHub {
    pub fn subscribe(&self) -> broadcast::Receiver<FsChanged> {
        self.tx.subscribe()
    }

    /// Idempotently start watching `root` (recursive). Called from the
    /// files handler on every request, so the first `ListDir` a client
    /// sends is what arms liveness for that root.
    pub fn ensure_watched(&self, root: &Path) {
        let Ok(mut watchers) = self.watchers.lock() else {
            return;
        };
        if watchers.contains_key(root) {
            return;
        }
        let event_tx = self.event_tx.clone();
        let event_root = root.to_path_buf();
        let watcher = notify::recommended_watcher(
            move |event: Result<notify::Event, notify::Error>| {
                let Ok(event) = event else { return };
                // Content-only modifications don't change the tree
                // shape, but renames/creates/removes do; send them
                // all and let the debounce + client re-list absorb
                // the noise (a listing is cheap).
                for path in event.paths {
                    let _ = event_tx.send((event_root.clone(), path));
                }
            },
        );
        match watcher {
            Ok(mut watcher) => {
                if let Err(error) = watcher.watch(root, RecursiveMode::Recursive) {
                    tracing::warn!(
                        target: "neoism::fs_watch",
                        %error,
                        root = %root.display(),
                        "fs watch failed; live tree pushes disabled for this root"
                    );
                    return;
                }
                tracing::info!(
                    target: "neoism::fs_watch",
                    root = %root.display(),
                    "watching files root for live tree pushes"
                );
                watchers.insert(root.to_path_buf(), watcher);
            }
            Err(error) => {
                tracing::warn!(
                    target: "neoism::fs_watch",
                    %error,
                    root = %root.display(),
                    "fs watcher construction failed"
                );
            }
        }
    }
}
