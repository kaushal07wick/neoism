use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use neoism_agent_core::{
    event_type, EventPayload, PromptPart, PromptRequest, SessionStatus, UserModel,
};
use serde::Serialize;
use serde_json::json;

use crate::error::ApiError;
use crate::session_run::{busy_status, publish_idle_if_no_run, session_status_payload};
use crate::state::AppState;
use crate::{append_prompt, ensure_session};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionQueueInfo {
    #[serde(rename = "sessionID")]
    session_id: String,
    count: usize,
    running: bool,
    worker: bool,
    items: Vec<SessionQueueItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionQueueItem {
    index: usize,
    text: Option<String>,
    no_reply: bool,
    agent: Option<String>,
    model: Option<UserModel>,
    part_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionQueueMutation {
    #[serde(rename = "sessionID")]
    session_id: String,
    removed: usize,
    queue: SessionQueueInfo,
}

pub(crate) async fn session_queue(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionQueueInfo>, ApiError> {
    ensure_session(&state, &session_id).await?;
    Ok(Json(session_queue_info(&state, &session_id).await))
}

pub(crate) async fn session_queue_clear(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionQueueMutation>, ApiError> {
    ensure_session(&state, &session_id).await?;
    let removed = clear_queued_prompts(&state, &session_id).await;
    publish_prompt_queue_changed(&state, &session_id, "clear", None, removed).await;
    publish_prompt_queue_status(&state, &session_id, 0).await;
    Ok(Json(SessionQueueMutation {
        session_id: session_id.clone(),
        removed,
        queue: session_queue_info(&state, &session_id).await,
    }))
}

pub(crate) async fn session_queue_pop(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionQueueMutation>, ApiError> {
    ensure_session(&state, &session_id).await?;
    let popped = pop_queued_prompt(&state, &session_id).await;
    let removed = usize::from(popped.is_some());
    publish_prompt_queue_changed(&state, &session_id, "pop", popped.as_ref(), removed)
        .await;
    let queue_len = queued_prompt_count(&state, &session_id).await;
    publish_prompt_queue_status(&state, &session_id, queue_len).await;
    Ok(Json(SessionQueueMutation {
        session_id: session_id.clone(),
        removed,
        queue: session_queue_info(&state, &session_id).await,
    }))
}

pub(crate) async fn prompt_async(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<PromptRequest>,
) -> Result<StatusCode, ApiError> {
    ensure_session(&state, &session_id).await?;
    let event_request = request.clone();
    let (start_worker, queue_len) =
        enqueue_prompt_request(&state, &session_id, request).await?;
    publish_prompt_queue_changed(&state, &session_id, "enqueue", Some(&event_request), 0)
        .await;
    publish_prompt_queue_status(&state, &session_id, queue_len).await;
    if start_worker {
        tokio::spawn(drain_prompt_queue(state, session_id));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn enqueue_prompt_request(
    state: &AppState,
    session_id: &str,
    request: PromptRequest,
) -> Result<(bool, usize), ApiError> {
    let mut workers = state.inner.prompt_queue_workers.write().await;
    let queue_len = state
        .inner
        .store
        .enqueue_prompt(session_id, &request)
        .await?;
    let start_worker = workers.insert(session_id.to_string());
    Ok((start_worker, queue_len))
}

async fn session_queue_info(state: &AppState, session_id: &str) -> SessionQueueInfo {
    let items = state
        .inner
        .store
        .list_queued_prompts(session_id)
        .await
        .unwrap_or_default();
    let running = state.inner.runs.read().await.contains_key(session_id);
    let worker = state
        .inner
        .prompt_queue_workers
        .read()
        .await
        .contains(session_id);
    SessionQueueInfo {
        session_id: session_id.to_string(),
        count: items.len(),
        running,
        worker,
        items: items
            .into_iter()
            .enumerate()
            .map(|(index, request)| queued_prompt_item(index, request))
            .collect(),
    }
}

fn queued_prompt_item(index: usize, request: PromptRequest) -> SessionQueueItem {
    let text = queued_prompt_text(&request);
    SessionQueueItem {
        index,
        text,
        no_reply: request.no_reply,
        agent: request.agent,
        model: request.model,
        part_count: request.parts.len(),
    }
}

fn queued_prompt_text(request: &PromptRequest) -> Option<String> {
    request.parts.iter().find_map(|part| match part {
        PromptPart::Text { text } => Some(truncate_queue_preview(text)),
        PromptPart::Agent { name, .. } => Some(format!("@{name}")),
        PromptPart::Subtask {
            description,
            agent,
            prompt,
            ..
        } => {
            let label = if description.trim().is_empty() {
                truncate_queue_preview(prompt)
            } else {
                truncate_queue_preview(description)
            };
            Some(format!("@{agent} {label}"))
        }
        PromptPart::File { filename, .. } => Some(format!("@{filename}")),
    })
}

fn truncate_queue_preview(text: &str) -> String {
    const MAX: usize = 120;
    let text = text.trim().replace('\n', " ");
    let mut preview = text.chars().take(MAX).collect::<String>();
    if text.chars().count() > MAX {
        preview.push_str("...");
    }
    preview
}

/// Clears any queued prompts for a session and publishes the resulting empty
/// queue, returning how many were removed. Used when forcibly stopping a
/// subagent so its pending follow-ups do not start after cancellation.
pub(crate) async fn clear_session_prompt_queue(
    state: &AppState,
    session_id: &str,
) -> usize {
    let removed = clear_queued_prompts(state, session_id).await;
    publish_prompt_queue_changed(state, session_id, "clear", None, removed).await;
    publish_prompt_queue_status(state, session_id, 0).await;
    removed
}

async fn clear_queued_prompts(state: &AppState, session_id: &str) -> usize {
    let removed = state
        .inner
        .store
        .clear_queued_prompts(session_id)
        .await
        .unwrap_or(0);
    if !state.inner.runs.read().await.contains_key(session_id) {
        state
            .inner
            .prompt_queue_workers
            .write()
            .await
            .remove(session_id);
    }
    removed
}

async fn pop_queued_prompt(state: &AppState, session_id: &str) -> Option<PromptRequest> {
    let popped = state
        .inner
        .store
        .pop_queued_prompt(session_id)
        .await
        .ok()
        .flatten();
    if popped.is_some() && !state.inner.runs.read().await.contains_key(session_id) {
        state
            .inner
            .prompt_queue_workers
            .write()
            .await
            .remove(session_id);
    }
    popped
}

pub(crate) async fn queued_prompt_count(state: &AppState, session_id: &str) -> usize {
    state
        .inner
        .store
        .queued_prompt_count(session_id)
        .await
        .unwrap_or(0)
}

pub(crate) async fn queued_prompt_preview(
    state: &AppState,
    session_id: &str,
) -> Option<String> {
    state
        .inner
        .store
        .list_queued_prompts(session_id)
        .await
        .ok()
        .and_then(|queue| queue.into_iter().next())
        .as_ref()
        .and_then(queued_prompt_text)
}

async fn next_queued_prompt(
    state: &AppState,
    session_id: &str,
) -> Option<(PromptRequest, usize)> {
    let mut workers = state.inner.prompt_queue_workers.write().await;
    let Some(request) = state
        .inner
        .store
        .pop_queued_prompt(session_id)
        .await
        .ok()
        .flatten()
    else {
        workers.remove(session_id);
        return None;
    };
    let remaining = state
        .inner
        .store
        .queued_prompt_count(session_id)
        .await
        .unwrap_or(0);
    Some((request, remaining))
}

pub(crate) async fn publish_prompt_queue_status(
    state: &AppState,
    session_id: &str,
    queue_len: usize,
) {
    let active_worker = state
        .inner
        .prompt_queue_workers
        .read()
        .await
        .contains(session_id);
    let status = if active_worker
        || queue_len > 0
        || state.inner.runs.read().await.contains_key(session_id)
    {
        busy_status(queue_len, queued_prompt_preview(state, session_id).await)
    } else {
        SessionStatus::Idle
    };
    let busy = matches!(status, SessionStatus::Busy { .. });
    if busy {
        state
            .inner
            .statuses
            .write()
            .await
            .insert(session_id.to_string(), status.clone());
    } else {
        state.inner.statuses.write().await.remove(session_id);
    }
    let mut payload = session_status_payload(state, session_id, &status).await;
    payload["queue"] = json!(queue_len);
    state.publish(EventPayload::new(event_type::SESSION_STATUS, payload));
}

pub(crate) async fn publish_prompt_queue_changed(
    state: &AppState,
    session_id: &str,
    action: &str,
    request: Option<&PromptRequest>,
    removed: usize,
) {
    let mut payload = json!({
        "sessionID": session_id,
        "action": action,
        "removed": removed,
        "queue": session_queue_info(state, session_id).await,
    });
    if let Some(request) = request {
        payload["request"] = json!(request);
    }
    state.publish(EventPayload::new(
        event_type::SESSION_QUEUE_UPDATED,
        payload,
    ));
}

pub(crate) async fn drain_queued_prompts_into_active_run(
    state: &AppState,
    session_id: &str,
) -> usize {
    let mut drained = 0;
    while let Some((request, remaining)) = next_queued_prompt(state, session_id).await {
        publish_prompt_queue_changed(state, session_id, "dequeue", Some(&request), 1)
            .await;
        publish_prompt_queue_status(state, session_id, remaining).await;
        drained += 1;
        if let Err(error) = append_prompt(state, session_id, request, false).await {
            state.publish(EventPayload::new(
                event_type::SESSION_ERROR,
                json!({ "sessionID": session_id, "error": { "name": "PromptError", "data": { "message": error.to_string() } } }),
            ));
        }
    }
    drained
}

async fn wait_until_session_not_running(state: &AppState, session_id: &str) {
    while state.inner.runs.read().await.contains_key(session_id) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub(crate) async fn drain_prompt_queue(state: AppState, session_id: String) {
    loop {
        wait_until_session_not_running(&state, &session_id).await;
        let Some((request, remaining)) = next_queued_prompt(&state, &session_id).await
        else {
            break;
        };
        publish_prompt_queue_changed(&state, &session_id, "dequeue", Some(&request), 1)
            .await;
        publish_prompt_queue_status(&state, &session_id, remaining).await;
        let create_reply = !request.no_reply;
        if let Err(error) =
            append_prompt(&state, &session_id, request, create_reply).await
        {
            state.publish(EventPayload::new(
                event_type::SESSION_ERROR,
                json!({ "sessionID": session_id, "error": { "name": "PromptError", "data": { "message": error.to_string() } } }),
            ));
        }
    }
    publish_idle_if_no_run(&state, &session_id).await;
}

pub(crate) fn spawn_drain_prompt_queue(state: AppState, session_id: String) {
    tokio::spawn(drain_prompt_queue(state, session_id));
}

pub(crate) async fn resume_prompt_queues(state: AppState) -> anyhow::Result<()> {
    for session_id in state.inner.store.queued_session_ids().await? {
        let mut workers = state.inner.prompt_queue_workers.write().await;
        if workers.insert(session_id.clone()) {
            tokio::spawn(drain_prompt_queue(state.clone(), session_id));
        }
    }
    Ok(())
}
