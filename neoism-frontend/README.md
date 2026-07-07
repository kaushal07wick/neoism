# neoism-frontend

All frontend code for Neoism lives here. The directory is split four ways:

| Path        | Cargo / npm name        | What it is                                                              |
|-------------|-------------------------|-------------------------------------------------------------------------|
| `desktop/`  | `neoism` (bin)          | Native binary. winit + sugarloaf + wgpu. Owns the OS window and PTYs.   |
| `shared/`   | `neoism-ui` (lib)       | UI state, layout, policy. Imported by both `desktop` and `wasm`.        |
| `web/`      | `@neoism/web` (npm)     | TypeScript + Vite client. Talks to `neoism-workspace-daemon` over WS.   |
| `wasm/`     | `neoism-terminal-wasm`  | Terminal renderer compiled to `wasm32-unknown-unknown`. Loaded by web.  |

`shared` is the source of truth for any state or policy that has to behave
identically on desktop and web — keybindings, command palette, file tree,
session-layout tree, agent pane, command composer, etc. Both the desktop
binary and the wasm renderer link `neoism-ui`; the web TS code drives that
shared state through the daemon's JSON wire format (`neoism-protocol`).

## Build

```sh
# Desktop
cargo check -p neoism -p neoism-ui
cargo run   -p neoism

# Daemon (the web client's backend)
cargo run -p neoism-workspace-daemon

# Web (in another shell, after the daemon is up)
cd neoism-frontend/web
npm install
npm run dev   # http://127.0.0.1:5173, proxies /ws → 127.0.0.1:7878/session

# Wasm terminal renderer (rebuild when wasm/ or its deps change)
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build --target web \
  -d neoism-frontend/web/public/neoism-terminal-wasm \
  neoism-frontend/wasm
```

## How wasm and native share

The desktop binary calls into `neoism-ui` directly. The web client drives the
same `neoism-ui` state in two ways:

1. **Policy / layout / panels** — `neoism-ui` types are serialized by the
   daemon and sent over WebSocket to the TS client. The TS code mirrors the
   structures and renders DOM.
2. **Terminals** — `neoism-terminal-wasm` re-exports a `RenderedTerminal`
   that owns a `<canvas>` and paints cells through sugarloaf
   (WebGPU / WebGL). The TS terminal panel hands its canvas to this exported
   class. If the wasm bundle is absent, the panel falls back to a small
   canvas2d stub so the app still runs in zero-config dev.

## Adding a new shared panel

1. Add the panel's state + render policy under
   `neoism-frontend/shared/src/panels/<your_panel>/`. Keep it
   side-effect-free; expose a snapshot type the daemon can serialize.
2. Re-export the snapshot from `neoism-protocol` so the wire format learns
   the new shape.
3. Mount it on the desktop side under `neoism-frontend/desktop/src/` next to
   the existing panel hosts (`screen/`, `chrome/`, etc.).
4. Mount it on the web side under `neoism-frontend/web/src/panels/` — render
   the snapshot, send back any user input through the existing dispatcher.
5. `cargo check -p neoism -p neoism-ui` and `npm run typecheck` should both
   stay green.

Anything that needs identical behaviour on phone and desktop belongs in
`shared/`. Anything that's truly OS-specific (window management, wgpu setup,
native font cache backing) stays in `desktop/`.
