# Shared UI Architecture

Status: current architecture note. This replaces an older extraction plan that referred to the pre-split frontend tree.

Neoism now has an explicit frontend split under `neoism-frontend/`:

- `shared/` builds the `neoism-ui` Rust crate. It owns shared UI state, layout policy, panel state, command behavior, and reusable IDE chrome logic.
- `desktop/` builds the native `neoism` binary. It owns the OS window, native input, local PTYs, native renderer setup, and direct integration with terminal/editor hosting.
- `web/` builds the `@neoism/web` Vite client. It renders browser chrome, connects to the workspace daemon over WebSocket, and mirrors daemon snapshots in TypeScript.
- `wasm/` builds `neoism-terminal-wasm`. It exposes the browser terminal renderer loaded by the web client when the wasm bundle is available.

## Design Goal

The shared UI layer should contain behavior that must remain consistent across desktop, web, and future surfaces. It should not own OS windows, browser DOM APIs, local process spawning, filesystem watchers, PTY lifetimes, or network sockets directly.

The practical rule is simple: if a panel decision should be identical on desktop and web, put the state and policy in `neoism-frontend/shared`. If a panel needs to cross a process or browser boundary, expose serializable state through `neoism-protocol` and let the host surface provide the transport.

## Ownership Boundaries

`neoism-frontend/shared` may own:

- Panel state machines and update policy.
- Layout state and cross-surface chrome behavior.
- Pure rendering decisions that are independent from native/window/browser APIs.
- Snapshot types and adapters that are serializable or can be mirrored by the daemon/web client.
- Commands expressed as typed actions for the host to execute.

`neoism-frontend/desktop` owns:

- Native window/event-loop integration.
- Local PTY creation and resize wiring.
- Native input translation.
- Native renderer setup and desktop bridges.
- OS clipboard, process, and filesystem capabilities when they are not daemon-mediated.

`neoism-frontend/web` owns:

- DOM event handling and browser layout concerns.
- WebSocket connection management to `neoism-workspace-daemon`.
- TypeScript rendering of daemon snapshots.
- Mobile-first CSS and browser-specific fallbacks.

`neoism-workspace-daemon` owns:

- Daemon-backed sessions.
- Files, git, diagnostics, search, PTY routing, and agent integration exposed to web/cloud-style clients.
- Persistence and auth/pairing gates for daemon clients.

`neoism-protocol` owns:

- Serializable message and snapshot shapes shared between clients and daemon services.
- No I/O, no async runtime, no host-specific behavior.

## Adding A Shared Panel

1. Put shared state and policy under `neoism-frontend/shared/src/panels/<panel>/`.
2. Keep the shared layer side-effect-free where possible. Host actions should be emitted as typed commands instead of executed directly.
3. Add or reuse `neoism-protocol` types when the daemon or web client needs to observe the panel state.
4. Mount native rendering/input glue in `neoism-frontend/desktop/src`.
5. Mount browser rendering/input glue in `neoism-frontend/web/src`.
6. Verify both Rust and web surfaces with `cargo check -p neoism -p neoism-ui` and `npm run typecheck` from `neoism-frontend/web`.

## What Must Stay Host-Specific

These are intentionally not shared UI responsibilities:

- PTY process lifetime and local shell spawning.
- Native window lifecycle and raw platform input events.
- Browser DOM APIs, WebSocket lifecycle, and mobile CSS.
- Filesystem and git operations that need OS access unless mediated by daemon/protocol services.
- Agent provider networking, MCP processes, and long-running runtime tasks.

The shared layer can describe intent, state, and user actions for these domains, but the host or daemon executes them.

## Scope Notes

Desktop is the complete local runtime today. Web is a real daemon-backed frontend and should continue to get behavior from shared state and protocol snapshots instead of cloning desktop-only policy. Mobile and cloud should follow the same boundary: portable state in shared/protocol crates, host-specific capabilities at the edge.