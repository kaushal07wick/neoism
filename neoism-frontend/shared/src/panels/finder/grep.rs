// Grep-mode helpers — fallback path that asks the SearchService for
// raw `rg <query>` results (used when the service can't return its
// own fuzzy/regex grep scores).

use std::path::Path;

use super::modes::GrepSearchMode;
use super::types::{GrepResult, Result_};
use crate::services::SearchService;

/// Ask the SearchService to run a grep and adapt the rows into the
/// finder's internal `Result_` shape.
pub(super) fn run_ripgrep(
    search: &dyn SearchService,
    cwd: &Path,
    query: &str,
    mode: GrepSearchMode,
) -> Vec<(i32, Result_)> {
    let hits = match search.search_grep(cwd, query, mode.as_service_mode()) {
        Ok(hits) => hits,
        Err(error) => {
            tracing::warn!(
                target: "neoism::finder",
                ?error,
                "SearchService::search_grep failed"
            );
            return Vec::new();
        }
    };
    hits.into_iter()
        .take(500)
        .map(|hit| {
            (
                hit.score,
                Result_::Grep(GrepResult {
                    path: hit.path,
                    line: hit.line,
                    column: hit.column,
                    text: hit.text,
                }),
            )
        })
        .collect()
}
