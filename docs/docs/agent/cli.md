---
sidebar_position: 2
title: Agent CLI
---

# Agent CLI

The `neoism-agent` binary is the command-line interface for the local agent runtime.

## Serve

Start the local HTTP/SSE server:

```bash
cargo run -p neoism-agent -- serve
```

Options include:

```bash
neoism-agent serve --hostname 127.0.0.1 --port 4096
```

Use `--cors` when a browser client needs an allowed origin during development.

## Other Commands

The CLI also contains commands for agent/provider workflows and server requests. Current command groups include:

- `serve` - start the local server.
- `acp` - run the agent control protocol bridge.
- `run`, `chat`, and `tui` - start interactive or one-shot agent workflows.
- `config` and `doctor` - inspect configuration and environment health.
- `providers` and `models` - inspect available providers and model metadata.
- `auth` - inspect and manage provider authentication.
- `mcp` - manage Model Context Protocol servers.
- `session` and `tool` - inspect sessions and registered tools.

Most server-backed commands accept `--server`, defaulting to `http://127.0.0.1:4096`. Workspace-scoped commands generally accept `--dir`.

Common workflow commands:

```bash
cargo run -p neoism-agent -- run --dir . "summarize this project"
cargo run -p neoism-agent -- chat --dir .
cargo run -p neoism-agent -- doctor --dir .
cargo run -p neoism-agent -- models --verbose
cargo run -p neoism-agent -- session list --dir .
cargo run -p neoism-agent -- tool list --dir .
```

Run:

```bash
cargo run -p neoism-agent -- --help
```

and command-specific help:

```bash
cargo run -p neoism-agent -- serve --help
```

Prefer help output for exact flags while the CLI is still evolving.