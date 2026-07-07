#!/usr/bin/env bash
# Rebuild the wasm renderer that the web app loads from `src/wasm/`.
#
# THIS is the step `vite` / `npm run build` does NOT do: vite only bundles
# whatever is already in `src/wasm/`. Changing Rust in `neoism-frontend/wasm`
# does nothing until wasm-pack regenerates that bundle. Wired into the `dev`
# and `build` npm scripts so the served wasm can never silently go stale.
#
# Paths are resolved from this script's location (absolute) so the
# wasm-pack `--out-dir` lands deterministically regardless of cwd.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEB_DIR="$(cd "$HERE/.." && pwd)"
WASM_CRATE="$(cd "$WEB_DIR/../wasm" && pwd)"

# wasm-pack (and the cargo/rustc it shells out to) live in ~/.cargo/bin,
# which isn't always on an interactive shell's PATH even when `cargo` is.
export PATH="$HOME/.cargo/bin:$PATH"

WASM_PACK="$(command -v wasm-pack || true)"
if [ -z "$WASM_PACK" ] && [ -x "$HOME/.cargo/bin/wasm-pack" ]; then
  WASM_PACK="$HOME/.cargo/bin/wasm-pack"
fi
if [ -z "$WASM_PACK" ]; then
  echo "error: wasm-pack not found (looked on PATH and in ~/.cargo/bin)." >&2
  echo "       install it once with:  cargo install wasm-pack --locked" >&2
  exit 1
fi

echo "[build-wasm] building $WASM_CRATE -> $WEB_DIR/src/wasm"
exec "$WASM_PACK" build "$WASM_CRATE" \
  --target web \
  --out-dir "$WEB_DIR/src/wasm" \
  --out-name neoism_terminal_wasm
