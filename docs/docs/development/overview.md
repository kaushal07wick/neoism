---
sidebar_position: 1
title: Development Overview
---

# Development Overview

Neoism is a multi-target Rust workspace with a native desktop frontend, a shared UI/state crate, a web frontend, a WebAssembly terminal renderer, daemon-backed workspace services, terminal infrastructure, and a local agent runtime.

The most important development rule is to keep platform boundaries honest. Shared behavior belongs in `neoism-frontend/shared` and protocol crates. Native-only behavior stays in `neoism-frontend/desktop`, `neoism-window`, `sugarloaf`, and PTY/runtime crates. Browser behavior stays in `neoism-frontend/web` unless it is really shared state or policy.

## Common Commands

```bash
cargo build -p neoism
cargo run -p neoism
cargo check -p neoism -p neoism-ui
cargo run -p neoism-workspace-daemon
cargo run -p neoism-agent -- serve
```

Web frontend:

```bash
cd neoism-frontend/web
npm install
npm run dev
npm run typecheck
npm run build
```

Docs site:

```bash
cd docs
npm install
npm start
npm run build
```

## Frontend Architecture

`neoism-frontend` is split into four surfaces:

- `shared/` builds the `neoism-ui` crate. It owns cross-platform UI state, layout policy, panel state, command behavior, and reusable IDE chrome logic.
- `desktop/` builds the native `neoism` binary. It owns the OS window, native event loop, local PTYs, native rendering setup, and direct integration with terminal/editor hosting.
- `web/` builds the `@neoism/web` Vite client. It renders browser chrome, connects to the workspace daemon over WebSocket, and mirrors daemon snapshots using TypeScript.
- `wasm/` builds `neoism-terminal-wasm`. The web terminal panel loads this bundle when available and falls back to a canvas stub for zero-config development.

When adding a feature that should behave the same on desktop and web, start in `neoism-frontend/shared`. Expose serializable state through `neoism-protocol` if the daemon or browser needs to observe it. Only then mount platform-specific rendering or input handling in `desktop/` and `web/`.

## Protocol And Daemon Boundary

`neoism-protocol` is intentionally a pure wire-format crate. It defines serializable client/server messages and shared snapshots, but it does not own I/O, tasks, sockets, or async runtime behavior.

`neoism-workspace-daemon` is the host for daemon-backed workspace behavior. The web frontend talks to it through WebSocket messages. This boundary is what keeps web and future cloud/mobile surfaces from depending directly on native desktop internals.

Keep these responsibilities separate:

- Put message shapes, snapshot types, and cross-process enums in `neoism-protocol`.
- Put socket handling, session ownership, file/git/diagnostics routing, PTY routing, and agent integration in `neoism-workspace-daemon`.
- Put browser rendering and browser event handling in `neoism-frontend/web`.
- Put native window/input/rendering integration in `neoism-frontend/desktop`.

## Development Principles

- Verify settings, paths, and commands against code before documenting them.
- Keep docs and UI copy independent from inherited terminal-emulator wording unless the inherited behavior is still the relevant source of truth.
- Do not duplicate shared UI policy into platform frontends just to make one surface work faster.
- Prefer protocol snapshots and explicit user actions over ad hoc frontend/backend state coupling.
- Treat the agent server and frontend timeline as one streamed system: server part order, SSE events, tool results, and UI reconciliation all matter.
- Keep generated scratch files and local build output out of git.
- Prefer focused tests for UI ordering, protocol behavior, agent events, config behavior, and daemon message handling.

## Important Areas

- `neoism-frontend/shared/src/panels/agent_pane` - agent panel state, API conversion, event updates, and timeline behavior.
- `neoism-frontend/shared/src/panels` - shared IDE panels such as buffer tabs, command palette, git diff, notes, diagnostics, and file/workspace UI.
- `neoism-frontend/desktop/src` - native app host, screen integration, desktop-specific bridges, and local rendering/input glue.
- `neoism-frontend/web/src` - browser app shell, terminal panel, styles, WebSocket services, and TypeScript mirrors of daemon state.
- `neoism-frontend/wasm/src` - browser terminal renderer bridge.
- `neoism-protocol/src` - shared message and snapshot types for workspace, PTY, editor, git, files, diagnostics, search, CRDT, cursor, pairing, auth, and agent flows.
- `neoism-workspace-daemon/src` - daemon session, workspace, files, git, diagnostics, PTY, and agent backend behavior.
- `neoism-agent/crates/neoism-agent-server` - server routes, provider stream processing, tool runtime, and LSP/tool support.
- `neoism-backend/src/config` - configuration model inherited and extended by Neoism.