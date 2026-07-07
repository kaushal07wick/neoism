use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static FILE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> =
    OnceLock::new();

pub(super) struct FileLockGuards {
    _guards: Vec<OwnedMutexGuard<()>>,
}

pub(super) async fn lock_file(path: &Path) -> FileLockGuards {
    lock_files([path.to_path_buf()]).await
}

pub(super) async fn lock_files(
    paths: impl IntoIterator<Item = PathBuf>,
) -> FileLockGuards {
    let mut keys = paths.into_iter().map(lock_key).collect::<Vec<_>>();
    keys.sort();
    keys.dedup();

    let mut guards = Vec::with_capacity(keys.len());
    for key in keys {
        tracing::info!(path = %key.display(), "edit file lock waiting");
        let lock = file_lock(key);
        guards.push(lock.lock_owned().await);
        tracing::info!("edit file lock acquired");
    }
    FileLockGuards { _guards: guards }
}

fn file_lock(key: PathBuf) -> Arc<AsyncMutex<()>> {
    let registry = FILE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry.lock().expect("file lock registry poisoned");
    registry
        .entry(key)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

fn lock_key(path: PathBuf) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let Some(parent) = path.parent() else {
        return path;
    };
    let parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    match path.file_name() {
        Some(name) => parent.join(name),
        None => parent,
    }
}
