use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use neoism_agent_core::{event_type, EventPayload, NeoismConfig};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;
use crate::{
    config, default_config_dir, default_state_dir, pty, resolve_directory, InstanceQuery,
};

pub(crate) async fn global_health() -> Json<Value> {
    Json(json!({ "healthy": true, "version": env!("CARGO_PKG_VERSION") }))
}

pub(crate) async fn path_get(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<Value> {
    let directory = resolve_directory(query.directory, &headers);
    Json(json!({
        "home": std::env::var("HOME").unwrap_or_default(),
        "state": default_state_dir(),
        "config": default_config_dir(),
        "worktree": directory,
        "directory": directory,
    }))
}

pub(crate) async fn config_get(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Result<Json<NeoismConfig>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    Ok(Json(config::load(&directory)?.info))
}

pub(crate) async fn config_validate(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<config::ConfigValidation> {
    let directory = resolve_directory(query.directory, &headers);
    Json(config::validate(&directory))
}

pub(crate) async fn config_update(
    Json(config): Json<NeoismConfig>,
) -> Json<NeoismConfig> {
    Json(config)
}

pub(crate) async fn global_dispose(State(state): State<AppState>) -> Json<bool> {
    pty::stop_all_pty_processes().await;
    state.publish(EventPayload::new(
        event_type::SERVER_INSTANCE_DISPOSED,
        json!({}),
    ));
    Json(true)
}

pub(crate) async fn instance_dispose(State(state): State<AppState>) -> Json<bool> {
    global_dispose(State(state)).await
}

pub(crate) async fn global_upgrade(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({
        "success": false,
        "error": format!(
            "self-upgrade is not available for neoism-agent{}",
            body.get("target")
                .and_then(Value::as_str)
                .map(|target| format!(" target {target}"))
                .unwrap_or_default()
        )
    }))
}
