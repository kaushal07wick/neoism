use neoism_agent_core::{McpPromptArgument, McpPromptInfo, McpResource, McpToolInfo};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolWire {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    input_schema: Value,
    #[serde(default)]
    annotations: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ToolsListWire {
    #[serde(default)]
    tools: Vec<ToolWire>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceWire {
    name: String,
    uri: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResourcesListWire {
    #[serde(default)]
    resources: Vec<ResourceWire>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptWire {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    arguments: Vec<McpPromptArgument>,
}

#[derive(Debug, Deserialize)]
struct PromptsListWire {
    #[serde(default)]
    prompts: Vec<PromptWire>,
}

pub(super) fn parse_tools(client: &str, value: Value) -> Vec<McpToolInfo> {
    serde_json::from_value::<ToolsListWire>(value)
        .map(|list| {
            list.tools
                .into_iter()
                .map(|tool| McpToolInfo {
                    name: tool.name,
                    description: tool.description,
                    input_schema: if tool.input_schema.is_null() {
                        json!({ "type": "object" })
                    } else {
                        tool.input_schema
                    },
                    client: client.to_string(),
                    annotations: tool.annotations,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn parse_resources(client: &str, value: Value) -> Vec<McpResource> {
    serde_json::from_value::<ResourcesListWire>(value)
        .map(|list| {
            list.resources
                .into_iter()
                .map(|resource| McpResource {
                    name: resource.name,
                    uri: resource.uri,
                    description: resource.description,
                    mime_type: resource.mime_type,
                    client: client.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn parse_prompts(client: &str, value: Value) -> Vec<McpPromptInfo> {
    serde_json::from_value::<PromptsListWire>(value)
        .map(|list| {
            list.prompts
                .into_iter()
                .map(|prompt| McpPromptInfo {
                    name: prompt.name,
                    description: prompt.description,
                    arguments: prompt.arguments,
                    client: client.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}
