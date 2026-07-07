//! Portable Agent Client Protocol policy helpers.
//!
//! This module stays I/O-free. Desktop hosts are still responsible for
//! resolving symlinks, checking existence, and reading or writing files.

use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpWorkspacePathError {
    MissingPath,
    RelativePath { raw: String },
    OutsideWorkspace { path: PathBuf },
}

impl AcpWorkspacePathError {
    pub fn json_rpc_code(&self) -> i64 {
        match self {
            Self::MissingPath | Self::RelativePath { .. } => -32602,
            Self::OutsideWorkspace { .. } => -32000,
        }
    }
}

impl fmt::Display for AcpWorkspacePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPath => write!(f, "ACP request missing path"),
            Self::RelativePath { raw } => write!(f, "ACP path must be absolute: {raw}"),
            Self::OutsideWorkspace { path } => write!(
                f,
                "ACP file access outside workspace is blocked: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for AcpWorkspacePathError {}

pub fn prepare_workspace_path(
    raw: Option<&str>,
) -> Result<PathBuf, AcpWorkspacePathError> {
    let raw = raw.ok_or(AcpWorkspacePathError::MissingPath)?;
    normalize_absolute_path(Path::new(raw)).ok_or_else(|| {
        AcpWorkspacePathError::RelativePath {
            raw: raw.to_string(),
        }
    })
}

pub fn authorize_workspace_path(
    requested_path: &Path,
    resolved_workspace: &Path,
    resolved_existing_ancestor: &Path,
) -> Result<(), AcpWorkspacePathError> {
    if resolved_existing_ancestor.starts_with(resolved_workspace) {
        Ok(())
    } else {
        Err(AcpWorkspacePathError::OutsideWorkspace {
            path: requested_path.to_path_buf(),
        })
    }
}

pub fn normalize_absolute_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn prepare_workspace_path_requires_a_path() {
        let err = prepare_workspace_path(None).expect_err("missing path should fail");
        assert_eq!(err, AcpWorkspacePathError::MissingPath);
        assert_eq!(err.json_rpc_code(), -32602);
        assert_eq!(err.to_string(), "ACP request missing path");
    }

    #[cfg(unix)]
    #[test]
    fn prepare_workspace_path_rejects_relative_paths() {
        let err = prepare_workspace_path(Some("src/lib.rs"))
            .expect_err("relative path should fail");
        assert_eq!(
            err,
            AcpWorkspacePathError::RelativePath {
                raw: "src/lib.rs".to_string()
            }
        );
        assert_eq!(err.json_rpc_code(), -32602);
        assert_eq!(err.to_string(), "ACP path must be absolute: src/lib.rs");
    }

    #[cfg(unix)]
    #[test]
    fn prepare_workspace_path_normalizes_absolute_paths_lexically() {
        let path = prepare_workspace_path(Some("/tmp/work/./src/../README.md"))
            .expect("absolute path should normalize");
        assert_eq!(path, PathBuf::from("/tmp/work/README.md"));
    }

    #[cfg(unix)]
    #[test]
    fn authorize_workspace_path_allows_resolved_descendants() {
        let requested = Path::new("/tmp/work/src/new.rs");
        authorize_workspace_path(
            requested,
            Path::new("/tmp/work"),
            Path::new("/tmp/work/src"),
        )
        .expect("descendant should be allowed");
    }

    #[cfg(unix)]
    #[test]
    fn authorize_workspace_path_blocks_resolved_siblings() {
        let requested = Path::new("/tmp/other/src/new.rs");
        let err = authorize_workspace_path(
            requested,
            Path::new("/tmp/work"),
            Path::new("/tmp/other/src"),
        )
        .expect_err("sibling should be blocked");
        assert_eq!(
            err,
            AcpWorkspacePathError::OutsideWorkspace {
                path: requested.to_path_buf()
            }
        );
        assert_eq!(err.json_rpc_code(), -32000);
        assert_eq!(
            err.to_string(),
            "ACP file access outside workspace is blocked: /tmp/other/src/new.rs"
        );
    }
}
