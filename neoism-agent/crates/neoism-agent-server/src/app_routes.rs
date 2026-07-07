use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use neoism_agent_core::{AgentInfo, PluginStatusInfo, SkillInfo};

use crate::agent::AgentCatalog;
use crate::error::ApiError;
use crate::state::AppState;
use crate::{config, resolve_directory, skill, InstanceQuery};

pub(crate) async fn agent_list(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<AgentInfo>>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    Ok(Json(AgentCatalog::load(&directory)?.list()))
}

pub(crate) async fn agent_get(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<AgentInfo>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    AgentCatalog::load(&directory)?
        .get(&name)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Agent not found"))
}

pub(crate) async fn skill_list(
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<SkillInfo>>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    Ok(Json(skill::list_async(&directory).await?))
}

pub(crate) async fn plugin_status(
    State(state): State<AppState>,
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<PluginStatusInfo>>, ApiError> {
    let directory = resolve_directory(query.directory, &headers);
    let loaded = config::load(&directory)?;
    state
        .inner
        .plugins
        .register_configured_plugins(&loaded.info, &directory);
    Ok(Json(state.inner.plugins.statuses()))
}
