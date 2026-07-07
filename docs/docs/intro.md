---
sidebar_position: 1
title: What Is Neoism?
---

# What Is Neoism?

Neoism is a terminal-first Neovim IDE written mostly in Rust. It combines a real terminal, managed Neovim panes, Rust-owned IDE chrome, workspace services, and a local agent runtime.

Neoism is not an Electron editor and it is not only a terminal emulator. The native desktop app is the primary target today, while the web frontend and WebAssembly renderer are being built around the same shared state and protocol boundaries.

## What Neoism Provides

- GPU-rendered terminal workspaces with tabs, splits, scrollback, selection, and modern terminal protocol support.
- Managed Neovim/editor surfaces with app-owned UI for buffers, files, diagnostics, search, command workflows, notes, git views, and agent panes.
- A shared Rust UI/state layer in `neoism-frontend/shared` so desktop, web, and future surfaces can reuse the same layout and policy instead of cloning behavior per platform.
- A native desktop frontend in `neoism-frontend/desktop` that owns OS windows, native input, PTYs, and the high-performance terminal/editor host.
- A web frontend in `neoism-frontend/web` that talks to `neoism-workspace-daemon` over WebSocket using the JSON shapes from `neoism-protocol`.
- A WebAssembly terminal renderer in `neoism-frontend/wasm` for the browser canvas path.
- A local agent stack under `neoism-agent/crates` for sessions, tools, provider streaming, and CLI/server integration.

## Current Scope

Neoism is scoped around a local-first developer workspace. Desktop is the most complete runtime because it directly owns the OS window, terminal PTYs, GPU renderer, and local process model.

The web path is real but intentionally split from desktop-only responsibilities. Browser code should consume daemon snapshots and send user actions through protocol messages; it should not reimplement workspace policy that already belongs in shared Rust code. Mobile and cloud use the same direction: keep portable behavior in shared state and protocol types, and keep OS-specific or host-specific behavior at the edge.

## Architecture At A Glance

- `neoism-frontend/shared` - shared UI state, panel policy, layout state, command palette behavior, agent pane state, and cross-surface chrome logic.
- `neoism-frontend/desktop` - native `neoism` binary using the windowing/rendering stack and direct local PTY/session ownership.
- `neoism-frontend/web` - TypeScript/Vite browser client for the daemon-backed workspace surface.
- `neoism-frontend/wasm` - WebAssembly terminal renderer loaded by the web frontend.
- `neoism-protocol` - pure serializable wire-format crate for frontend/daemon messages; no async runtime and no I/O.
- `neoism-workspace-daemon` - daemon backend for web/cloud-style workspace access, session state, files, git, diagnostics, PTY routing, and agent integration.
- `neoism-backend` - terminal backend, configuration, performer, and runtime integration inherited and extended from the terminal base.
- `neoism-terminal-core` - terminal parser, grid, snapshots, and protocol behavior.
- `neoism-terminal-pty` - local pseudo-terminal support.
- `neoism-window` and `sugarloaf` - native windowing plus rendering/font stack.
- `neoism-agent/crates` - local agent core, server, and CLI.
- `docs` - this Docusaurus documentation site.

## Where To Start

- Install Neoism from source: [Build From Source](./install/build-from-source.md)
- Learn the workspace: [Using Neoism](./using-neoism/overview.md)
- Configure the app: [Configuration Overview](./configuration/overview.md)
- Understand the agent: [Agent Overview](./agent/overview.md)
- Contribute to the codebase: [Development Overview](./development/overview.md)