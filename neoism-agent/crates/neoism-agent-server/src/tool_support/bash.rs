use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use serde_json::{json, Value};
use tokio::process::Command;

use super::args::{optional_string, required_string, usize_arg};
use super::paths::{display_path, existing_project_path};
use super::{process, shell_scan, truncate, ToolContext, ToolExecutionResult};

pub(super) async fn bash_tool(
    context: ToolContext,
    arguments: Value,
) -> anyhow::Result<ToolExecutionResult> {
    let command = required_string(&arguments, "command")?.to_string();
    let cwd = if let Some(workdir) = optional_string(&arguments, "workdir") {
        existing_project_path(&context, &workdir)?
    } else {
        context.cwd.clone()
    };
    let project_root = context.cwd.canonicalize().with_context(|| {
        format!(
            "failed to resolve project directory {}",
            context.cwd.display()
        )
    })?;
    let scan = shell_scan::scan(&command, &cwd, &project_root);
    for dir in &scan.external_dirs {
        context.ensure_explicit_allowed("external_directory", dir)?;
    }
    for pattern in &scan.command_patterns {
        context.ensure_allowed("bash", pattern)?;
    }
    let snapshot_before = crate::snapshot::bash_before(&context.cwd);
    let timeout_ms = usize_arg(&arguments, "timeout").unwrap_or(120_000).max(1) as u64;
    let description =
        optional_string(&arguments, "description").unwrap_or_else(|| command.clone());
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut process = Command::new(&shell);
    process
        .arg("-lc")
        .arg(&command)
        .current_dir(&cwd)
        .env("TERM", "xterm-256color")
        .env("NEOISM_TERMINAL", "1")
        .envs(context.env.clone())
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process::set_new_process_group(&mut process);
    let mut child = process
        .spawn()
        .with_context(|| format!("failed to spawn shell {shell}"))?;
    let child_id = child.id();
    let stdout_task = process::read_child_output(child.stdout.take());
    let stderr_task = process::read_child_output(child.stderr.take());
    let timeout = tokio::time::sleep(Duration::from_millis(timeout_ms));
    tokio::pin!(timeout);
    let wait_result: anyhow::Result<std::process::ExitStatus> = tokio::select! {
        status = child.wait() => {
            status.with_context(|| format!("failed to wait for shell {shell}"))
        }
        _ = &mut timeout => {
            process::terminate_child(&mut child, child_id).await;
            Err(anyhow::anyhow!("bash command timed out after {timeout_ms}ms"))
        }
        _ = process::wait_for_cancel(context.cancel.clone()) => {
            process::terminate_child(&mut child, child_id).await;
            Err(anyhow::anyhow!("bash command aborted"))
        }
    };

    let stdout = stdout_task.await??;
    let stderr = stderr_task.await??;
    let stdout = String::from_utf8_lossy(&stdout);
    let stderr = String::from_utf8_lossy(&stderr);
    let mut rendered = String::new();
    let mut truncated = false;
    let mut output_path = None;
    if !stdout.is_empty() {
        let output = truncate::truncate_output(&stdout);
        truncated |= output.truncated;
        output_path = output.output_path;
        rendered.push_str(&output.output);
    }
    if !stderr.is_empty() {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        let output = truncate::truncate_output(&stderr);
        truncated |= output.truncated;
        output_path = output_path.or(output.output_path);
        rendered.push_str(&output.output);
    }
    if rendered.is_empty() {
        rendered.push_str("(no output)");
    }

    let status = match wait_result {
        Ok(status) => status,
        Err(error) => anyhow::bail!("{error}\n{rendered}"),
    };
    let exit = status.code();
    if !status.success() {
        anyhow::bail!("bash command failed with status {:?}\n{}", exit, rendered);
    }
    let snapshots = crate::snapshot::bash_after(snapshot_before);
    let mut metadata = json!({
        "command": command,
        "description": description.clone(),
        "exit": exit,
        "timeout": timeout_ms,
        "workdir": display_path(&context.cwd, &cwd),
        "truncated": truncated,
        "alwaysPatterns": scan.always_patterns.into_iter().collect::<Vec<_>>(),
        "commandPatterns": scan.command_patterns.into_iter().collect::<Vec<_>>(),
        "externalDirectories": scan.external_dirs.into_iter().collect::<Vec<_>>(),
        "outputPath": output_path.as_ref().map(|path| path.to_string_lossy().to_string()),
    });
    crate::snapshot::add_metadata_snapshots(&mut metadata, snapshots);

    Ok(ToolExecutionResult {
        title: description.clone(),
        output: rendered,
        metadata: Some(metadata),
    })
}
