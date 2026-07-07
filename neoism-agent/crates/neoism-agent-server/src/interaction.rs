use axum::extract::{Path, State};
use axum::Json;
use neoism_agent_core::{
    event_type, EventPayload, PermissionRequestInfo, QuestionRequestInfo,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;
use crate::{permission_grants, permission_request_allowed};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PermissionReplyRequest {
    pub(crate) reply: Option<String>,
    pub(crate) response: Option<String>,
    pub(crate) message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QuestionReplyRequest {
    pub(crate) answers: Vec<Vec<String>>,
}

pub(crate) async fn permission_list(
    State(state): State<AppState>,
) -> Json<Vec<PermissionRequestInfo>> {
    Json(
        state
            .inner
            .permissions
            .read()
            .await
            .values()
            .cloned()
            .collect(),
    )
}

pub(crate) async fn permission_reply(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    Json(reply): Json<PermissionReplyRequest>,
) -> Result<Json<bool>, ApiError> {
    let reply_kind = reply
        .reply
        .clone()
        .or_else(|| reply.response.clone())
        .unwrap_or_else(|| "once".to_string());
    let pending = state
        .inner
        .permission_waiters
        .write()
        .await
        .remove(&request_id);
    let removed = state.inner.permissions.write().await.remove(&request_id);
    let mut publish = Vec::new();
    let mut sends = Vec::new();

    if let Some(pending) = pending {
        publish.push((pending.request.clone(), reply_kind.clone()));
        match reply_kind.as_str() {
            "reject" => {
                sends.push((
                    pending.sender,
                    Err(reply
                        .message
                        .clone()
                        .unwrap_or_else(|| "Permission rejected".to_string())),
                ));
                let same_session = {
                    let waiters = state.inner.permission_waiters.read().await;
                    waiters
                        .iter()
                        .filter(|(_, item)| {
                            item.request.session_id == pending.request.session_id
                        })
                        .map(|(id, _)| id.clone())
                        .collect::<Vec<_>>()
                };
                for id in same_session {
                    if let Some(item) =
                        state.inner.permission_waiters.write().await.remove(&id)
                    {
                        state.inner.permissions.write().await.remove(&id);
                        publish.push((item.request.clone(), "reject".to_string()));
                        sends.push((item.sender, Err("Permission rejected".to_string())));
                    }
                }
            }
            "always" => {
                let grants = permission_grants(&pending.request, true);
                let project_id = state
                    .inner
                    .store
                    .get_session(&pending.request.session_id)
                    .await?
                    .map(|session| session.project_id)
                    .unwrap_or_else(|| "global".to_string());
                let approvals = {
                    let mut approval_map = state.inner.permission_approvals.write().await;
                    let approvals = approval_map.entry(project_id.clone()).or_default();
                    approvals.extend(grants.clone());
                    approvals.clone()
                };
                state
                    .inner
                    .store
                    .save_permission_approvals(&project_id, &approvals)
                    .await?;
                sends.push((pending.sender, Ok(grants)));
                let same_session = {
                    let waiters = state.inner.permission_waiters.read().await;
                    waiters
                        .iter()
                        .filter(|(_, item)| {
                            item.request.session_id == pending.request.session_id
                        })
                        .filter(|(_, item)| {
                            permission_request_allowed(&item.request, &approvals)
                        })
                        .map(|(id, _)| id.clone())
                        .collect::<Vec<_>>()
                };
                for id in same_session {
                    if let Some(item) =
                        state.inner.permission_waiters.write().await.remove(&id)
                    {
                        state.inner.permissions.write().await.remove(&id);
                        publish.push((item.request.clone(), "always".to_string()));
                        sends.push((item.sender, Ok(Vec::new())));
                    }
                }
            }
            _ => {
                sends.push((
                    pending.sender,
                    Ok(permission_grants(&pending.request, false)),
                ));
            }
        }
    }

    let published_specific = !publish.is_empty();
    for (request, reply_kind) in publish {
        let mut payload =
            crate::permission_runtime::permission_request_payload(&state, &request).await;
        payload["requestID"] = json!(request.id);
        payload["reply"] = json!(reply_kind);
        state.publish(EventPayload::new(event_type::PERMISSION_REPLIED, payload));
    }
    if !published_specific {
        if let Some(request) = removed.as_ref() {
            let mut payload =
                crate::permission_runtime::permission_request_payload(&state, request)
                    .await;
            payload["requestID"] = json!(request_id);
            payload["reply"] = json!(reply);
            state.publish(EventPayload::new(event_type::PERMISSION_REPLIED, payload));
        } else {
            state.publish(EventPayload::new(
                event_type::PERMISSION_REPLIED,
                json!({ "requestID": request_id, "reply": reply }),
            ));
        }
    }
    for (sender, result) in sends {
        let _ = sender.send(result);
    }
    Ok(Json(true))
}

pub(crate) async fn question_list(
    State(state): State<AppState>,
) -> Json<Vec<QuestionRequestInfo>> {
    Json(
        state
            .inner
            .questions
            .read()
            .await
            .values()
            .cloned()
            .collect(),
    )
}

pub(crate) async fn question_reply(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    Json(reply): Json<QuestionReplyRequest>,
) -> Json<bool> {
    let removed = state.inner.questions.write().await.remove(&request_id);
    if let Some(pending) = state
        .inner
        .question_waiters
        .write()
        .await
        .remove(&request_id)
    {
        let _ = pending.sender.send(Ok(reply.answers.clone()));
    }
    state.publish(EventPayload::new(
        event_type::QUESTION_REPLIED,
        json!({ "requestID": request_id, "reply": reply, "info": removed }),
    ));
    Json(true)
}

pub(crate) async fn question_reject(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> Json<bool> {
    let removed = state.inner.questions.write().await.remove(&request_id);
    if let Some(pending) = state
        .inner
        .question_waiters
        .write()
        .await
        .remove(&request_id)
    {
        let _ = pending
            .sender
            .send(Err("question request was rejected".to_string()));
    }
    state.publish(EventPayload::new(
        event_type::QUESTION_REJECTED,
        json!({ "requestID": request_id, "info": removed }),
    ));
    Json(true)
}
