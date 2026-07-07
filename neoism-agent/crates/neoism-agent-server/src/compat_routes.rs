use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub(crate) async fn empty_array() -> Json<Vec<Value>> {
    Json(Vec::new())
}

pub(crate) async fn sync_start() -> Json<bool> {
    Json(true)
}

pub(crate) async fn sync_replay(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let events = body
        .get("events")
        .and_then(Value::as_array)
        .filter(|events| !events.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("sync replay requires a non-empty events array")
        })?;
    let mut parsed = Vec::new();
    for event in events {
        parsed.push(
            crate::sync::replay_event_payload(event)
                .map_err(|error| ApiError::bad_request(error.to_string()))?,
        );
    }
    let owner_id = body
        .get("ownerID")
        .or_else(|| body.get("ownerId"))
        .and_then(Value::as_str);
    let source = crate::sync::replay_all(&state, parsed, owner_id, true)
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?
        .unwrap_or_default();
    Ok(Json(json!({ "sessionID": source })))
}

pub(crate) async fn sync_steal(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let session_id = body
        .get("sessionID")
        .or_else(|| body.get("sessionId"))
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad_request("sync steal requires sessionID"))?;
    if let Some(owner_id) = body
        .get("ownerID")
        .or_else(|| body.get("ownerId"))
        .or_else(|| body.get("workspaceID"))
        .or_else(|| body.get("workspaceId"))
        .and_then(Value::as_str)
    {
        state
            .inner
            .store
            .claim_aggregate_owner(session_id, owner_id)
            .await
            .map_err(ApiError::from)?;
    }
    Ok(Json(json!({ "sessionID": session_id })))
}

pub(crate) async fn sync_history(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Json<Vec<Value>> {
    if let Some(known_sequences) = crate::sync::parse_known_sequences(&body) {
        let events = state
            .inner
            .store
            .list_events_after(0, usize::MAX, None)
            .await
            .unwrap_or_default();
        return Json(crate::sync::opencode_history_rows(events, &known_sequences));
    }
    let since = body
        .get("since")
        .or_else(|| body.get("cursor"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let limit = body
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1_000);
    let session_id = body
        .get("sessionID")
        .or_else(|| body.get("sessionId"))
        .and_then(Value::as_str);
    let events = state
        .inner
        .store
        .list_events_after(since, limit, session_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|event| {
            json!({
                "seq": event.seq,
                "id": event.payload.id,
                "type": event.payload.kind,
                "properties": event.payload.properties,
            })
        })
        .collect();
    Json(events)
}

pub(crate) async fn experimental_console_get() -> Json<Value> {
    Json(json!({ "providers": [], "switchableOrgCount": 0 }))
}

pub(crate) async fn experimental_console_orgs() -> Json<Value> {
    Json(json!({ "orgs": [] }))
}

pub(crate) async fn experimental_console_switch(Json(_body): Json<Value>) -> Json<bool> {
    Json(true)
}
