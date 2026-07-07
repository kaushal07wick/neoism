use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use crate::managed_lsp_path::managed_lsp_path;

use super::{capability_enabled, path_to_file_uri, INITIALIZE_TIMEOUT, SHUTDOWN_TIMEOUT};

pub(crate) struct InitializeResult {
    pub(crate) workspace_symbol_provider: bool,
    pub(crate) completion_provider: bool,
    pub(crate) completion_trigger_characters: Vec<String>,
    pub(crate) hover_provider: bool,
    pub(crate) definition_provider: bool,
    pub(crate) references_provider: bool,
    pub(crate) implementation_provider: bool,
    pub(crate) call_hierarchy_provider: bool,
    pub(crate) document_symbol_provider: bool,
    pub(crate) formatting_provider: bool,
    pub(crate) code_action_provider: bool,
    pub(crate) rename_provider: bool,
    pub(crate) diagnostic_provider: bool,
}

pub(crate) struct StdioLspClient {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    next_id: i64,
    label: String,
}

impl StdioLspClient {
    pub(crate) fn spawn(root: &Path, command: &[String]) -> Result<Self> {
        Self::spawn_with_env(root, "", command, &BTreeMap::new())
    }

    pub(crate) fn spawn_with_env(
        root: &Path,
        language: &str,
        command: &[String],
        env: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let (program, args) = command
            .split_first()
            .ok_or_else(|| anyhow!("LSP command is empty"))?;
        let mut command_proc = Command::new(program);
        command_proc.args(args).current_dir(root).envs(env);
        if !env.contains_key("PATH") {
            if let Some(path) = managed_lsp_path() {
                command_proc.env("PATH", path);
            }
        }
        let label = command.join(" ");
        let spawn_started = crate::perf::now();
        tracing::info!(
            target: "neoism_agent::perf",
            command = %program,
            args = ?args,
            root = %root.display(),
            "lsp process spawning"
        );
        let mut child = command_proc
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!("failed to spawn LSP command `{}`", command.join(" "))
            })?;
        tracing::info!(
            target: "neoism_agent::perf",
            lsp = %label,
            pid = child.id(),
            spawn_ms = crate::perf::elapsed_ms(spawn_started),
            "lsp process spawned"
        );

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("failed to capture LSP stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("failed to capture LSP stdout"))?;
        let (sender, messages) = mpsc::channel();

        let reader_root = root.to_path_buf();
        let reader_language = language.to_string();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_lsp_message(&mut reader) {
                    Ok(Some(message)) => {
                        let method = message.get("method").and_then(Value::as_str);
                        // Tap `publishDiagnostics` for the real-time event bus
                        // BEFORE handing the message to the request/response
                        // channel (where notifications would be discarded).
                        if method == Some("textDocument/publishDiagnostics") {
                            if let Some(params) = message.get("params") {
                                crate::lsp::dispatch_diagnostics(
                                    &reader_root,
                                    &reader_language,
                                    params,
                                );
                            }
                        }
                        // Surface indexing/loading progress so it's clear
                        // rust-analyzer is warming up (completions/diagnostics
                        // only start once the workspace is loaded — minutes on a
                        // big project, especially first build-script/proc-macro
                        // compile).
                        if std::env::var_os("NEOISM_LSP_LOG").is_some() {
                            if method == Some("$/progress") {
                                if let Some(value) = message.pointer("/params/value") {
                                    let kind = value
                                        .get("kind")
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    let title = value
                                        .get("title")
                                        .or_else(|| value.get("message"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    let pct =
                                        value.get("percentage").and_then(Value::as_u64);
                                    eprintln!(
                                        "neoism::lsp progress[{reader_language}]: {kind} {title} {pct:?}"
                                    );
                                }
                            } else if method == Some("window/logMessage")
                                || method == Some("window/showMessage")
                            {
                                if let Some(text) = message
                                    .pointer("/params/message")
                                    .and_then(Value::as_str)
                                {
                                    eprintln!(
                                        "neoism::lsp server[{reader_language}]: {text}"
                                    );
                                }
                            }
                        }
                        if sender.send(message).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::debug!(error = %error, "failed to read LSP message");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            messages,
            next_id: 1,
            label,
        })
    }

    pub(crate) fn initialize(&mut self, root: &Path) -> Result<InitializeResult> {
        self.initialize_with_options(root, None)
    }

    pub(crate) fn initialize_with_options(
        &mut self,
        root: &Path,
        initialization_options: Option<Value>,
    ) -> Result<InitializeResult> {
        let root_uri = path_to_file_uri(root);
        let mut params = json!({
            "processId": null,
            "rootPath": root.display().to_string(),
            "rootUri": root_uri,
            "capabilities": {
                "general": {
                    // nvim reports cursor columns as UTF-8 BYTE offsets. Tell
                    // the server to use utf-8 positions so hover/completion land
                    // on the right column (default is utf-16 → wrong offset on
                    // any non-ASCII line).
                    "positionEncodings": ["utf-8"]
                },
                "window": {
                    // Let the server report indexing/build progress via
                    // `$/progress` so we can show it's warming up.
                    "workDoneProgress": true
                },
                "workspace": {
                    "symbol": {
                        "dynamicRegistration": false,
                        "symbolKind": {
                            "valueSet": (1..=26).collect::<Vec<u8>>()
                        }
                    }
                },
                "textDocument": {
                    "completion": {
                        "dynamicRegistration": false,
                        "contextSupport": true,
                        "completionItem": {
                            // Plain-text inserts for v1 — no snippet placeholder
                            // parsing; the label/insertText is inserted verbatim.
                            "snippetSupport": false,
                            "documentationFormat": ["markdown", "plaintext"],
                            "deprecatedSupport": true,
                            "preselectSupport": true
                        },
                        "completionItemKind": {
                            "valueSet": (1..=25).collect::<Vec<u8>>()
                        }
                    },
                    "hover": {
                        "dynamicRegistration": false,
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "definition": {
                        "dynamicRegistration": false,
                        "linkSupport": true
                    },
                    "references": {
                        "dynamicRegistration": false
                    },
                    "implementation": {
                        "dynamicRegistration": false,
                        "linkSupport": true
                    },
                    "callHierarchy": {
                        "dynamicRegistration": false
                    },
                    "documentSymbol": {
                        "dynamicRegistration": false,
                        "hierarchicalDocumentSymbolSupport": true,
                        "symbolKind": {
                            "valueSet": (1..=26).collect::<Vec<u8>>()
                        }
                    },
                    "formatting": {
                        "dynamicRegistration": false
                    },
                    "codeAction": {
                        "dynamicRegistration": false,
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": [
                                    "",
                                    "quickfix",
                                    "refactor",
                                    "refactor.extract",
                                    "refactor.inline",
                                    "refactor.rewrite",
                                    "source",
                                    "source.organizeImports"
                                ]
                            }
                        }
                    },
                    "synchronization": {
                        "didOpen": true,
                        "didChange": true,
                        "didSave": true,
                        "didClose": true
                    },
                    "diagnostic": {
                        "dynamicRegistration": false,
                        "relatedDocumentSupport": false
                    }
                }
            },
            "workspaceFolders": [{
                "uri": root_uri,
                "name": root.file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or("workspace")
            }]
        });
        let configuration_settings = initialization_options.clone();
        if let Some(options) = initialization_options {
            params["initializationOptions"] = options;
        }
        let initialized_started = crate::perf::now();
        let result = self.request("initialize", params, INITIALIZE_TIMEOUT)?;
        self.notify("initialized", json!({}))?;
        if let Some(settings) = configuration_settings {
            self.notify(
                "workspace/didChangeConfiguration",
                json!({ "settings": settings }),
            )?;
        }

        let workspace_symbol_provider = result
            .pointer("/capabilities/workspaceSymbolProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        // completionProvider is an object (or absent) — a bare bool is never
        // used, so presence of a non-null value means the server completes.
        let completion_provider = result
            .pointer("/capabilities/completionProvider")
            .map(|value| !value.is_null())
            .unwrap_or(false);
        let completion_trigger_characters = result
            .pointer("/capabilities/completionProvider/triggerCharacters")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let hover_provider = result
            .pointer("/capabilities/hoverProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let definition_provider = result
            .pointer("/capabilities/definitionProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let references_provider = result
            .pointer("/capabilities/referencesProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let implementation_provider = result
            .pointer("/capabilities/implementationProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let call_hierarchy_provider = result
            .pointer("/capabilities/callHierarchyProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let document_symbol_provider = result
            .pointer("/capabilities/documentSymbolProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let formatting_provider = result
            .pointer("/capabilities/documentFormattingProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let code_action_provider = result
            .pointer("/capabilities/codeActionProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        let rename_provider = result
            .pointer("/capabilities/renameProvider")
            .map(capability_enabled)
            .unwrap_or(true);
        // Pull diagnostics (`textDocument/diagnostic`, LSP 3.17) — only advertised
        // by servers that opt in (rust-analyzer, gopls, …). Absent on most others,
        // so default false and fall back to the push wait when missing.
        let diagnostic_provider = result
            .pointer("/capabilities/diagnosticProvider")
            .map(capability_enabled)
            .unwrap_or(false);

        tracing::info!(
            target: "neoism_agent::perf",
            lsp = %self.label,
            initialize_ms = crate::perf::elapsed_ms(initialized_started),
            workspace_symbol_provider,
            hover_provider,
            definition_provider,
            references_provider,
            implementation_provider,
            document_symbol_provider,
            formatting_provider,
            code_action_provider,
            rename_provider,
            diagnostic_provider,
            "lsp initialized"
        );

        Ok(InitializeResult {
            workspace_symbol_provider,
            completion_provider,
            completion_trigger_characters,
            hover_provider,
            definition_provider,
            references_provider,
            implementation_provider,
            call_hierarchy_provider,
            document_symbol_provider,
            formatting_provider,
            code_action_provider,
            rename_provider,
            diagnostic_provider,
        })
    }

    pub(crate) fn open_document(&mut self, file: &Path, language_id: &str) -> Result<()> {
        let text = fs::read_to_string(file)
            .with_context(|| format!("failed to read document {}", file.display()))?;
        self.open_document_with_text(file, language_id, &text)
    }

    pub(crate) fn open_document_with_text(
        &mut self,
        file: &Path,
        language_id: &str,
        text: &str,
    ) -> Result<()> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": path_to_file_uri(file),
                    "languageId": language_id,
                    "version": 0,
                    "text": text,
                }
            }),
        )
    }

    pub(crate) fn change_document(
        &mut self,
        file: &Path,
        version: i32,
        text: &str,
    ) -> Result<()> {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {
                    "uri": path_to_file_uri(file),
                    "version": version,
                },
                "contentChanges": [{ "text": text }],
            }),
        )
    }

    pub(crate) fn save_document(&mut self, file: &Path) -> Result<()> {
        self.notify(
            "textDocument/didSave",
            json!({ "textDocument": { "uri": path_to_file_uri(file) } }),
        )
    }

    /// Pull-model diagnostics (`textDocument/diagnostic`, LSP 3.17). Returns the
    /// report's `items` array so the caller can reuse `parse_diagnostics`. This is
    /// the fast opencode-style path: it returns whatever the server has ready
    /// *right now* instead of blocking on a `publishDiagnostics` push that, for
    /// servers like rust-analyzer, only arrives after `cargo check` finishes.
    pub(crate) fn pull_diagnostics(
        &mut self,
        file: &Path,
        timeout: Duration,
    ) -> Result<Value> {
        let report = self.request(
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": path_to_file_uri(file) } }),
            timeout,
        )?;
        // RelatedFullDocumentDiagnosticReport: { kind: "full", items: [...] }. An
        // "unchanged" report (or a server that omits items) yields an empty list,
        // which the caller treats as "nothing new ready yet".
        let items = report
            .get("items")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        Ok(json!({
            "uri": path_to_file_uri(file),
            "diagnostics": items,
        }))
    }

    pub(crate) fn close_document(&mut self, file: &Path) -> Result<()> {
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": path_to_file_uri(file) } }),
        )
    }

    pub(crate) fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let params_bytes = params.to_string().len();
        let started = crate::perf::now();
        write_lsp_message(
            &mut self.stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            }),
        )
        .with_context(|| format!("failed to send LSP request `{method}`"))?;
        let response = wait_for_response(&self.messages, id, timeout)
            .with_context(|| format!("LSP request `{method}` failed"));
        tracing::info!(
            target: "neoism_agent::perf",
            lsp = %self.label,
            method,
            id,
            timeout_ms = timeout.as_millis(),
            params_bytes,
            elapsed_ms = crate::perf::elapsed_ms(started),
            ok = response.is_ok(),
            error = response.as_ref().err().map(|error| error.to_string()),
            "lsp request completed"
        );
        response
    }

    pub(crate) fn wait_for_notification(
        &self,
        method: &str,
        timeout: Duration,
    ) -> Result<Option<Value>> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            match self.messages.recv_timeout(remaining) {
                Ok(message) => {
                    if message.get("method").and_then(Value::as_str) == Some(method) {
                        return Ok(Some(
                            message.get("params").cloned().unwrap_or(Value::Null),
                        ));
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(None),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        write_lsp_message(
            &mut self.stdin,
            &json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }),
        )
        .with_context(|| format!("failed to send LSP notification `{method}`"))
    }

    pub(crate) fn shutdown(&mut self) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        write_lsp_message(
            &mut self.stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "shutdown",
                "params": null,
            }),
        )?;
        let _ = wait_for_response(&self.messages, id, SHUTDOWN_TIMEOUT);
        self.notify("exit", Value::Null)
    }

    pub(crate) fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for StdioLspClient {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            tracing::warn!(
                target: "neoism_agent::perf",
                lsp = %self.label,
                pid = self.child.id(),
                "lsp process still running on drop; killing"
            );
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn wait_for_response(
    messages: &Receiver<Value>,
    id: i64,
    timeout: Duration,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("timed out waiting for response id {id}");
        }

        let message = messages
            .recv_timeout(remaining)
            .with_context(|| format!("timed out waiting for response id {id}"))?;
        if message.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = message.get("error") {
            bail!("server returned error for response id {id}: {error}");
        }
        return Ok(message.get("result").cloned().unwrap_or(Value::Null));
    }
}

fn write_lsp_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

pub(crate) fn read_lsp_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;

    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }

        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }

        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.trim().parse::<usize>().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid length: {error}"),
                )
            })?);
        }
    }

    let content_length = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP message missing Content-Length",
        )
    })?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(io::Error::other)
}
