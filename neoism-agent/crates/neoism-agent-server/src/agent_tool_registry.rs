use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use neoism_agent_core::{McpToolInfo, PermissionRule, ToolListItem};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::session_loop::wait_for_cancellation;
use crate::state::AppState;
use crate::{
    config, ensure_tool_permission, mcp, mcp_auth, permission, tool,
    tool_allowed_for_model,
};

pub(crate) async fn configured_mcp_tools_with_state(
    directory: &str,
    state: Option<AppState>,
) -> Vec<McpToolInfo> {
    let names = config::load(directory)
        .map(|loaded| loaded.info.mcp.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut tools = Vec::new();
    for name in names {
        let Ok(mut items) = mcp::tools_with_state(
            directory,
            &name,
            &mcp_auth::McpAuthStore::from_env(),
            state.clone(),
        )
        .await
        else {
            continue;
        };
        tools.append(&mut items);
    }
    tools
}

pub(crate) async fn available_tools_for_directory(
    state: &AppState,
    directory: &str,
) -> Result<Vec<ToolListItem>, ApiError> {
    if let Ok(loaded) = config::load(directory) {
        state
            .inner
            .plugins
            .register_configured_plugins(&loaded.info, directory);
    }
    let mut tools = tool::list();
    tools.extend(crate::custom_tool::list(directory));
    tools.extend(
        configured_mcp_tools_with_state(directory, Some(state.clone()))
            .await
            .into_iter()
            .map(mcp_tool_list_item),
    );
    for tool in &mut tools {
        state
            .inner
            .plugins
            .tool_definition(tool)
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
    }
    tools.sort_by(|left, right| left.id.cmp(&right.id));
    tools.dedup_by(|left, right| left.id == right.id);
    Ok(tools)
}

pub(crate) async fn provider_tools_for_agent(
    state: &AppState,
    directory: &str,
    permissions: &[PermissionRule],
    model_id: &str,
) -> Result<Vec<ToolListItem>, ApiError> {
    let tools = available_tools_for_directory(state, directory).await?;
    let ids = tools.iter().map(|tool| tool.id.clone()).collect::<Vec<_>>();
    let disabled = permission::disabled(&ids, permissions);
    Ok(tools
        .into_iter()
        .filter(|tool| !disabled.contains(&tool.id))
        .filter(|tool| tool_allowed_for_model(&tool.id, model_id))
        .collect())
}

fn mcp_tool_list_item(tool: McpToolInfo) -> ToolListItem {
    ToolListItem {
        id: mcp::tool_runtime_id(&tool.client, &tool.name),
        description: tool
            .description
            .unwrap_or_else(|| format!("MCP tool {} from {}", tool.name, tool.client)),
        parameters: tool.input_schema,
    }
}

pub(crate) async fn execute_mcp_tool_by_runtime_id(
    directory: &str,
    runtime_id: &str,
    arguments: Value,
    permissions: &[PermissionRule],
    cancel: Option<Arc<AtomicBool>>,
    state: Option<AppState>,
) -> anyhow::Result<Option<tool::ToolExecutionResult>> {
    if !runtime_id.starts_with("mcp__") {
        return Ok(None);
    }
    ensure_tool_permission(permissions, "mcp", runtime_id)
        .map_err(|error| anyhow::anyhow!(error))?;
    let Some(tool) = configured_mcp_tools_with_state(directory, state.clone())
        .await
        .into_iter()
        .find(|tool| mcp::tool_runtime_id(&tool.client, &tool.name) == runtime_id)
    else {
        anyhow::bail!("unknown MCP tool {runtime_id}");
    };
    let auth_store = mcp_auth::McpAuthStore::from_env();
    let call = mcp::call_tool_with_state(
        directory,
        &tool.client,
        &tool.name,
        arguments,
        &auth_store,
        state,
    );
    let result = if let Some(cancel) = cancel {
        tokio::select! {
            result = call => result?,
            _ = wait_for_cancellation(cancel) => {
                anyhow::bail!("MCP tool call aborted");
            }
        }
    } else {
        call.await?
    };
    let output = mcp::tool_result_text(&result);
    if result.is_error.unwrap_or(false) {
        anyhow::bail!("MCP tool {} returned an error\n{}", tool.name, output);
    }
    Ok(Some(tool::ToolExecutionResult {
        title: format!("MCP {}.{}", tool.client, tool.name),
        output,
        metadata: Some(json!({
            "mcp": {
                "client": tool.client,
                "tool": tool.name,
                "runtimeId": runtime_id,
                "result": result,
            }
        })),
    }))
}
