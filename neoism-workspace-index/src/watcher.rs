use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

use crate::link_repair::is_markdown_event_path;
use crate::query::NoteGraph;

#[derive(Debug)]
pub struct NoteGraphWatcher {
    shutdown: mpsc::Sender<()>,
    handle: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WatcherEvent {
    pub path: String,
    pub action: String,
}

impl NoteGraphWatcher {
    pub fn start(graph: NoteGraph) -> std::io::Result<Self> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("neoism-note-graph-watch".to_string())
            .spawn(move || {
                run_watcher(graph, shutdown_rx);
            })
            .map_err(std::io::Error::other)?;
        Ok(Self {
            shutdown: shutdown_tx,
            handle: Some(handle),
        })
    }

    pub fn stop(mut self) {
        let _ = self.shutdown.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for NoteGraphWatcher {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}

fn run_watcher(graph: NoteGraph, shutdown_rx: mpsc::Receiver<()>) {
    let (event_tx, event_rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(event_tx, notify::Config::default()) {
        Ok(watcher) => watcher,
        Err(err) => {
            eprintln!("neoism notes watcher failed to start: {err}");
            return;
        }
    };

    for root in note_watch_roots(&graph) {
        let mode = if root.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        if let Err(err) = watcher.watch(&root, mode) {
            eprintln!(
                "neoism notes watcher could not watch {}: {err}",
                root.display()
            );
        }
    }

    loop {
        if shutdown_rx.try_recv().is_ok() {
            return;
        }
        match event_rx.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(Ok(event)) => handle_event(&graph, event),
            Ok(Err(err)) => eprintln!("neoism notes watcher error: {err}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn handle_event(graph: &NoteGraph, event: Event) {
    if event.need_rescan() {
        let _ = graph.reindex();
        return;
    }
    if is_rename_event(&event) && event.paths.len() >= 2 {
        let old = &event.paths[0];
        let new = &event.paths[1];
        if is_markdown_event_path(old) || is_markdown_event_path(new) {
            let _ = graph.repair_moved_note(old, new);
        }
        return;
    }
    for path in event.paths {
        if !is_markdown_event_path(&path) {
            continue;
        }
        match event.kind {
            EventKind::Remove(_) => {
                let _ = graph.remove_file(&path);
            }
            EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Any
            | EventKind::Other => {
                let _ = graph.replace_file(&path);
            }
            EventKind::Access(_) => {}
        }
    }
}

fn is_rename_event(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Modify(notify::event::ModifyKind::Name(_))
    )
}

fn note_watch_roots(graph: &NoteGraph) -> Vec<PathBuf> {
    graph
        .workspace()
        .note_roots()
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}
