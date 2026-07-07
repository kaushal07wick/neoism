---
sidebar_position: 2
title: Repository Map
---

# Repository Map

This map describes the current repository layout. It is intentionally scoped to what the codebase owns today instead of listing every dependency or future idea.

## Frontend

- `neoism-frontend/shared` - `neoism-ui` Rust library. Shared UI state, layout policy, panels, command palette, agent pane state, notes/sidebar state, diagnostics UI state, and cross-surface chrome behavior.
- `neoism-frontend/desktop` - native `neoism` binary. Owns desktop window integration, local terminal/editor hosting, PTYs, native event handling, and desktop bridges into the shared UI layer.
- `neoism-frontend/web` - `@neoism/web` TypeScript/Vite client. Connects to `neoism-workspace-daemon` over WebSocket, renders the browser workspace shell, and hosts the web terminal panel.
- `neoism-frontend/wasm` - `neoism-terminal-wasm` crate. Exposes the browser terminal renderer used by the web client.

## Protocol And Workspace Services

- `neoism-protocol` - pure serializable message and snapshot crate shared between clients and daemon services. It contains no I/O or async runtime ownership.
- `neoism-workspace-daemon` - daemon process for web/cloud-style workspace access. Handles sessions, workspace state, files, git, diagnostics, PTY routing, and agent integration behind a WebSocket boundary.
- `neoism-workspace-index` - project indexing support.
- `neoism-sync` - synchronization-related workspace code.

## Terminal And Rendering

- `neoism-terminal-core` - terminal parser, grid, snapshots, ANSI/protocol behavior, and terminal data model.
- `neoism-terminal-pty` - pseudo-terminal integration for local terminal sessions.
- `neoism-backend` - terminal backend, config, performer, and runtime integration.
- `neoism-window` - native window and platform layer.
- `sugarloaf` - rendering and font stack used by native and wasm terminal rendering paths.
- `neoism-grapheme-width` - grapheme/cell width support.
- `copa` - ANSI/VTE parser crate used by terminal handling.
- `corcovado` - lower-level event/runtime support.
- `teletypewriter` - PTY helper crate.

## Agent

- `neoism-agent/crates/neoism-agent-core` - core agent/session types.
- `neoism-agent/crates/neoism-agent-server` - local agent server, provider streaming, tool runtime, LSP/tool support, and HTTP/SSE integration.
- `neoism-agent/crates/neoism-agent-cli` - CLI wrapper for agent workflows.
- `neoism-rm-agent` - additional agent-related crate in the workspace.

## Extensions And Devices

- `neoism-extensions` - extension-related workspace crate.
- `neoism-remarkable` - reMarkable-related integration crate.
- `neoism-notifier` - notification support.
- `neoism-proc-macros` - procedural macros used by Neoism crates.

## Docs And Packaging

- `docs/docs` - public documentation pages.
- `docs/src/pages` - custom Docusaurus pages.
- `docs/docusaurus.config.js` - docs site configuration.
- `docs/sidebars.js` - docs sidebar structure.
- `packaging` - packaging support files.
- `misc`, `extra`, and root `Makefile` targets - release, install, man page, and developer workflow support.

## Boundary Rules

- Shared behavior should live in `neoism-frontend/shared`, not separately in desktop and web.
- Wire shapes should live in `neoism-protocol`, not in ad hoc frontend-only structs when daemon/web compatibility matters.
- Native-only behavior belongs near `neoism-frontend/desktop`, `neoism-window`, `sugarloaf`, and PTY/runtime crates.
- Browser-only rendering and browser event handling belong in `neoism-frontend/web`.
- Daemon-owned session, file, git, diagnostics, PTY, and agent routing belongs in `neoism-workspace-daemon`.