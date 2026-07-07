use std::path::PathBuf;

use axum::extract::Query;
use axum::http::HeaderMap;
use axum::Json;
use neoism_agent_core::{FileContent, FileInfo, FileNode};
use serde::Deserialize;

use crate::error::ApiError;
use crate::{resolve_directory, vcs, InstanceQuery};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FilePathQuery {
    pub path: String,
}

pub(crate) async fn file_list(
    Query(query): Query<FilePathQuery>,
    Query(instance): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<FileNode>>, ApiError> {
    let path = resolve_file_path(
        &resolve_directory(instance.directory, &headers),
        &query.path,
    );
    let entries = std::fs::read_dir(&path).map_err(|error| {
        ApiError::not_found(format!("failed to list {}: {error}", path.display()))
    })?;
    let mut nodes = Vec::new();
    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata() {
            nodes.push(FileNode {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().display().to_string(),
                kind: if metadata.is_dir() {
                    "directory"
                } else {
                    "file"
                }
                .to_string(),
                ignored: false,
                children: None,
            });
        }
    }
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Json(nodes))
}

pub(crate) async fn file_read(
    Query(query): Query<FilePathQuery>,
    Query(instance): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Result<Json<FileContent>, ApiError> {
    let path = resolve_file_path(
        &resolve_directory(instance.directory, &headers),
        &query.path,
    );
    let content = std::fs::read_to_string(&path).map_err(|error| {
        ApiError::not_found(format!("failed to read {}: {error}", path.display()))
    })?;
    Ok(Json(FileContent {
        path: path.display().to_string(),
        content,
    }))
}

pub(crate) async fn file_status(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<Vec<FileInfo>> {
    let directory = resolve_directory(query.directory, &headers);
    Json(
        vcs::status(&directory)
            .into_iter()
            .map(|status| FileInfo {
                path: status.path,
                status: status.status,
            })
            .collect(),
    )
}

fn resolve_file_path(directory: &str, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        PathBuf::from(directory).join(path)
    }
}
