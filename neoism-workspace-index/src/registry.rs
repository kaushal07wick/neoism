use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::config::NeoismWorkspace;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WorkspaceRegistry {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub last_opened: i64,
}

pub fn registry_path() -> PathBuf {
    config_dir_path().join("workspaces.toml")
}

pub fn load_registry() -> std::io::Result<WorkspaceRegistry> {
    let path = registry_path();
    if !path.is_file() {
        return Ok(WorkspaceRegistry {
            version: 1,
            workspaces: Vec::new(),
        });
    }
    let source = fs::read_to_string(&path)?;
    toml::from_str::<WorkspaceRegistry>(&source).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to parse {}: {err}", path.display()),
        )
    })
}

pub fn save_registry(registry: &WorkspaceRegistry) -> std::io::Result<()> {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let source = toml::to_string_pretty(registry).map_err(std::io::Error::other)?;
    fs::write(path, source)
}

pub fn register_workspace(workspace: &NeoismWorkspace) -> std::io::Result<()> {
    let mut registry = load_registry()?;
    let now = unix_now();
    let root = normalize_path(&workspace.root);
    let mut updated = false;
    for record in &mut registry.workspaces {
        if record.id == workspace.config.id || same_path(&record.path, &root) {
            if record.id == workspace.config.id
                && record.name == workspace.config.name
                && same_path(&record.path, &root)
                && record.last_opened == now
            {
                return Ok(());
            }
            record.id = workspace.config.id.clone();
            record.name = workspace.config.name.clone();
            record.path = root.clone();
            record.last_opened = now;
            updated = true;
            break;
        }
    }
    if !updated {
        registry.workspaces.push(WorkspaceRecord {
            id: workspace.config.id.clone(),
            name: workspace.config.name.clone(),
            path: root,
            last_opened: now,
        });
    }
    registry
        .workspaces
        .sort_by(|a, b| b.last_opened.cmp(&a.last_opened).then(a.name.cmp(&b.name)));
    save_registry(&registry)
}

pub fn touch_workspace(workspace: &NeoismWorkspace) -> std::io::Result<()> {
    let registry = load_registry()?;
    let root = normalize_path(&workspace.root);
    if registry.workspaces.iter().any(|record| {
        record.id == workspace.config.id
            && record.name == workspace.config.name
            && same_path(&record.path, &root)
    }) {
        return Ok(());
    }
    register_workspace(workspace)
}

fn same_path(a: &Path, b: &Path) -> bool {
    normalize_path(a) == normalize_path(b)
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn default_version() -> u32 {
    1
}

fn config_dir_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
        .join("neoism")
}
