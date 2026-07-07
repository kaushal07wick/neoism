// Extensions to the 2.1 surface added here:
//   - InstalledIndex::install_record / remove_record / is_installed / get
//   - InstalledIndex::load / save now perform real I/O (atomic via temp+rename).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::install_runner::InstallError;
use crate::paths;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledEntry {
    pub id: String,
    pub version: String,
    pub install_kind: String,
    pub bin_path: Option<PathBuf>,
    /// Epoch milliseconds at install completion.
    pub installed_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InstalledIndex {
    pub entries: BTreeMap<String, InstalledEntry>,
    #[serde(default)]
    pub disabled_builtins: BTreeSet<String>,
}

impl InstalledIndex {
    /// Load the index from `paths::installed_record_path()`. Returns an
    /// empty index if the file is missing or empty.
    pub fn load() -> Result<Self, InstallError> {
        Self::load_from(&paths::installed_record_path())
    }

    /// Persist the index atomically to `paths::installed_record_path()`.
    pub fn save(&self) -> Result<(), InstallError> {
        self.save_to(&paths::installed_record_path())
    }

    pub fn load_from(path: &Path) -> Result<Self, InstallError> {
        match std::fs::read(path) {
            Ok(bytes) if bytes.is_empty() => Ok(Self::default()),
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| InstallError::ParseManifest(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(InstallError::Io(e)),
        }
    }

    pub fn save_to(&self, path: &Path) -> Result<(), InstallError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialised = serde_json::to_vec_pretty(self)
            .map_err(|e| InstallError::ParseManifest(e.to_string()))?;
        write_atomic(path, &serialised)
    }

    pub fn install_record(&mut self, entry: InstalledEntry) {
        self.disabled_builtins.remove(&entry.id);
        self.entries.insert(entry.id.clone(), entry);
    }

    pub fn remove_record(&mut self, id: &str) -> Option<InstalledEntry> {
        self.entries.remove(id)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    pub fn get(&self, id: &str) -> Option<&InstalledEntry> {
        self.entries.get(id)
    }

    pub fn disable_builtin(&mut self, id: &str) {
        self.entries.remove(id);
        self.disabled_builtins.insert(id.to_string());
    }

    pub fn enable_builtin(&mut self, id: &str) {
        self.disabled_builtins.remove(id);
    }

    pub fn is_builtin_disabled(&self, id: &str) -> bool {
        self.disabled_builtins.contains(id)
    }
}

/// Atomic write: write to a sibling temp file, then rename over the target.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), InstallError> {
    let parent = path.parent().ok_or_else(|| {
        InstallError::ParseManifest(format!("path has no parent: {}", path.display()))
    })?;
    std::fs::create_dir_all(parent)?;
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let tmp_name = format!(".{}.tmp.{}", file_name, std::process::id());
    let tmp_path = parent.join(tmp_name);
    std::fs::write(&tmp_path, bytes)?;
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(InstallError::Io(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(id: &str, version: &str) -> InstalledEntry {
        InstalledEntry {
            id: id.to_string(),
            version: version.to_string(),
            install_kind: "npm".to_string(),
            bin_path: Some(PathBuf::from(format!("/tmp/bin/{}", id))),
            installed_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn round_trip_save_and_load() {
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(sample_entry("server-filesystem", "1.0.0"));
        idx.install_record(sample_entry("server-github", "0.4.1"));
        idx.save_to(&path).unwrap();

        let loaded = InstalledIndex::load_from(&path).unwrap();
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.get("server-filesystem").unwrap().version, "1.0.0");
        assert_eq!(loaded.get("server-github").unwrap().version, "0.4.1");
    }

    #[test]
    fn install_record_overwrites_by_id() {
        let mut idx = InstalledIndex::default();
        idx.install_record(sample_entry("dup", "0.1.0"));
        idx.install_record(sample_entry("dup", "0.2.0"));
        assert_eq!(idx.entries.len(), 1);
        assert_eq!(idx.get("dup").unwrap().version, "0.2.0");
    }

    #[test]
    fn remove_record_returns_entry() {
        let mut idx = InstalledIndex::default();
        idx.install_record(sample_entry("x", "1.0.0"));
        let removed = idx.remove_record("x").unwrap();
        assert_eq!(removed.id, "x");
        assert!(idx.remove_record("x").is_none());
    }

    #[test]
    fn disabled_builtin_survives_round_trip_and_install_reenables() {
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(sample_entry("neoism-memory", "1.0.0"));
        idx.disable_builtin("neoism-memory");
        assert!(idx.get("neoism-memory").is_none());
        assert!(idx.is_builtin_disabled("neoism-memory"));
        idx.save_to(&path).unwrap();

        let mut loaded = InstalledIndex::load_from(&path).unwrap();
        assert!(loaded.is_builtin_disabled("neoism-memory"));
        loaded.install_record(sample_entry("neoism-memory", "1.0.1"));
        assert!(!loaded.is_builtin_disabled("neoism-memory"));
        assert_eq!(loaded.get("neoism-memory").unwrap().version, "1.0.1");
    }

    #[test]
    fn load_from_missing_path_returns_default() {
        let dir = tempdir();
        let path = dir.join("does-not-exist.json");
        let idx = InstalledIndex::load_from(&path).unwrap();
        assert!(idx.entries.is_empty());
    }

    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "neoism-extensions-test-{}-{}",
            std::process::id(),
            nanoid()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn nanoid() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{:x}", n)
    }
}
