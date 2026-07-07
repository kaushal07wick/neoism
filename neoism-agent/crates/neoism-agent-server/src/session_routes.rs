use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use neoism_agent_core::{
    event_type, CreateSessionRequest, EventPayload, Id, IdKind, MessageId, MessageInfo,
    MessageWithParts, Part, SessionInfo, TimeInfo, TodoInfo, VcsFileDiff,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agent::AgentCatalog;
use crate::error::ApiError;
use crate::state::AppState;
use crate::{
    config, filter_sessions, message_id_of, model_ref_from_config_with_variant,
    now_millis, project, resolve_directory, slug, vcs, InstanceQuery,
};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SessionListQuery {
    pub directory: Option<String>,
    pub path: Option<String>,
    pub roots: Option<String>,
    pub start: Option<u64>,
    pub search: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SessionUpdateRequest {
    title: Option<String>,
    agent: Option<String>,
    permission: Option<Vec<neoism_agent_core::PermissionRule>>,
    model: Option<neoism_agent_core::ModelRef>,
    time: Option<SessionUpdateTime>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SessionUpdateTime {
    archived: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ForkSessionRequest {
    message_id: Option<MessageId>,
}

pub(crate) async fn session_list(
    State(state): State<AppState>,
    Query(query): Query<SessionListQuery>,
) -> Result<Json<Vec<SessionInfo>>, ApiError> {
    let mut sessions = state.inner.store.list_sessions().await?;
    filter_sessions(&mut sessions, &query);
    Ok(Json(sessions))
}

pub(crate) async fn session_create(
    State(state): State<AppState>,
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
    body: Option<Json<CreateSessionRequest>>,
) -> Result<Json<SessionInfo>, ApiError> {
    let request = body.map(|Json(body)| body).unwrap_or(CreateSessionRequest {
        parent_id: None,
        title: None,
        agent: None,
        model: None,
        permission: None,
        workspace_id: None,
    });
    let now = now_millis();
    let id = neoism_agent_core::new_session_id();
    let project_context = project::discover(resolve_directory(query.directory, &headers));
    let directory = project_context.directory.clone();
    let loaded_config = config::load(&directory)?;
    let agents = AgentCatalog::from_config(&loaded_config.info);
    let is_child = request.parent_id.is_some();
    let info = SessionInfo {
        id: id.clone(),
        slug: slug(),
        project_id: project_context.info.id,
        workspace_id: request.workspace_id,
        directory,
        path: project_context.path,
        parent_id: request.parent_id,
        title: request
            .title
            .unwrap_or_else(|| neoism_agent_core::default_session_title(is_child, now)),
        agent: Some(
            request
                .agent
                .unwrap_or_else(|| agents.default_agent().to_string()),
        ),
        model: request.model.or_else(|| {
            loaded_config.info.model.as_deref().and_then(|model| {
                model_ref_from_config_with_variant(
                    model,
                    loaded_config.info.variant.clone(),
                )
            })
        }),
        version: env!("CARGO_PKG_VERSION").to_string(),
        time: TimeInfo {
            created: now,
            updated: now,
            compacting: None,
            archived: None,
        },
        permission: request.permission,
        extra: BTreeMap::new(),
    };

    state.inner.store.insert_session(&info).await?;
    state.publish(EventPayload::new(
        event_type::SESSION_CREATED,
        json!({ "sessionID": id, "info": info }),
    ));
    Ok(Json(info))
}

pub(crate) async fn session_get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionInfo>, ApiError> {
    let info = state
        .inner
        .store
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Session not found"))?;
    Ok(Json(info))
}

pub(crate) async fn session_delete(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<bool>, ApiError> {
    if !state.inner.store.delete_session(&session_id).await? {
        return Err(ApiError::not_found("Session not found"));
    }
    state.inner.statuses.write().await.remove(&session_id);
    state.publish(EventPayload::new(
        event_type::SESSION_DELETED,
        json!({ "sessionID": session_id }),
    ));
    Ok(Json(true))
}

pub(crate) async fn session_update(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(update): Json<SessionUpdateRequest>,
) -> Result<Json<SessionInfo>, ApiError> {
    let mut info = state
        .inner
        .store
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Session not found"))?;
    if let Some(title) = update.title {
        info.title = title;
    }
    if let Some(agent) = update.agent {
        info.agent = Some(agent);
    }
    if let Some(permission) = update.permission {
        info.permission = Some(permission);
    }
    if let Some(model) = update.model {
        info.model = Some(model);
    }
    if let Some(time) = update.time {
        if let Some(archived) = time.archived {
            info.time.archived = Some(archived);
        }
    }
    info.time.updated = now_millis();
    state.inner.store.update_session(&info).await?;
    state.publish(EventPayload::new(
        event_type::SESSION_UPDATED,
        json!({ "sessionID": session_id, "info": info }),
    ));
    Ok(Json(info))
}

pub(crate) async fn session_children(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<SessionInfo>>, ApiError> {
    crate::ensure_session(&state, &session_id).await?;
    Ok(Json(
        state
            .inner
            .store
            .list_sessions()
            .await?
            .into_iter()
            .filter(|session| {
                session.parent_id.as_ref().map(|id| id.as_str())
                    == Some(session_id.as_str())
            })
            .collect(),
    ))
}

pub(crate) async fn session_todo_list(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<TodoInfo>>, ApiError> {
    crate::ensure_session(&state, &session_id).await?;
    Ok(Json(
        state
            .inner
            .todos
            .read()
            .await
            .get(&session_id)
            .cloned()
            .unwrap_or_default(),
    ))
}

pub(crate) async fn session_fork(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    body: Option<Json<ForkSessionRequest>>,
) -> Result<Json<SessionInfo>, ApiError> {
    let parent = crate::ensure_session(&state, &session_id).await?;
    let now = now_millis();
    let child_id = neoism_agent_core::new_session_id();
    let child = SessionInfo {
        id: child_id.clone(),
        slug: slug(),
        project_id: parent.project_id,
        workspace_id: parent.workspace_id,
        directory: parent.directory,
        path: parent.path,
        parent_id: Some(parent.id),
        title: format!("Fork - {}", parent.title),
        agent: parent.agent,
        model: parent.model,
        version: env!("CARGO_PKG_VERSION").to_string(),
        time: TimeInfo {
            created: now,
            updated: now,
            compacting: None,
            archived: None,
        },
        permission: parent.permission,
        extra: parent.extra,
    };
    state.inner.store.insert_session(&child).await?;
    let cutoff = body.and_then(|Json(body)| body.message_id.map(|id| id.to_string()));
    for message in state.inner.store.list_messages(&session_id).await? {
        let original_id = message_id_of(&message);
        let retargeted = retarget_message(message, &child_id);
        state
            .inner
            .store
            .append_message(child_id.as_str(), &retargeted)
            .await?;
        if cutoff.as_deref() == Some(original_id.as_str()) {
            break;
        }
    }
    state.publish(EventPayload::new(
        event_type::SESSION_CREATED,
        json!({ "sessionID": child_id, "info": child }),
    ));
    Ok(Json(child))
}

pub(crate) async fn session_status(
    State(state): State<AppState>,
) -> Json<HashMap<String, Value>> {
    let statuses = state.inner.statuses.read().await.clone();
    let runs = state.inner.runs.read().await.clone();
    let mut out = HashMap::new();
    for (session_id, status) in statuses {
        let mut value = json!(status);
        if let Some(run) = runs.get(&session_id) {
            value["runID"] = json!(run.id);
            value["startedAt"] = json!(run.started_at);
        }
        if let Ok(Some(session)) = state.inner.store.get_session(&session_id).await {
            if let Some(parent_id) = session.parent_id.as_ref() {
                value["parentSessionID"] = json!(parent_id);
                value["sourceSessionID"] = json!(session.id.to_string());
                value["sourceTitle"] = json!(session.title);
                if let Some(agent) = session.agent.as_ref() {
                    value["sourceAgent"] = json!(agent);
                }
            }
        }
        out.insert(session_id, value);
    }
    Json(out)
}

pub(crate) async fn session_share(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionInfo>, ApiError> {
    let mut info = crate::ensure_session(&state, &session_id).await?;
    info.extra.insert(
        "share".to_string(),
        json!({ "url": format!("neoism://session/{}", session_id) }),
    );
    info.time.updated = now_millis();
    state.inner.store.update_session(&info).await?;
    Ok(Json(info))
}

pub(crate) async fn session_unshare(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionInfo>, ApiError> {
    let mut info = crate::ensure_session(&state, &session_id).await?;
    info.extra.remove("share");
    info.time.updated = now_millis();
    state.inner.store.update_session(&info).await?;
    Ok(Json(info))
}

pub(crate) async fn session_diff(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<VcsFileDiff>>, ApiError> {
    let info = crate::ensure_session(&state, &session_id).await?;
    Ok(Json(vcs::diff(&info.directory)))
}

fn retarget_message(
    mut message: MessageWithParts,
    session_id: &neoism_agent_core::SessionId,
) -> MessageWithParts {
    let next_message_id = Id::ascending(IdKind::Message);
    match &mut message.info {
        MessageInfo::User(info) => {
            info.id = next_message_id.clone();
            info.session_id = session_id.clone();
        }
        MessageInfo::Assistant(info) => {
            info.id = next_message_id.clone();
            info.session_id = session_id.clone();
        }
    }
    for part in &mut message.parts {
        match part {
            Part::Text(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::Compaction(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::Agent(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::Subtask(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::Reasoning(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::Tool(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::StepStart(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::StepFinish(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
            Part::File(part) => {
                part.id = Id::ascending(IdKind::Part);
                part.session_id = session_id.clone();
                part.message_id = next_message_id.clone();
            }
        }
    }
    message
}
