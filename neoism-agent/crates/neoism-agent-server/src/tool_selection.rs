use std::collections::HashSet;

use neoism_agent_core::ToolListItem;
use serde_json::Value;

pub(crate) fn provider_tool_id_set(tools: &[ToolListItem]) -> HashSet<String> {
    tools.iter().map(|tool| tool.id.clone()).collect()
}

pub(crate) fn normalize_provider_tool_name(
    name: &str,
    input: &Value,
    available: &HashSet<String>,
) -> Option<String> {
    if available.contains(name) {
        return Some(name.to_string());
    }
    if name == "patch" && available.contains("apply_patch") {
        return Some("apply_patch".to_string());
    }
    if name == "edit"
        && available.contains("apply_patch")
        && patch_text_arg_value(input).is_some()
    {
        return Some("apply_patch".to_string());
    }
    None
}

fn patch_text_arg_value(input: &Value) -> Option<&str> {
    input
        .get("patchText")
        .or_else(|| input.get("patch"))
        .or_else(|| input.get("diff"))
        .or_else(|| input.get("content"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn tool_allowed_for_model(tool_id: &str, model_id: &str) -> bool {
    if use_apply_patch_for_model(model_id) {
        tool_id != "write"
    } else {
        tool_id != "apply_patch"
    }
}

pub(crate) fn use_apply_patch_for_model(model_id: &str) -> bool {
    let model_id = model_id.to_ascii_lowercase();
    is_openai_patch_model(&model_id)
}

fn is_openai_patch_model(model_id: &str) -> bool {
    if model_id.contains("oss") || model_id.contains("gpt-4") {
        return false;
    }
    if model_id == "stub" {
        return false;
    }
    model_id
        .split('/')
        .any(|part| part.starts_with("gpt-5") || part.contains("codex"))
}
