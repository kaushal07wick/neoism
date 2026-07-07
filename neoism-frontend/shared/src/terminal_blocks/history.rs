use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum PersistentHistory {
    Neoism(PathBuf),
    Zsh(PathBuf),
}

impl PersistentHistory {
    pub fn path(&self) -> &Path {
        match self {
            PersistentHistory::Neoism(path) | PersistentHistory::Zsh(path) => path,
        }
    }
}
