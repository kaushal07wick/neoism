# @neoism/web

TypeScript + Vite web frontend for the neoism workspace daemon.

Mobile-first. Talks to `neoism-workspace-daemon` over a WebSocket using the
`neoism-protocol` JSON wire format. The terminal panel hosts
`neoism-terminal-wasm`: when the wasm bundle is present it hands its
`<canvas>` to the exported `RenderedTerminal` (sugarloaf-backed
WebGPU/WebGL renderer); when the bundle is absent it falls back to a
canvas2d stub so the app stays runnable in zero-config dev.

## Prerequisites

- Node.js 20+
- A running daemon on `127.0.0.1:7878` (see `neoism-workspace-daemon`).

## Install

```
cd neoism-frontend/web
npm install
```

## Develop

```
npm run dev
```

Vite serves on `http://127.0.0.1:5173`. The dev server proxies `/ws` to
`ws://127.0.0.1:7878/session`, so the in-app default URL works locally.

## Typecheck / build

```
npm run typecheck
npm run build
```

## Build the renderer bundle

The terminal panel auto-upgrades from the canvas2d stub to the real
sugarloaf-backed renderer when it finds the wasm bundle under
`src/wasm/` (that is the path `loadRealWasm` in
`src/terminal/createTerminal.ts` actually imports — NOT
`public/neoism-terminal-wasm/`, which is a stale legacy copy; building
there produces a bundle the browser never loads). Build it with:

```
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build --target web -d neoism-frontend/web/src/wasm \
  neoism-frontend/wasm
```

Run from the repository root. The output directory is `.gitignore`d; rebuild
whenever `neoism-terminal-wasm` or its workspace deps change. With the
bundle in place, `npm run dev` (or `npm run build`) loads
`RenderedTerminal` and sugarloaf paints cells through WebGPU/WebGL — no
xterm.js in the loop.

## Fonts

The chrome (connection screen, frame, fallback background) ships with
[JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) as the
primary monospace face, with a system-monospace stack as fallback. Four
static weights/styles are committed under
`public/fonts/jetbrains-mono/`:

- `JetBrainsMono-Regular.woff2`
- `JetBrainsMono-Bold.woff2`
- `JetBrainsMono-Italic.woff2`
- `JetBrainsMono-BoldItalic.woff2`

These were extracted from the official `JetBrainsMono-2.304.zip` release
(`fonts/webfonts/`) and are tracked verbatim. The license (SIL OFL 1.1)
travels with the upstream archive; refer to that release for the LICENSE
file if redistributing.

If you ever need to refresh the fonts, drop the same four filenames into
`public/fonts/jetbrains-mono/` and rebuild. The `@font-face`
declarations live at the top of `src/styles/app.css`.

`index.html` preloads `JetBrainsMono-Regular.woff2` so the first paint
of the connection screen lands on the correct glyphs.

## Theme

CSS theme tokens (`--neoism-*` in `src/styles/app.css`) only paint
chrome. The terminal renderer reads its ANSI palette and cursor colour
from `TerminalSnapshot.theme` — that Rust struct in
`neoism-terminal-core` is the source of truth for cell colours. If the
two ever drift, update the Rust side and let the snapshot push it
through.

## Notes

- No `xterm.js`. The terminal rendering path is either the
  canvas2d stub (no wasm bundle) or `neoism-terminal-wasm`'s
  `RenderedTerminal` driving sugarloaf over the same `<canvas>`.
- Layout is mobile-first. Every rule has to hold at a 375px viewport.
- No CSS framework dependency: chrome styling is hand-rolled CSS so the
  bundle stays small and the rules stay legible.
