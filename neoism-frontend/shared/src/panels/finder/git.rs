// Git-changes mode: SearchService-backed status fetch, status
// classification, and exact-match sorting over the change list.

use std::path::{Path, PathBuf};

use super::file_search::exact_file_match_score;
use super::types::{GitChangeStatus, GitResult, Result_};
use crate::services::SearchService;

pub(super) fn collect_git_changes(
    search: &dyn SearchService,
    cwd: &Path,
) -> Vec<GitResult> {
    let hits = match search.collect_git_changes(cwd) {
        Ok(hits) => hits,
        Err(error) => {
            tracing::warn!(
                target: "neoism::finder",
                ?error,
                "SearchService::collect_git_changes failed"
            );
            return Vec::new();
        }
    };
    hits.into_iter()
        .map(|hit| GitResult {
            path: hit.path,
            status: GitChangeStatus::from_service(hit.status),
            line: hit.line,
            text: hit.text,
        })
        .collect()
}

pub(super) fn git_repo_root(search: &dyn SearchService, cwd: &Path) -> Option<PathBuf> {
    search.git_repo_root(cwd)
}

pub(super) fn exact_git_results(
    query: &str,
    changes: &[GitResult],
) -> Vec<(i32, Result_)> {
    let query = query.trim();
    if query.is_empty() {
        return changes
            .iter()
            .take(500)
            .cloned()
            .map(|change| (0, Result_::Git(change)))
            .collect();
    }

    let smart_case = query.chars().any(|c| c.is_ascii_uppercase());
    let mut scored: Vec<(i32, Result_)> = changes
        .iter()
        .filter_map(|change| {
            let score = exact_file_match_score(query, &change.path, smart_case)?;
            Some((score, Result_::Git(change.clone())))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path().cmp(b.1.path())));
    scored.truncate(500);
    scored
}
