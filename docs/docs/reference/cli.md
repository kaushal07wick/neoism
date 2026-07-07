---
sidebar_position: 1
title: CLI Reference
---

# CLI Reference

## Neoism App

Build and run the app package:

```bash
cargo run -p neoism
```

## Agent Server

```bash
cargo run -p neoism-agent -- serve
```

Common serve flags:

```bash
neoism-agent serve --hostname 127.0.0.1 --port 4096
```

Use command help for the live source of truth:

```bash
cargo run -p neoism-agent -- --help
cargo run -p neoism-agent -- serve --help
```

## Workspace Daemon

The web frontend talks to `neoism-workspace-daemon` over WebSocket. The daemon defaults to `127.0.0.1:7878` and can be changed with `--addr` or `NEOISM_DAEMON_ADDR`.

```bash
cargo run -p neoism-workspace-daemon
cargo run -p neoism-workspace-daemon -- --addr 127.0.0.1:7878
NEOISM_DAEMON_ADDR=127.0.0.1:7878 cargo run -p neoism-workspace-daemon
```

Useful daemon flags include:

- `--background` - detach and continue running in the background.
- `--pidfile <path>` - write a pidfile and remove it on shutdown.
- `--ephemeral` - skip snapshot load/save.
- `--state-dir <dir>` - choose the daemon snapshot directory.
- `--unix-socket <path>` - serve on a Unix socket.
- `--no-unix-socket` - disable the Unix socket listener.

## Agent Environment

- `NEOISM_AGENT_DISABLE_PROJECT_CONFIG=1` - ignore project config files.
- `NEOISM_AGENT_MODELS_URL` - override provider catalog source.
- `NEOISM_AGENT_MODELS_PATH` - override local provider catalog cache path.
- `NEOISM_AGENT_OPENAI_API_KEY` or `OPENAI_API_KEY` - OpenAI-compatible API key.
- `NEOISM_AGENT_OPENAI_BASE_URL` or `OPENAI_BASE_URL` - OpenAI-compatible base URL.
- `NEOISM_AGENT_PERF_LOG=1` - enable perf-oriented logging.

## Daemon Environment

- `NEOISM_DAEMON_ADDR` - daemon HTTP/WebSocket bind address.
- `NEOISM_DAEMON_SOCKET` - Unix socket path for daemon attach.
- `NEOISM_DAEMON_TOKEN` - explicit daemon token on platforms where automatic token bootstrap is unavailable.
- `NEOISM_REQUIRE_AUTH=1` - require the daemon pairing/auth gate.