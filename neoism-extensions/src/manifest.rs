use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub downloads: Option<u64>,
    pub categories: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    pub repository_url: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    pub install: InstallKind,
    pub run: Option<RunSpec>,
    #[serde(default)]
    pub env_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallKind {
    Npm {
        package: String,
        version: String,
    },
    Pip {
        package: String,
        version: String,
    },
    GithubRelease {
        owner: String,
        repo: String,
        tag: String,
        assets: Vec<GithubAsset>,
    },
    Cargo {
        crate_name: String,
        version: String,
        features: Vec<String>,
    },
}

/// One per-platform asset entry. `target` follows the Mason naming scheme
/// (e.g. `linux_x64_gnu`, `darwin_arm64`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GithubAsset {
    pub target: String,
    pub file: String,
    pub bin: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSpec {
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
}
