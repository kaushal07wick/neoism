use std::path::PathBuf;

use axum::extract::Query;
use axum::http::HeaderMap;
use axum::Json;
use neoism_agent_core::SearchMatch;
use serde::Deserialize;

use crate::{lsp, resolve_directory, InstanceQuery};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FileFindQuery {
    pub pattern: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FileFindFileQuery {
    pub query: String,
    pub dirs: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SymbolQuery {
    pub query: String,
}

pub(crate) async fn find_text(
    Query(query): Query<FileFindQuery>,
    Query(instance): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<Vec<SearchMatch>> {
    let directory = resolve_directory(instance.directory, &headers);
    Json(search_text(&PathBuf::from(directory), &query.pattern, 10))
}

pub(crate) async fn find_file(
    Query(query): Query<FileFindFileQuery>,
    Query(instance): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<Vec<String>> {
    let directory = PathBuf::from(resolve_directory(instance.directory, &headers));
    let limit = query.limit.unwrap_or(10).min(200);
    Json(search_files(&directory, &query, limit))
}

pub(crate) async fn find_symbol(
    Query(query): Query<SymbolQuery>,
    Query(instance): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<Vec<lsp::WorkspaceSymbol>> {
    let directory = resolve_directory(instance.directory, &headers);
    Json(lsp::workspace_symbols(directory, &query.query))
}

fn search_text(root: &PathBuf, pattern: &str, limit: usize) -> Vec<SearchMatch> {
    let mut matches = Vec::new();
    search_text_inner(root, pattern, limit, &mut matches);
    matches
}

fn search_text_inner(
    path: &PathBuf,
    pattern: &str,
    limit: usize,
    matches: &mut Vec<SearchMatch>,
) {
    if matches.len() >= limit {
        return;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                search_text_inner(&entry.path(), pattern, limit, matches);
                if matches.len() >= limit {
                    break;
                }
            }
        }
        return;
    }
    if metadata.len() > 2_000_000 {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for (index, line) in content.lines().enumerate() {
        if let Some(column) = line.find(pattern) {
            matches.push(SearchMatch {
                path: path.display().to_string(),
                line: index as u64 + 1,
                column: column as u64 + 1,
                text: line.to_string(),
            });
            if matches.len() >= limit {
                break;
            }
        }
    }
}

fn search_files(root: &PathBuf, query: &FileFindFileQuery, limit: usize) -> Vec<String> {
    let mut matches = Vec::new();
    search_files_inner(root, query, limit, &mut matches);
    matches
}

fn search_files_inner(
    path: &PathBuf,
    query: &FileFindFileQuery,
    limit: usize,
    matches: &mut Vec<String>,
) {
    if matches.len() >= limit {
        return;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let is_dir = metadata.is_dir();
    if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
        return;
    }
    let include_dirs = query.dirs.as_deref() != Some("false");
    let kind_matches = match query.kind.as_deref() {
        Some("file") => metadata.is_file(),
        Some("directory") => is_dir,
        _ => metadata.is_file() || (include_dirs && is_dir),
    };
    if kind_matches
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.contains(&query.query))
            .unwrap_or(false)
    {
        matches.push(path.display().to_string());
    }
    if is_dir {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                search_files_inner(&entry.path(), query, limit, matches);
                if matches.len() >= limit {
                    break;
                }
            }
        }
    }
}
