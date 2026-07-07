use axum::extract::{Path, Query, State};
use axum::Json;
use neoism_agent_core::{event_type, EventPayload, MessageWithParts, Part, ToolState};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;
use crate::{now_millis, part_id_of};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct MessageListQuery {
    pub limit: Option<usize>,
    pub order: Option<String>,
    pub slim: Option<bool>,
    pub cursor: Option<String>,
}

pub(crate) async fn message_list(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<MessageListQuery>,
) -> Result<Json<Vec<MessageWithParts>>, ApiError> {
    let started = crate::perf::now();
    state
        .inner
        .store
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Session not found"))?;
    // Page in SQL (cursor + limit) instead of loading the whole transcript and
    // slicing in memory — "load older" on a huge session no longer re-reads
    // every message each request.
    let order_desc = query.order.as_deref() == Some("desc");
    let mut messages = state
        .inner
        .store
        .list_messages_page(
            &session_id,
            query.cursor.as_deref(),
            query.limit,
            order_desc,
        )
        .await?;
    if query.slim == Some(true) {
        for message in &mut messages {
            slim_message(message);
        }
    }
    if crate::perf::enabled() {
        let part_count: usize = messages.iter().map(|message| message.parts.len()).sum();
        let json_bytes = serde_json::to_string(&messages).map(|text| text.len()).ok();
        tracing::info!(
            target: "neoism_agent::perf",
            session_id,
            limit = query.limit,
            order = query.order.as_deref(),
            cursor = query.cursor.as_deref(),
            messages = messages.len(),
            part_count,
            json_bytes,
            elapsed_ms = crate::perf::elapsed_ms(started),
            "session message list"
        );
    }
    Ok(Json(messages))
}

fn slim_message(message: &mut MessageWithParts) {
    for part in &mut message.parts {
        match part {
            Part::Tool(part) => {
                part.metadata = slim_metadata(part.metadata.take());
                match &mut part.state {
                    ToolState::Completed { metadata, .. } => {
                        *metadata = slim_metadata_value(std::mem::take(metadata));
                    }
                    ToolState::Pending { .. }
                    | ToolState::Running { .. }
                    | ToolState::Error { .. } => {}
                }
            }
            Part::Reasoning(part) => {
                part.metadata = slim_metadata(part.metadata.take());
            }
            _ => {}
        }
    }
}

fn slim_metadata(metadata: Option<Value>) -> Option<Value> {
    let Some(Value::Object(mut metadata)) = metadata else {
        return metadata;
    };
    if metadata.remove("snapshots").is_some() {
        metadata.insert("snapshotsOmitted".to_string(), Value::Bool(true));
    }
    Some(Value::Object(metadata))
}

fn slim_metadata_value(metadata: Value) -> Value {
    slim_metadata(Some(metadata)).unwrap_or(Value::Null)
}

pub(crate) async fn message_get(
    State(state): State<AppState>,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Json<MessageWithParts>, ApiError> {
    let message = state
        .inner
        .store
        .get_message(&session_id, &message_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Message not found"))?;
    Ok(Json(message))
}

pub(crate) async fn message_delete(
    State(state): State<AppState>,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Json<bool>, ApiError> {
    if !state
        .inner
        .store
        .delete_message(&session_id, &message_id)
        .await?
    {
        return Err(ApiError::not_found("Message not found"));
    }
    state.publish(EventPayload::new(
        event_type::MESSAGE_REMOVED,
        json!({ "sessionID": session_id, "messageID": message_id }),
    ));
    Ok(Json(true))
}

pub(crate) async fn part_delete(
    State(state): State<AppState>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
) -> Result<Json<bool>, ApiError> {
    let mut message = state
        .inner
        .store
        .get_message(&session_id, &message_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Message not found"))?;
    let before = message.parts.len();
    message.parts.retain(|part| part_id_of(part) != part_id);
    if message.parts.len() == before {
        return Err(ApiError::not_found("Part not found"));
    }
    state
        .inner
        .store
        .update_message(&session_id, &message)
        .await?;
    state.publish(EventPayload::new(
        event_type::MESSAGE_PART_REMOVED,
        json!({ "sessionID": session_id, "messageID": message_id, "partID": part_id }),
    ));
    Ok(Json(true))
}

pub(crate) async fn part_update(
    State(state): State<AppState>,
    Path((session_id, message_id, part_id)): Path<(String, String, String)>,
    Json(part): Json<Part>,
) -> Result<Json<Part>, ApiError> {
    if part_id_of(&part) != part_id {
        return Err(ApiError::bad_request("part id does not match route"));
    }
    let mut message = state
        .inner
        .store
        .get_message(&session_id, &message_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Message not found"))?;
    let Some(existing) = message
        .parts
        .iter_mut()
        .find(|existing| part_id_of(existing) == part_id)
    else {
        return Err(ApiError::not_found("Part not found"));
    };
    *existing = part.clone();
    state
        .inner
        .store
        .update_message(&session_id, &message)
        .await?;
    state.publish(EventPayload::new(
        event_type::MESSAGE_PART_UPDATED,
        json!({ "sessionID": session_id, "part": part, "time": now_millis() }),
    ));
    Ok(Json(part))
}
