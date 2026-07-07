# Shared Chrome Status

Status: current architecture note. This replaces the older chrome-lift audit that referred to the pre-split frontend tree and a not-yet-created shared UI crate.

The shared UI split now exists under `neoism-frontend/shared`. The remaining work is not a wholesale lift from a desktop-only tree; it is ongoing boundary cleanup between shared state, native desktop glue, web rendering, protocol snapshots, and daemon services.

## Current Split

- `neoism-frontend/shared/src/chrome.rs` - shared chrome state and cross-surface coordination.
- `neoism-frontend/shared/src/panels` - shared panel state and update logic for agent pane, buffer tabs, command palette, git diff, notes sidebar, diagnostics, and related UI surfaces.
- `neoism-frontend/desktop/src` - native host that wires shared UI into the desktop window, renderer, terminal/editor host, PTYs, and OS capabilities.
- `neoism-frontend/web/src` - browser host that renders daemon-backed state and handles mobile-first browser UI.
- `neoism-frontend/wasm/src` - WebAssembly terminal renderer bridge for the web terminal panel.
- `neoism-protocol/src` - serializable workspace, PTY, editor, git, files, diagnostics, search, CRDT, cursor, pairing, auth, and agent messages.
- `neoism-workspace-daemon/src` - daemon implementation for web/cloud-style session access.

## What Is Already In The Right Shape

- Shared panel state exists as a first-class Rust crate instead of being embedded only in the desktop binary.
- Web and desktop have explicit frontend directories instead of one native tree pretending to be portable.
- Protocol types are isolated in `neoism-protocol`, which keeps wire formats separate from runtime I/O.
- The web app is daemon-backed and does not own native PTYs directly.
- The frontend README documents the intended flow for adding a shared panel.

## Remaining Audit Areas

These areas need continued review when adding or moving UI features:

- Direct filesystem, git, shell, or process access from shared UI code.
- Native window/input types leaking into shared panel state.
- Browser-only state being copied into shared Rust instead of represented as host input or daemon snapshot data.
- Protocol drift between Rust snapshots and TypeScript web mirrors.
- Agent pane behavior split between shared UI state, local agent server, daemon transport, and web rendering.
- Terminal rendering differences between native `sugarloaf`/window paths and the wasm canvas path.

## Boundary Rules

- Shared chrome can own state, policy, layout, focus, and typed user intent.
- Desktop owns native windows, local PTYs, native input translation, and OS capability implementations.
- Web owns DOM events, CSS, browser connection lifecycle, and browser rendering constraints.
- The daemon owns session state, workspace access, file/git/diagnostics/search routing, PTY routing, and agent integration for web/cloud clients.
- Protocol owns message shapes only.

## Review Checklist

Use this checklist before moving code into `neoism-frontend/shared`:

- Does the code compile without native-only dependencies such as `neoism-window`, PTY crates, process spawning, or OS clipboard APIs?
- Are filesystem/git/search operations represented through host or daemon services instead of direct side effects?
- Is browser-only behavior kept in `neoism-frontend/web`?
- Does daemon-visible state have a serializable shape in `neoism-protocol`?
- Can desktop and web render the same logical state without duplicating policy?
- Are tests or typechecks covering both the Rust shared crate and the web client when the change crosses the boundary?

## Historical Note

Older docs described `neoism-ui` as a future extraction. That plan is obsolete. The current source of truth is the `neoism-frontend/` split and the repository map in `docs/docs/reference/repo-map.md`.