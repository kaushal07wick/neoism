use neoism_agent_core::{Id, Part, PartTime, ToolPart, ToolState};
use serde_json::{json, Value};

use crate::now_millis;

pub(crate) fn append_text_delta(parts: &mut [Part], part_id: &str, delta: &str) {
    for part in parts {
        match part {
            Part::Text(text) if text.id.as_str() == part_id => {
                text.text.push_str(delta);
                return;
            }
            Part::Reasoning(reasoning) if reasoning.id.as_str() == part_id => {
                reasoning.text.push_str(delta);
                return;
            }
            _ => {}
        }
    }
}

pub(crate) fn finish_text_part(
    parts: &mut [Part],
    part_id: &str,
    text: Option<String>,
) -> Option<Part> {
    for part in parts {
        match part {
            Part::Text(text_part) if text_part.id.as_str() == part_id => {
                if let Some(text) = text {
                    text_part.text = text;
                }
                if let Some(time) = &mut text_part.time {
                    time.end = Some(now_millis());
                }
                return Some(Part::Text(text_part.clone()));
            }
            Part::Reasoning(reasoning) if reasoning.id.as_str() == part_id => {
                if let Some(text) = text {
                    reasoning.text = text;
                }
                reasoning.time.end = Some(now_millis());
                return Some(Part::Reasoning(reasoning.clone()));
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn mark_interrupted_tool_parts(parts: &mut [Part]) -> Vec<Part> {
    let mut updated = Vec::new();
    for part in parts {
        let Part::Tool(tool) = part else {
            continue;
        };
        if !matches!(
            tool.state,
            ToolState::Pending { .. } | ToolState::Running { .. }
        ) {
            continue;
        }
        let input = tool_state_input(&tool.state);
        let start = tool_state_start(&tool.state).unwrap_or_else(now_millis);
        tool.state = ToolState::Error {
            input,
            error: "Tool execution aborted".to_string(),
            time: PartTime {
                start,
                end: Some(now_millis()),
            },
        };
        tool.metadata = Some(interrupted_tool_metadata(tool.metadata.take()));
        updated.push(Part::Tool(tool.clone()));
    }
    updated
}

fn interrupted_tool_metadata(existing: Option<Value>) -> Value {
    let mut metadata = match existing {
        Some(Value::Object(map)) => Value::Object(map),
        Some(value) => json!({ "previous": value }),
        None => json!({}),
    };
    if let Some(map) = metadata.as_object_mut() {
        map.insert("interrupted".to_string(), json!(true));
    }
    metadata
}

pub(crate) fn upsert_part(parts: &mut Vec<Part>, part: Part) {
    let id = part_id(&part).to_string();
    if let Some(existing) = parts.iter_mut().find(|item| part_id(item) == id) {
        *existing = part;
        return;
    }
    parts.push(part);
}

fn part_id(part: &Part) -> &str {
    match part {
        Part::Text(part) => part.id.as_str(),
        Part::Compaction(part) => part.id.as_str(),
        Part::Agent(part) => part.id.as_str(),
        Part::Subtask(part) => part.id.as_str(),
        Part::Reasoning(part) => part.id.as_str(),
        Part::Tool(part) => part.id.as_str(),
        Part::StepStart(part) => part.id.as_str(),
        Part::StepFinish(part) => part.id.as_str(),
        Part::File(part) => part.id.as_str(),
    }
}

pub(crate) fn append_tool_input_delta(
    parts: &mut [Part],
    part_id: &str,
    delta: &str,
) -> Option<Part> {
    for part in parts {
        if let Part::Tool(tool) = part {
            if tool.id.as_str() == part_id {
                if let ToolState::Pending { raw, .. } = &mut tool.state {
                    raw.push_str(delta);
                }
                return Some(Part::Tool(tool.clone()));
            }
        }
    }
    None
}

pub(crate) fn set_tool_running(
    parts: &mut Vec<Part>,
    part_id: Id,
    session_id: &Id,
    message_id: &Id,
    call_id: String,
    name: String,
    input: Value,
) -> Part {
    let part_id_text = part_id.to_string();
    for part in parts.iter_mut() {
        if let Part::Tool(tool) = part {
            if tool.id.as_str() == part_id_text {
                tool.tool = name;
                tool.call_id = call_id;
                tool.state = ToolState::Running {
                    input,
                    time: PartTime {
                        start: now_millis(),
                        end: None,
                    },
                };
                return Part::Tool(tool.clone());
            }
        }
    }
    let part = Part::Tool(ToolPart {
        id: part_id,
        session_id: session_id.clone(),
        message_id: message_id.clone(),
        tool: name,
        call_id,
        state: ToolState::Running {
            input,
            time: PartTime {
                start: now_millis(),
                end: None,
            },
        },
        metadata: None,
    });
    parts.push(part.clone());
    part
}

pub(crate) fn set_tool_completed(
    parts: &mut [Part],
    part_id: &str,
    output: String,
    title: String,
    metadata: Value,
) -> Option<Part> {
    for part in parts {
        if let Part::Tool(tool) = part {
            if tool.id.as_str() == part_id {
                let input = tool_state_input(&tool.state);
                let start = tool_state_start(&tool.state).unwrap_or_else(now_millis);
                let metadata =
                    stable_tool_metadata(metadata, &tool.tool, &title, &output);
                tool.state = ToolState::Completed {
                    input,
                    output,
                    metadata,
                    title,
                    time: PartTime {
                        start,
                        end: Some(now_millis()),
                    },
                };
                return Some(Part::Tool(tool.clone()));
            }
        }
    }
    None
}

fn stable_tool_metadata(metadata: Value, tool: &str, title: &str, output: &str) -> Value {
    let mut metadata = match metadata {
        Value::Object(object) => object,
        other => {
            let mut object = serde_json::Map::new();
            object.insert("raw".to_string(), other);
            object
        }
    };
    let has_snapshots = metadata.get("snapshots").is_some();
    let kind = if has_snapshots {
        "diff"
    } else if metadata.get("lsp").is_some() {
        "lsp"
    } else if metadata.get("todos").is_some() {
        "todo"
    } else {
        "text"
    };
    metadata.entry("render".to_string()).or_insert_with(|| {
        json!({
            "version": 1,
            "tool": tool,
            "title": title,
            "lineCount": output.lines().count(),
            "byteCount": output.len(),
            "hasSnapshots": has_snapshots,
            "kind": kind,
        })
    });
    Value::Object(metadata)
}

pub(crate) fn set_tool_error(
    parts: &mut [Part],
    part_id: &str,
    error: String,
) -> Option<Part> {
    for part in parts {
        if let Part::Tool(tool) = part {
            if tool.id.as_str() == part_id {
                let input = tool_state_input(&tool.state);
                let start = tool_state_start(&tool.state).unwrap_or_else(now_millis);
                tool.state = ToolState::Error {
                    input,
                    error,
                    time: PartTime {
                        start,
                        end: Some(now_millis()),
                    },
                };
                return Some(Part::Tool(tool.clone()));
            }
        }
    }
    None
}

pub(crate) fn tool_state_input(state: &ToolState) -> Value {
    match state {
        ToolState::Pending { input, .. }
        | ToolState::Running { input, .. }
        | ToolState::Completed { input, .. }
        | ToolState::Error { input, .. } => input.clone(),
    }
}

pub(crate) fn tool_state_start(state: &ToolState) -> Option<u64> {
    match state {
        ToolState::Running { time, .. }
        | ToolState::Completed { time, .. }
        | ToolState::Error { time, .. } => Some(time.start),
        ToolState::Pending { .. } => None,
    }
}
