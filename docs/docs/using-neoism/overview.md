---
sidebar_position: 1
title: Using Neoism
---

# Using Neoism

Neoism is organized around terminal-native workspaces. A workspace can contain shells, managed Neovim panes, agent sessions, project navigation UI, diagnostics, and editor surfaces.

## Core Concepts

- **Workspace** - the project directory Neoism is operating in.
- **Pane** - a terminal, Neovim/editor, or agent surface inside the workspace.
- **IDE chrome** - Rust-owned UI such as file tree, tabs, command palette, diagnostics, finder, and panels.
- **Agent session** - a local conversation managed by `neoism-agent`, with tools, permissions, and streamed message parts.
- **Subagent** - a background or child agent task launched from an agent session.

## Typical Workflow

1. Open Neoism in a project directory.
2. Use terminal panes for shell work and build/test commands.
3. Use managed Neovim panes for editing.
4. Use diagnostics, file navigation, and project search from the Neoism UI.
5. Start an agent session when you want codebase-aware assistance.
6. Review tool permissions before an agent runs commands or edits files.

## Desktop And Web Surfaces

The native desktop app is the main workflow today. It owns the local OS window, terminal sessions, native input, and direct rendering path.

The web client is a daemon-backed surface. Start `neoism-workspace-daemon` when developing or testing browser access; the Vite app in `neoism-frontend/web` connects to `ws://127.0.0.1:7878/session` through its dev proxy. The browser should receive workspace snapshots and send explicit user actions instead of owning local PTYs or duplicating desktop-only state.

```bash
cargo run -p neoism-workspace-daemon
cd neoism-frontend/web
npm run dev
```

## Desktop And Web Surfaces

The native desktop app is the main workflow today. It owns the local OS window, terminal sessions, native input, and direct rendering path.

The web client is a daemon-backed surface. Start `neoism-workspace-daemon` when developing or testing browser access; the Vite app in `neoism-frontend/web` connects to `ws://127.0.0.1:7878/session` through its dev proxy. The browser should receive workspace snapshots and send explicit user actions instead of owning local PTYs or duplicating desktop-only state.

```bash
cargo run -p neoism-workspace-daemon
cd neoism-frontend/web
npm run dev
```

## Terminal Foundation

Neoism keeps a capable terminal core: tabs, splits, scrollback, selection, keyboard handling, shell integration, and graphics protocols are part of the foundation. The current product goal is to make that terminal foundation serve the IDE and agent workflow.

## Editor Direction

Neoism embeds and manages Neovim rather than replacing it. The Rust UI owns the outer workflow: file tree, buffer tabs, command palette, diagnostics, timeline/agent panels, and workspace-aware navigation.

## What To Expect From Shared UI

Shared behavior should feel consistent across desktop and web because cross-surface state lives in `neoism-frontend/shared` and cross-process shapes live in `neoism-protocol`. Platform surfaces still differ where they must: desktop handles local windows and PTYs directly, while web works through daemon messages and browser rendering constraints.

## What To Expect From Shared UI

Shared behavior should feel consistent across desktop and web because cross-surface state lives in `neoism-frontend/shared` and cross-process shapes live in `neoism-protocol`. Platform surfaces still differ where they must: desktop handles local windows and PTYs directly, while web works through daemon messages and browser rendering constraints.