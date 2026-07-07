use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};

use super::client::{
    emit, AcpDebugDirection, AcpRpcError, AcpShared, AcpUiEvent, JSONRPC, PROMPT_TIMEOUT,
};
use super::paths::workspace_path;

pub(super) fn handle_incoming(
    server_id: &str,
    cwd: &Path,
    value: Value,
    shared: &Arc<AcpShared>,
) {
    let method = value.get("method").and_then(Value::as_str);
    let id = value.get("id").cloned();

    match (id, method) {
        (Some(id), Some(method)) => {
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            handle_agent_request(server_id, cwd, id, method, params, shared);
        }
        (Some(id), None) => handle_agent_response(id, value, shared),
        (None, Some(method)) => {
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            handle_agent_notification(server_id, method, params, shared);
        }
        (None, None) => emit(
            &shared.ui_tx,
            &shared.wake,
            AcpUiEvent::Error {
                server_id: server_id.to_string(),
                message: "ACP message had neither id nor method".to_string(),
            },
        ),
    }
}

pub(super) fn handle_agent_response(id: Value, value: Value, shared: &Arc<AcpShared>) {
    let Some(id) = id.as_u64() else {
        return;
    };
    let Some(tx) = shared
        .pending
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&id))
    else {
        return;
    };

    if let Some(result) = value.get("result") {
        let _ = tx.send(Ok(result.clone()));
    } else if let Some(error) = value.get("error") {
        let _ = tx.send(Err(AcpRpcError {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(-32000),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("ACP request failed")
                .to_string(),
        }));
    }
}

pub(super) fn handle_agent_notification(
    server_id: &str,
    method: &str,
    params: Value,
    shared: &Arc<AcpShared>,
) {
    match method {
        "session/update" => {
            let session_id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let update = params.get("update").cloned().unwrap_or(Value::Null);
            emit(
                &shared.ui_tx,
                &shared.wake,
                AcpUiEvent::SessionUpdate {
                    server_id: server_id.to_string(),
                    session_id,
                    update,
                },
            );
        }
        _ => {
            emit(
                &shared.ui_tx,
                &shared.wake,
                AcpUiEvent::DebugLine {
                    server_id: server_id.to_string(),
                    direction: AcpDebugDirection::Incoming,
                    line: format!("notification {method}: {params}"),
                },
            );
        }
    }
}

pub(super) fn handle_agent_request(
    server_id: &str,
    cwd: &Path,
    id: Value,
    method: &str,
    params: Value,
    shared: &Arc<AcpShared>,
) {
    let response = match method {
        "fs/read_text_file" => handle_read_text_file(server_id, cwd, &params, shared),
        "fs/write_text_file" => handle_write_text_file(server_id, cwd, &params, shared),
        "session/request_permission" => {
            handle_request_permission(server_id, &params, shared)
        }
        "terminal/create" => shared.terminals.create(server_id, &params, shared),
        "terminal/output" => shared.terminals.output(&params),
        "terminal/wait_for_exit" => shared.terminals.wait_for_exit(&params),
        "terminal/kill" => shared.terminals.kill(&params),
        "terminal/release" => shared.terminals.release(&params),
        _ => Err(AcpRpcError {
            code: -32601,
            message: format!("Unsupported ACP client method `{method}`"),
        }),
    };

    let message = match response {
        Ok(result) => json!({
            "jsonrpc": JSONRPC,
            "id": id,
            "result": result,
        }),
        Err(err) => json!({
            "jsonrpc": JSONRPC,
            "id": id,
            "error": {
                "code": err.code,
                "message": err.message,
            },
        }),
    };

    if let Ok(line) = serde_json::to_string(&message) {
        emit(
            &shared.ui_tx,
            &shared.wake,
            AcpUiEvent::DebugLine {
                server_id: server_id.to_string(),
                direction: AcpDebugDirection::Outgoing,
                line: line.clone(),
            },
        );
        let _ = shared.write_tx.send(line);
    }
}

pub(super) fn handle_read_text_file(
    server_id: &str,
    cwd: &Path,
    params: &Value,
    shared: &Arc<AcpShared>,
) -> Result<Value, AcpRpcError> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let path = workspace_path(cwd, params.get("path").and_then(Value::as_str))?;
    let mut content = fs::read_to_string(&path).map_err(|err| AcpRpcError {
        code: -32000,
        message: format!("Could not read {}: {err}", path.display()),
    })?;

    let start_line = params.get("line").and_then(Value::as_u64).unwrap_or(1);
    let limit = params.get("limit").and_then(Value::as_u64);
    if start_line > 1 || limit.is_some() {
        let start = start_line.saturating_sub(1) as usize;
        let mut lines = content.lines().skip(start);
        content = if let Some(limit) = limit {
            lines
                .by_ref()
                .take(limit as usize)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            lines.collect::<Vec<_>>().join("\n")
        };
    }

    emit(
        &shared.ui_tx,
        &shared.wake,
        AcpUiEvent::FileRead {
            server_id: server_id.to_string(),
            session_id,
            path,
        },
    );
    Ok(json!({ "content": content }))
}

pub(super) fn handle_write_text_file(
    server_id: &str,
    cwd: &Path,
    params: &Value,
    shared: &Arc<AcpShared>,
) -> Result<Value, AcpRpcError> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let path = workspace_path(cwd, params.get("path").and_then(Value::as_str))?;
    let content = params
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| AcpRpcError {
            code: -32602,
            message: "fs/write_text_file missing content".to_string(),
        })?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| AcpRpcError {
            code: -32000,
            message: format!("Could not create {}: {err}", parent.display()),
        })?;
    }
    fs::write(&path, content).map_err(|err| AcpRpcError {
        code: -32000,
        message: format!("Could not write {}: {err}", path.display()),
    })?;

    emit(
        &shared.ui_tx,
        &shared.wake,
        AcpUiEvent::FileWritten {
            server_id: server_id.to_string(),
            session_id,
            path,
            bytes: content.len(),
        },
    );
    Ok(Value::Null)
}

pub(super) fn handle_request_permission(
    server_id: &str,
    params: &Value,
    shared: &Arc<AcpShared>,
) -> Result<Value, AcpRpcError> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
    let options = params.get("options").cloned().unwrap_or(Value::Null);
    let option_ids = options
        .as_array()
        .map(|options| {
            options
                .iter()
                .filter_map(|option| option.get("optionId").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let request_id = shared.next_permission_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    if let Ok(mut pending) = shared.pending_permissions.lock() {
        pending.insert(request_id, tx);
    } else {
        return Ok(json!({
            "outcome": {
                "outcome": "cancelled",
            }
        }));
    }

    emit(
        &shared.ui_tx,
        &shared.wake,
        AcpUiEvent::PermissionRequested {
            server_id: server_id.to_string(),
            session_id,
            request_id,
            tool_call,
            options,
        },
    );

    let selected = rx.recv_timeout(PROMPT_TIMEOUT).ok().flatten();
    let _ = shared
        .pending_permissions
        .lock()
        .map(|mut pending| pending.remove(&request_id));
    let selected = selected.filter(|selected| option_ids.contains(selected));

    match selected {
        Some(option_id) => Ok(json!({
            "outcome": {
                "outcome": "selected",
                "optionId": option_id,
            }
        })),
        None => Ok(json!({
            "outcome": {
                "outcome": "cancelled",
            }
        })),
    }
}
