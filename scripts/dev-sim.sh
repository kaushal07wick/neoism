#!/usr/bin/env bash
# Two-instance multiplayer sim on ONE machine — always the same build.
#
#   ./scripts/dev-sim.sh          # launch host-sim + guest-sim
#   ./scripts/dev-sim.sh kill     # stop both
#
# Instance A ("host-sim"): its own embedded daemon (default socket +
# 127.0.0.1:7878), cwd = a small sim project. Share its workspace with
# the palette (Cmd+; → "workspace share").
# Instance B ("guest-sim"): attaches to A's daemon over ws://127.0.0.1:7878
# with a DIFFERENT host id, so A's shared workspace shows up under a
# foreign host in B's Workspaces modal — the full join/guest flow
# (adopt, guest icon, remote tree via the files plane, ownership
# guards) exercises exactly like two laptops, minus the network.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$REPO/target/debug/neoism"
SIMDIR="${NEOISM_SIM_DIR:-/tmp/neoism-sim-project}"
LOGDIR="${NEOISM_SIM_LOGS:-/tmp/neoism-sim-logs}"

if [[ "${1:-}" == "kill" ]]; then
  pkill -f "target/debug/neoism" || true
  echo "sim instances stopped"
  exit 0
fi

mkdir -p "$SIMDIR/src" "$SIMDIR/docs" "$LOGDIR"
[[ -f "$SIMDIR/README.md" ]] || {
  echo "# Sim Project" > "$SIMDIR/README.md"
  echo 'fn main() { println!("hi"); }' > "$SIMDIR/src/main.rs"
  echo "shared notes" > "$SIMDIR/docs/NOTES.md"
}

RUST_FILTER='neoism::remote_files=info,neoism::workspaces=info,neoism::workspace_root=debug'

cd "$SIMDIR"
NEOISM_HOST_ID=host-sim NEOISM_LOG_FILE= RUST_LOG="$RUST_FILTER" \
  "$BIN" >| "$LOGDIR/host-sim.log" 2>&1 &
echo "host-sim pid=$!  (cwd=$SIMDIR, daemon on 127.0.0.1:7878)"
sleep 4

cd "$HOME"
NEOISM_HOST_ID=guest-sim RUST_LOG="$RUST_FILTER" \
  "$BIN" --daemon-url ws://127.0.0.1:7878 >| "$LOGDIR/guest-sim.log" 2>&1 &
echo "guest-sim pid=$! (attached to host-sim's daemon)"

echo
echo "flow: in host-sim  → Cmd+; → 'workspace share' → Enter"
echo "      in guest-sim → Cmd+; → 'workplaces' → Enter → pick host-sim's workspace"
echo "logs: $LOGDIR/{host-sim,guest-sim}.log"
