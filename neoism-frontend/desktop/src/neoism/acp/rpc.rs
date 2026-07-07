use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{mpsc, Arc};
use std::thread;

use super::client::{emit, AcpDebugDirection, AcpRpcError, AcpShared, AcpUiEvent};
use super::handlers::handle_incoming;

pub(super) fn spawn_writer(
    server_id: String,
    mut stdin: std::process::ChildStdin,
    write_rx: mpsc::Receiver<String>,
    ui_tx: mpsc::Sender<AcpUiEvent>,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
) {
    thread::Builder::new()
        .name(format!("neoism-acp-{server_id}-writer"))
        .spawn(move || {
            for line in write_rx {
                if let Err(err) = stdin.write_all(line.as_bytes()) {
                    emit(
                        &ui_tx,
                        &wake,
                        AcpUiEvent::Error {
                            server_id: server_id.clone(),
                            message: format!("ACP write failed: {err}"),
                        },
                    );
                    break;
                }
                if let Err(err) = stdin.write_all(b"\n") {
                    emit(
                        &ui_tx,
                        &wake,
                        AcpUiEvent::Error {
                            server_id: server_id.clone(),
                            message: format!("ACP newline write failed: {err}"),
                        },
                    );
                    break;
                }
                if let Err(err) = stdin.flush() {
                    emit(
                        &ui_tx,
                        &wake,
                        AcpUiEvent::Error {
                            server_id: server_id.clone(),
                            message: format!("ACP flush failed: {err}"),
                        },
                    );
                    break;
                }
            }
        })
        .ok();
}

pub(super) fn spawn_reader(
    server_id: String,
    cwd: PathBuf,
    stdout: std::process::ChildStdout,
    shared: Arc<AcpShared>,
) {
    thread::Builder::new()
        .name(format!("neoism-acp-{server_id}-reader"))
        .spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(err) => {
                        emit(
                            &shared.ui_tx,
                            &shared.wake,
                            AcpUiEvent::Error {
                                server_id: server_id.clone(),
                                message: format!("ACP read failed: {err}"),
                            },
                        );
                        break;
                    }
                };
                if line.trim().is_empty() {
                    continue;
                }
                emit(
                    &shared.ui_tx,
                    &shared.wake,
                    AcpUiEvent::DebugLine {
                        server_id: server_id.clone(),
                        direction: AcpDebugDirection::Incoming,
                        line: line.clone(),
                    },
                );
                match serde_json::from_str::<Value>(&line) {
                    Ok(value) => handle_incoming(&server_id, &cwd, value, &shared),
                    Err(err) => emit(
                        &shared.ui_tx,
                        &shared.wake,
                        AcpUiEvent::Error {
                            server_id: server_id.clone(),
                            message: format!("ACP JSON parse failed: {err}: {line}"),
                        },
                    ),
                }
            }
        })
        .ok();
}

pub(super) fn spawn_stderr(
    server_id: String,
    stderr: std::process::ChildStderr,
    ui_tx: mpsc::Sender<AcpUiEvent>,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
) {
    thread::Builder::new()
        .name(format!("neoism-acp-{server_id}-stderr"))
        .spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                emit(
                    &ui_tx,
                    &wake,
                    AcpUiEvent::Stderr {
                        server_id: server_id.clone(),
                        line,
                    },
                );
            }
        })
        .ok();
}

pub(super) fn spawn_waiter(server_id: String, mut child: Child, shared: Arc<AcpShared>) {
    thread::Builder::new()
        .name(format!("neoism-acp-{server_id}-wait"))
        .spawn(move || {
            let status = child.wait().ok().and_then(|status| status.code());
            if let Ok(mut pending) = shared.pending.lock() {
                for (_, tx) in pending.drain() {
                    let _ = tx.send(Err(AcpRpcError {
                        code: -32000,
                        message: "ACP server exited".to_string(),
                    }));
                }
            }
            emit(
                &shared.ui_tx,
                &shared.wake,
                AcpUiEvent::Exited { server_id, status },
            );
        })
        .ok();
}
