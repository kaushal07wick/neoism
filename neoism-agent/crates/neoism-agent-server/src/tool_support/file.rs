use anyhow::Context;
use base64::Engine;
use serde_json::{json, Value};

struct WriteMutation {
    display: String,
    content_len: usize,
    previous_len: usize,
    snapshot_before: crate::snapshot::FileState,
}

struct EditMutation {
    display: String,
    count: usize,
    remaining_matches: usize,
    snapshot_before: crate::snapshot::FileState,
}

use super::args::{
    optional_string, required_string, required_string_either,
    required_string_either_many, string_either_many, usize_arg,
};
use super::paths::{
    directory_entries, display_path, existing_project_path, project_path_for_write,
    truncate_line,
};
use super::search::{collect_glob_matches, collect_grep_matches};
use super::{diagnostics, edit_match, format, locks, ToolContext, ToolExecutionResult};

const DEFAULT_READ_LIMIT: usize = 2000;
const MAX_READ_BYTES: usize = 50 * 1024;
const MAX_MEDIA_READ_BYTES: usize = 20 * 1024 * 1024;
const MAX_READ_MANY_RANGES: usize = 80;

pub(super) fn read_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    if let Some(paths) = read_paths_arg(&arguments) {
        let mut outputs = Vec::new();
        let mut items = Vec::new();
        for raw_path in paths {
            let mut item_args = arguments.clone();
            if let Some(object) = item_args.as_object_mut() {
                object.remove("paths");
                object.remove("filePaths");
                object.insert("filePath".to_string(), Value::String(raw_path));
            }
            let result = read_tool(context.clone(), item_args)?;
            if !result.output.is_empty() {
                outputs.push(result.output);
            }
            items.push(json!({
                "title": result.title,
                "metadata": result.metadata,
            }));
        }
        return Ok(ToolExecutionResult {
            title: format!("Read {} paths", items.len()),
            output: outputs.join("\n\n"),
            metadata: Some(json!({
                "type": "batch",
                "count": items.len(),
                "items": items,
            })),
        });
    }

    let raw_path = required_string_either(&arguments, "filePath", "path")?;
    let path = existing_project_path(&context, raw_path)?;
    let display = display_path(&context.cwd, &path);
    context.ensure_allowed("read", &display)?;
    let offset = usize_arg(&arguments, "offset").unwrap_or(1).max(1);
    let limit = usize_arg(&arguments, "limit")
        .unwrap_or(DEFAULT_READ_LIMIT)
        .max(1);

    if path.is_dir() {
        let entries = directory_entries(&path)?;
        if offset > entries.len().saturating_add(1) {
            anyhow::bail!(
                "offset {offset} is out of range for {} ({} entries)",
                display,
                entries.len()
            );
        }
        let start = offset - 1;
        let output = entries
            .iter()
            .skip(start)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let truncated = start.saturating_add(limit) < entries.len();
        let suffix = if truncated {
            format!(
                "\n(Showing {} of {} entries. Use offset={} to continue.)",
                output.lines().count(),
                entries.len(),
                offset + output.lines().count()
            )
        } else {
            format!("\n({} entries)", entries.len())
        };
        let output = format!(
            "<path>{}</path>\n<type>directory</type>\n<entries>\n{}{suffix}\n</entries>",
            path.display(),
            output
        );
        return Ok(ToolExecutionResult {
            title: format!("Read {display}"),
            output,
            metadata: Some(json!({
                "path": display,
                "type": "directory",
                "count": entries.len(),
                "offset": offset,
                "limit": limit,
                "truncated": truncated,
                "preview": entries.iter().skip(start).take(20).cloned().collect::<Vec<_>>().join("\n"),
            })),
        });
    }

    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if let Some(mime) = supported_media_mime(&path) {
        if bytes.len() > MAX_MEDIA_READ_BYTES {
            anyhow::bail!(
                "{display} is too large to attach ({} bytes, limit {} bytes)",
                bytes.len(),
                MAX_MEDIA_READ_BYTES
            );
        }
        let loaded = crate::instruction::nearby(&context.cwd, &path);
        let loaded_paths = loaded
            .iter()
            .map(|item| item.filepath.clone())
            .collect::<Vec<_>>();
        let url = format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        let media_kind = if mime == "application/pdf" {
            "PDF"
        } else {
            "Image"
        };
        return Ok(ToolExecutionResult {
            title: format!("Read {display}"),
            output: format!("{media_kind} read successfully"),
            metadata: Some(json!({
                "path": display,
                "type": "file",
                "mime": mime,
                "bytes": bytes.len(),
                "preview": format!("{media_kind} read successfully"),
                "truncated": false,
                "loaded": loaded_paths,
                "attachments": [{
                    "type": "file",
                    "mime": mime,
                    "url": url,
                    "filename": path.file_name().and_then(|name| name.to_str()).unwrap_or("file"),
                }],
            })),
        });
    }
    if bytes.contains(&0) {
        anyhow::bail!("{display} appears to be binary");
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines = text.lines().collect::<Vec<_>>();
    if offset > lines.len().saturating_add(usize::from(lines.is_empty())) {
        anyhow::bail!(
            "offset {offset} is out of range for {display} ({} lines)",
            lines.len()
        );
    }
    let start = offset - 1;
    let mut rendered = Vec::new();
    let mut rendered_bytes = 0usize;
    let mut byte_capped = false;
    for (index, line) in lines.iter().enumerate().skip(start) {
        if rendered.len() >= limit {
            break;
        }
        let line = format!("{}: {}", index + 1, truncate_line(line));
        let size = line.len() + usize::from(!rendered.is_empty());
        if rendered_bytes.saturating_add(size) > MAX_READ_BYTES {
            byte_capped = true;
            break;
        }
        rendered_bytes += size;
        rendered.push(line);
    }
    let preview = lines
        .iter()
        .skip(start)
        .take(rendered.len().min(20))
        .map(|line| truncate_line(line))
        .collect::<Vec<_>>()
        .join("\n");
    let line_more = start.saturating_add(rendered.len()) < lines.len();
    let truncated = byte_capped || line_more;
    let last = if rendered.is_empty() {
        offset.saturating_sub(1)
    } else {
        offset + rendered.len() - 1
    };
    let next = last + 1;
    let mut output = format!(
        "<path>{}</path>\n<type>file</type>\n<content>\n{}",
        path.display(),
        rendered.join("\n")
    );
    if byte_capped {
        output.push_str(&format!(
            "\n\n(Output capped at 50 KB. Showing lines {offset}-{last}. Use offset={next} to continue.)"
        ));
    } else if line_more {
        output.push_str(&format!(
            "\n\n(Showing lines {offset}-{last} of {}. Use offset={next} to continue.)",
            lines.len()
        ));
    } else {
        output.push_str(&format!("\n\n(End of file - total {} lines)", lines.len()));
    }
    output.push_str("\n</content>");
    let loaded = crate::instruction::nearby(&context.cwd, &path);
    if !loaded.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str("<system-reminder>\n");
        output.push_str(
            &loaded
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        );
        output.push_str("\n</system-reminder>");
    }
    let loaded_paths = loaded
        .iter()
        .map(|item| item.filepath.clone())
        .collect::<Vec<_>>();

    Ok(ToolExecutionResult {
        title: format!("Read {display}"),
        output,
        metadata: Some(json!({
            "path": display,
            "type": "file",
            "lines": lines.len(),
            "offset": offset,
            "limit": limit,
            "truncated": truncated,
            "preview": preview,
            "loaded": loaded_paths,
        })),
    })
}

pub(super) fn read_many_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let files = arguments
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("tool argument files is required"))?;
    if files.is_empty() {
        anyhow::bail!("tool argument files must not be empty");
    }

    let mut outputs = Vec::new();
    let mut items = Vec::new();
    let mut range_count = 0usize;

    for file in files {
        let (path, ranges) = read_many_file_ranges(file)?;
        for (offset, limit) in ranges {
            range_count += 1;
            if range_count > MAX_READ_MANY_RANGES {
                anyhow::bail!("read_many is limited to {MAX_READ_MANY_RANGES} ranges");
            }
            let result = read_tool(
                context.clone(),
                json!({
                    "filePath": path,
                    "offset": offset,
                    "limit": limit,
                }),
            )?;
            if !result.output.is_empty() {
                outputs.push(result.output);
            }
            items.push(json!({
                "path": path,
                "offset": offset,
                "limit": limit,
                "title": result.title,
                "metadata": result.metadata,
            }));
        }
    }

    Ok(ToolExecutionResult {
        title: format!("Read {range_count} ranges"),
        output: outputs.join("\n\n"),
        metadata: Some(json!({
            "type": "rangeBatch",
            "count": range_count,
            "items": items,
        })),
    })
}

pub(super) fn read_around_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let raw_path = required_string_either(&arguments, "filePath", "path")?;
    let path = existing_project_path(&context, raw_path)?;
    let display = display_path(&context.cwd, &path);
    context.ensure_allowed("read", &display)?;
    if path.is_dir() {
        anyhow::bail!("read_around requires a file, got directory {display}");
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.contains(&0) {
        anyhow::bail!("{display} appears to be binary");
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines = text.lines().collect::<Vec<_>>();
    let before = usize_arg(&arguments, "before").unwrap_or(40);
    let after = usize_arg(&arguments, "after").unwrap_or(120);
    let center = read_around_center_line(&arguments, &lines)?;
    let offset = center.saturating_sub(before).max(1);
    let end = center.saturating_add(after).min(lines.len().max(1));
    let limit = end.saturating_sub(offset).saturating_add(1).max(1);

    read_tool(
        context,
        json!({
            "filePath": raw_path,
            "offset": offset,
            "limit": limit,
        }),
    )
}

fn read_many_file_ranges(file: &Value) -> anyhow::Result<(String, Vec<(usize, usize)>)> {
    if let Some(path) = file.as_str().map(str::trim).filter(|path| !path.is_empty()) {
        return Ok((path.to_string(), vec![(1, DEFAULT_READ_LIMIT)]));
    }
    let object = file
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("files entries must be strings or objects"))?;
    let path = object
        .get("filePath")
        .or_else(|| object.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| anyhow::anyhow!("files entries require filePath or path"))?
        .to_string();

    let ranges = if let Some(raw_ranges) = object.get("ranges").and_then(Value::as_array)
    {
        if raw_ranges.is_empty() {
            anyhow::bail!("ranges must not be empty for {path}");
        }
        raw_ranges
            .iter()
            .map(|range| {
                let offset = range
                    .get("offset")
                    .or_else(|| range.get("line"))
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(1)
                    .max(1);
                let limit = range
                    .get("limit")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(DEFAULT_READ_LIMIT)
                    .max(1);
                Ok((offset, limit))
            })
            .collect::<anyhow::Result<Vec<_>>>()?
    } else {
        let offset = object
            .get("offset")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(1)
            .max(1);
        let limit = object
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(DEFAULT_READ_LIMIT)
            .max(1);
        vec![(offset, limit)]
    };

    Ok((path, ranges))
}

fn read_around_center_line(arguments: &Value, lines: &[&str]) -> anyhow::Result<usize> {
    if let Some(line) = usize_arg(arguments, "line").filter(|line| *line > 0) {
        return Ok(line.max(1));
    }
    let pattern = optional_string(arguments, "pattern")
        .or_else(|| optional_string(arguments, "symbol"))
        .ok_or_else(|| {
            anyhow::anyhow!("read_around requires line, pattern, or symbol")
        })?;
    lines
        .iter()
        .position(|line| line.contains(&pattern))
        .map(|index| index + 1)
        .ok_or_else(|| anyhow::anyhow!("pattern {pattern:?} was not found"))
}

fn read_paths_arg(arguments: &Value) -> Option<Vec<String>> {
    for key in ["paths", "filePaths"] {
        let Some(raw) = arguments.get(key) else {
            continue;
        };
        let paths = parse_read_paths_value(raw);
        if !paths.is_empty() {
            return Some(paths);
        }
    }
    None
}

fn parse_read_paths_value(raw: &Value) -> Vec<String> {
    if let Some(array) = raw.as_array() {
        return array
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
    }
    if let Some(s) = raw.as_str() {
        return s
            .split([',', '\n'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
    }
    Vec::new()
}

fn supported_media_mime(path: &std::path::Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}

pub(super) async fn write_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let path = project_path_for_write(
        &context,
        required_string_either(&arguments, "filePath", "path")?,
    )?;
    let display = display_path(&context.cwd, &path);
    context.ensure_allowed("edit", &display)?;
    let _lock = locks::lock_file(&path).await;

    let locked_path = path.clone();
    let mutation = tokio::task::spawn_blocking(move || {
        let result = write_tool_locked(arguments, locked_path, display.clone());
        result
    })
    .await
    .with_context(|| "write tool task panicked")??;
    drop(_lock);

    // LSP diagnostics + formatting do blocking I/O (each diagnostic query can
    // wait for the server). Run them off the async executor so the agent
    // response never freezes while waiting on a language server.
    tokio::task::spawn_blocking(move || write_tool_metadata(context, path, mutation))
        .await
        .with_context(|| "write tool metadata task panicked")?
}

fn write_tool_locked(
    arguments: Value,
    path: std::path::PathBuf,
    display: String,
) -> anyhow::Result<WriteMutation> {
    let content = arguments
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("tool argument content is required"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create directory {}", parent.display())
        })?;
    }
    let snapshot_before = crate::snapshot::FileState::from_path(&path)?;
    let previous_bytes = std::fs::read(&path).ok();
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    let previous_len = previous_bytes
        .as_ref()
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    Ok(WriteMutation {
        display,
        content_len: content.len(),
        previous_len,
        snapshot_before,
    })
}

fn write_tool_metadata(
    context: ToolContext,
    path: std::path::PathBuf,
    mutation: WriteMutation,
) -> anyhow::Result<ToolExecutionResult> {
    let formatted =
        format::format_paths(&context.cwd, context.formatter(), [path.clone()]);
    let lsp_touch = diagnostics::touch_paths(&context.cwd, [path.clone()]);

    let mut metadata = json!({
        "path": mutation.display,
        "bytes": mutation.content_len,
        "previousBytes": mutation.previous_len,
        "lspTouch": lsp_touch,
    });
    if let Some(snapshot) =
        crate::snapshot::file_change(&context.cwd, &path, mutation.snapshot_before)?
    {
        crate::snapshot::add_metadata_snapshots(&mut metadata, vec![snapshot]);
    }
    format::attach_formatted(&mut metadata, &formatted);
    let report =
        diagnostics::attach_lsp_diagnostics(&context.cwd, [path.clone()], &mut metadata);

    let mut output = format!(
        "Wrote {} bytes to {} (previously {} bytes)",
        mutation.content_len, mutation.display, mutation.previous_len
    );
    if let Some(report) = report {
        output.push_str("\n\n");
        output.push_str(&report);
    }

    Ok(ToolExecutionResult {
        title: format!("Write {}", mutation.display),
        output,
        metadata: Some(metadata),
    })
}

pub(super) async fn edit_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let path_arg = required_string_either_many(
        &arguments,
        &["filePath", "path", "file_path", "file"],
    )?;
    let path = existing_project_path(&context, path_arg)?;
    let display = display_path(&context.cwd, &path);
    context.ensure_allowed("edit", &display)?;
    let _lock = locks::lock_file(&path).await;

    let locked_path = path.clone();
    let mutation = tokio::task::spawn_blocking(move || {
        let result = edit_tool_locked(arguments, locked_path, display.clone());
        result
    })
    .await
    .with_context(|| "edit tool task panicked")??;
    drop(_lock);

    // LSP diagnostics + formatting do blocking I/O (each diagnostic query can
    // wait for the server). Run them off the async executor so the agent
    // response never freezes while waiting on a language server.
    tokio::task::spawn_blocking(move || edit_tool_metadata(context, path, mutation))
        .await
        .with_context(|| "edit tool metadata task panicked")?
}

fn edit_tool_locked(
    arguments: Value,
    path: std::path::PathBuf,
    display: String,
) -> anyhow::Result<EditMutation> {
    let old = required_string_either_many(
        &arguments,
        &["old", "oldString", "old_string", "find", "search"],
    )?;
    let new = string_either_many(
        &arguments,
        &["new", "newString", "new_string", "replace", "replacement"],
    )
    .ok_or_else(|| anyhow::anyhow!("tool argument new is required"))?;
    let replace_all = arguments
        .get("replaceAll")
        .or_else(|| arguments.get("replace_all"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw_content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let (had_bom, content) = super::patch::split_bom(&raw_content);

    let normalized_content = content.replace("\r\n", "\n");
    let normalized_old = old.replace("\r\n", "\n");
    let normalized_new = new.replace("\r\n", "\n");

    let snapshot_before = crate::snapshot::FileState::from_path(&path)?;
    let (updated, count, remaining_matches) = edit_match::replace(
        &normalized_content,
        &normalized_old,
        &normalized_new,
        replace_all,
    )
    .with_context(|| format!("failed to edit {display}"))?;

    let final_content = if content.contains("\r\n") {
        updated.replace('\n', "\r\n")
    } else {
        updated
    };
    std::fs::write(&path, super::patch::join_bom(&final_content, had_bom))
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(EditMutation {
        display,
        count,
        remaining_matches,
        snapshot_before,
    })
}

fn edit_tool_metadata(
    context: ToolContext,
    path: std::path::PathBuf,
    mutation: EditMutation,
) -> anyhow::Result<ToolExecutionResult> {
    let formatted =
        format::format_paths(&context.cwd, context.formatter(), [path.clone()]);
    let lsp_touch = diagnostics::touch_paths(&context.cwd, [path.clone()]);

    let mut metadata = json!({
        "path": mutation.display,
        "replaced": mutation.count,
        "remainingMatches": mutation.remaining_matches,
        "lspTouch": lsp_touch,
    });
    if let Some(snapshot) =
        crate::snapshot::file_change(&context.cwd, &path, mutation.snapshot_before)?
    {
        crate::snapshot::add_metadata_snapshots(&mut metadata, vec![snapshot]);
    }
    format::attach_formatted(&mut metadata, &formatted);
    let report =
        diagnostics::attach_lsp_diagnostics(&context.cwd, [path.clone()], &mut metadata);

    let mut output = format!(
        "Replaced {} occurrence(s) in {}",
        mutation.count, mutation.display
    );
    if let Some(report) = report {
        output.push_str("\n\n");
        output.push_str(&report);
    }

    Ok(ToolExecutionResult {
        title: format!("Edit {}", mutation.display),
        output,
        metadata: Some(metadata),
    })
}

pub(super) fn list_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let raw_path = optional_string(&arguments, "path").unwrap_or_else(|| ".".to_string());
    let path = existing_project_path(&context, &raw_path)?;
    context.ensure_allowed("read", &display_path(&context.cwd, &path))?;
    let entries = directory_entries(&path)?;

    Ok(ToolExecutionResult {
        title: format!("List {}", display_path(&context.cwd, &path)),
        output: entries.join("\n"),
        metadata: Some(
            json!({ "path": display_path(&context.cwd, &path), "count": entries.len() }),
        ),
    })
}

pub(super) fn grep_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let pattern = required_string(&arguments, "pattern")?.to_string();
    let limit = usize_arg(&arguments, "limit").unwrap_or(100).max(1);
    let include = optional_string(&arguments, "include");
    let exclude = optional_string(&arguments, "exclude");
    let raw_path = optional_string(&arguments, "path").unwrap_or_else(|| ".".to_string());
    let path = existing_project_path(&context, &raw_path)?;
    context.ensure_allowed("grep", &display_path(&context.cwd, &path))?;
    let matches = collect_grep_matches(
        &context.cwd,
        &path,
        &pattern,
        include.as_deref(),
        exclude.as_deref(),
    )?;
    let total = matches.len();
    let truncated = total > limit;
    let visible = matches.iter().take(limit).collect::<Vec<_>>();
    let mut output = Vec::new();
    if visible.is_empty() {
        output.push("No files found".to_string());
    } else {
        output.push(format!(
            "Found {total} matches{}",
            if truncated {
                format!(" (showing first {limit})")
            } else {
                String::new()
            }
        ));
        let mut current = "";
        for item in &visible {
            if current != item.path {
                if current != "" {
                    output.push(String::new());
                }
                current = &item.path;
                output.push(format!("{}:", item.path));
            }
            output.push(format!("  Line {}: {}", item.line, item.text));
        }
        if truncated {
            output.push(String::new());
            output.push(format!(
                "(Results truncated: showing {limit} of {total} matches ({} hidden). Consider using a more specific path or pattern.)",
                total - limit
            ));
        }
    }

    Ok(ToolExecutionResult {
        title: format!("Grep {pattern}"),
        output: output.join("\n"),
        metadata: Some(json!({
            "pattern": pattern,
            "include": include,
            "exclude": exclude,
            "matches": total,
            "truncated": truncated,
            "items": visible,
        })),
    })
}

pub(super) fn glob_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let pattern = required_string(&arguments, "pattern")?.to_string();
    let limit = usize_arg(&arguments, "limit").unwrap_or(100).max(1);
    let exclude = optional_string(&arguments, "exclude");
    let raw_path = optional_string(&arguments, "path").unwrap_or_else(|| ".".to_string());
    let path = existing_project_path(&context, &raw_path)?;
    context.ensure_allowed("glob", &display_path(&context.cwd, &path))?;
    if !path.is_dir() {
        anyhow::bail!("glob path must be a directory: {}", path.display());
    }
    let matches =
        collect_glob_matches(&context.cwd, &path, &pattern, exclude.as_deref())?;
    let total = matches.len();
    let truncated = total > limit;
    let visible = matches.iter().take(limit).collect::<Vec<_>>();
    let mut output = visible
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    if output.is_empty() {
        output.push("No files found".to_string());
    } else if truncated {
        output.push(String::new());
        output.push(format!(
            "(Results are truncated: showing first {limit} results. Consider using a more specific path or pattern.)"
        ));
    }

    Ok(ToolExecutionResult {
        title: format!("Glob {pattern}"),
        output: output.join("\n"),
        metadata: Some(json!({
            "pattern": pattern,
            "exclude": exclude,
            "count": visible.len(),
            "total": total,
            "truncated": truncated,
            "items": visible,
        })),
    })
}
