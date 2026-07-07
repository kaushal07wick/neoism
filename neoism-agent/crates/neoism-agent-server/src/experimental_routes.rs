use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use neoism_agent_core::{McpResource, SessionInfo};

use crate::error::ApiError;
use crate::state::AppState;
use crate::{
    config, filter_sessions, mcp, mcp_auth, resolve_directory, InstanceQuery,
    SessionListQuery,
};

pub(crate) async fn experimental_session_list(
    State(state): State<AppState>,
    Query(query): Query<SessionListQuery>,
) -> Result<Json<Vec<SessionInfo>>, ApiError> {
    let mut sessions = state.inner.store.list_sessions().await?;
    filter_sessions(&mut sessions, &query);
    Ok(Json(sessions))
}

pub(crate) async fn resource_list(
    State(state): State<AppState>,
    Query(query): Query<InstanceQuery>,
    headers: HeaderMap,
) -> Json<BTreeMap<String, McpResource>> {
    let directory = resolve_directory(query.directory, &headers);
    let mut resources = BTreeMap::new();
    let names = config::load(&directory)
        .map(|loaded| loaded.info.mcp.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for name in names {
        let Ok(items) = mcp::resources_with_state(
            &directory,
            &name,
            &mcp_auth::McpAuthStore::from_env(),
            Some(state.clone()),
        )
        .await
        else {
            continue;
        };
        for resource in items {
            resources.insert(format!("{}/{}", resource.client, resource.uri), resource);
        }
    }
    Json(resources)
}
