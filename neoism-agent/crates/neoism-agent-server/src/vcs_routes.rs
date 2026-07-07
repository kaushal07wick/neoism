use axum::extract::Query;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use neoism_agent_core::{VcsApplyResult, VcsFileDiff, VcsFileStatus, VcsInfo};
use serde_json::Value;

use crate::{resolve_directory, vcs, InstanceQuery};

pub(crate) async fn vcs_get(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<VcsInfo> {
    let directory = resolve_directory(query.directory, &headers);
    Json(vcs::info(&directory))
}

pub(crate) async fn vcs_status(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<Vec<VcsFileStatus>> {
    let directory = resolve_directory(query.directory, &headers);
    Json(vcs::status(&directory))
}

pub(crate) async fn vcs_diff(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<Vec<VcsFileDiff>> {
    let directory = resolve_directory(query.directory, &headers);
    Json(vcs::diff(&directory))
}

pub(crate) async fn vcs_diff_raw(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Response {
    let directory = resolve_directory(query.directory, &headers);
    let body = vcs::diff_raw(&directory);
    ([("content-type", "text/x-diff; charset=utf-8")], body).into_response()
}

pub(crate) async fn vcs_apply(
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<VcsApplyResult> {
    let directory = resolve_directory(
        body.get("directory")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        &headers,
    );
    let Some(patch) = vcs::patch_from_body(&body) else {
        return Json(VcsApplyResult {
            success: false,
            error: Some("missing patch".to_string()),
        });
    };
    Json(vcs::apply(&directory, patch))
}
