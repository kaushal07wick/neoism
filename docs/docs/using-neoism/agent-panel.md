---
sidebar_position: 2
title: Agent Panel
---

# Agent Panel

The agent panel connects Neoism to the local `neoism-agent` server. It is designed for codebase-aware assistance inside the same workspace as the editor and terminal.

## What You Can Do

- Send prompts tied to the current workspace.
- Stream assistant text and reasoning/thinking parts.
- Run tools such as file search, file reads, shell commands, patch application, and task tracking.
- Approve or reject permission-gated actions.
- Launch background subagents for parallel investigation.
- Switch agent/model/thinking settings when the UI exposes those controls.

## Runtime Pieces

The panel UI state lives in `neoism-frontend/shared/src/panels/agent_pane`. The local agent server lives in `neoism-agent/crates/neoism-agent-server`. The CLI entry point is `neoism-agent`, which can start the server or run direct agent workflows.

Run the server directly during development:

```bash
cargo run -p neoism-agent -- serve
```

By default it binds to `127.0.0.1:4096`.

## Message Ordering

Neoism renders assistant thinking/reasoning before the final answer. This is important during queued prompts and refreshes: the server may store the final text part before reasoning parts, but the UI normalizes that into a readable order.

## Permissions

The agent runtime is permission-aware. Some tools can run directly, while command execution, file edits, or external paths may require an explicit permission rule depending on the session configuration.

## Debugging Agent Output

Start the server directly to see logs and stream behavior:

```bash
cargo run -p neoism-agent -- serve
```

Healthy streams usually show `ok=true` on provider completion and successful tool completion logs. Very large `metadata_bytes` values usually mean a tool captured too much unignored workspace state, commonly from generated files that should be ignored.

Use `NEOISM_AGENT_PERF_LOG=1` when you need more detailed perf-oriented tracing from the agent runtime.