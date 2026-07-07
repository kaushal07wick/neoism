---
sidebar_position: 1
title: Agent Overview
---

# Agent Overview

`neoism-agent` is the local agent runtime used by Neoism. It is a Rust service and CLI for sessions, messages, provider streams, tools, permissions, and subagents.

## Packages

- `neoism-agent-core` - shared session/message/provider data types.
- `neoism-agent-server` - HTTP/SSE server, tool runtime, provider stream processing, persistence, and integrations.
- `neoism-agent-cli` - command-line entry point, including `serve`.

## Start The Server

```bash
cargo run -p neoism-agent -- serve
```

Defaults:

- Host: `127.0.0.1`
- Port: `4096`
- Base URL: `http://127.0.0.1:4096`

## Concepts

- **Session** - conversation state in a workspace.
- **Message** - user, assistant, or system entry with ordered parts.
- **Part** - text, reasoning, tool, file, subtask, or step metadata.
- **Tool** - operation exposed to the agent, such as search, read, shell, patch, task tracking, notes, and web operations.
- **Permission** - rule controlling whether a tool action can run.
- **Provider stream** - model output converted into Neoism message events.

## Configuration Locations

Agent config is loaded from the Neoism config directory and project config files.

- Global config directory: `$XDG_CONFIG_HOME/neoism`, or `$HOME/.config/neoism`.
- State directory: `$XDG_STATE_HOME/neoism`, or `$HOME/.local/state/neoism`.
- Cache directory: `$XDG_CACHE_HOME/neoism`, or `$HOME/.cache/neoism`.
- Global config names: `config.json`, `config.jsonc`, `neoism.json`, `neoism.jsonc`.
- Project config names: `neoism.json`, `neoism.jsonc` discovered upward through the workspace.

Set `NEOISM_AGENT_DISABLE_PROJECT_CONFIG=1` to ignore project config files during debugging.

Config parsing supports JSONC-style comments and trailing commas. Markdown files with YAML frontmatter can also contribute prompt/content during config merging.

## Logs

Perf logs use the `neoism_agent::perf` target. Normal logs include server bind/open events, provider stream completion, and tool execution completion. Large metadata sizes are a signal to check ignored files, tool snapshotting, or generated outputs.

Set `NEOISM_AGENT_PERF_LOG=1` for more detailed perf-oriented tracing.