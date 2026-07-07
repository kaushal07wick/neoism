use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use neoism_agent_core::EventPayload;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;
use crate::{resolve_directory, worktree, InstanceQuery};

pub(crate) async fn worktree_create(
    State(state): State<AppState>,
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    match worktree::create(
        &directory,
        worktree::create_request_from_value(body.map(|Json(value)| value)),
    ) {
        Ok(created) => {
            state.publish(EventPayload::new(
                "worktree.ready",
                json!({ "directory": created.directory.clone(), "branch": created.branch.clone() }),
            ));
            Ok(Json(json!(created)))
        }
        Err(message) => {
            state.publish(EventPayload::new(
                "worktree.failed",
                json!({ "directory": directory, "message": message }),
            ));
            Err(ApiError::bad_request(message))
        }
    }
}

pub(crate) async fn worktree_list(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    worktree::list(&directory)
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(crate) async fn worktree_remove(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<bool>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    let request = worktree::path_request_from_value(body.map(|Json(value)| value))
        .ok_or_else(|| ApiError::bad_request("missing worktree remove input"))?;
    worktree::remove(&directory, request)
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(crate) async fn worktree_reset(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<bool>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    worktree::reset(
        &directory,
        worktree::path_request_from_value(body.map(|Json(value)| value)),
    )
    .map(Json)
    .map_err(ApiError::bad_request)
}
