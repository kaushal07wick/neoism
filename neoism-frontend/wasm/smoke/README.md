# neoism-terminal-wasm smoke harness

Build the crate, then serve this directory with any static server.

## Build for the smoke page

From the workspace root:

```
wasm-pack build --target web \
  -d neoism-frontend/wasm/smoke/pkg \
  neoism-frontend/wasm
```

Then from this directory:

```
python -m http.server   # or `npx serve .`
```

Open `index.html`. The page imports the generated ES module,
instantiates `Terminal`, feeds `"\x1b[1;32mHello, world!\x1b[0m\r\n\x07"`,
calls `snapshot()`, `drain_effects_json()`, and `take_pty_writes()`, and
prints the row 0 characters + cursor position + effects + any PTY
write-back bytes to a `<pre>`. No bundler, no xterm.js.

## Build for the web frontend

The TS web frontend (`neoism-frontend/web/`) looks for the wasm bundle at
`/neoism-terminal-wasm/neoism_terminal_wasm.js` (vite serves `public/`
as `/`). From the workspace root:

```
wasm-pack build --target web \
  -d neoism-frontend/web/public/neoism-terminal-wasm \
  neoism-frontend/wasm
```

Without the bundle the web frontend falls back to a JS stub terminal
(see `neoism-frontend/web/src/terminal/createTerminal.ts` — it logs a
warning to the console and explains how to build).

## Pre-requisites

* `cargo install wasm-pack`
* `rustup target add wasm32-unknown-unknown`
