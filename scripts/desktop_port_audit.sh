#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUN_CHECKS=0
if [[ "${1:-}" == "--checks" ]]; then
  RUN_CHECKS=1
fi

have_rg=0
if command -v rg >/dev/null 2>&1; then
  have_rg=1
fi

section() {
  printf '\n== %s ==\n' "$1"
}

loc_total() {
  local dir="$1"
  if [[ -d "$dir" ]]; then
    find "$dir" -name '*.rs' -print0 | xargs -0 wc -l 2>/dev/null | tail -1 | awk '{print $1}'
  else
    printf '0'
  fi
}

section "LOC totals"
printf '%-32s %8s\n' "neoism-frontend/desktop/src" "$(loc_total neoism-frontend/desktop/src)"
printf '%-32s %8s\n' "neoism-frontend/shared/src" "$(loc_total neoism-frontend/shared/src)"
printf '%-32s %8s\n' "neoism-protocol/src" "$(loc_total neoism-protocol/src)"
printf '%-32s %8s\n' "neoism-frontend/wasm/src" "$(loc_total neoism-frontend/wasm/src)"
printf '%-32s %8s\n' "neoism-workspace-daemon/src" "$(loc_total neoism-workspace-daemon/src)"
printf '%-32s %8s\n' "neoism-frontend/web/src TS" "$(find neoism-frontend/web/src \( -name '*.ts' -o -name '*.tsx' \) -print0 2>/dev/null | xargs -0 wc -l 2>/dev/null | tail -1 | awk '{print $1}')"

section "Largest desktop Rust files"
find neoism-frontend/desktop/src -name '*.rs' -print0 \
  | xargs -0 wc -l \
  | sort -nr \
  | head -40

section "Large desktop files already importing neoism_ui"
find neoism-frontend/desktop/src -name '*.rs' -print0 | while IFS= read -r -d '' file; do
  lines="$(wc -l < "$file")"
  if [[ "$lines" -ge 300 ]]; then
    if [[ "$have_rg" -eq 1 ]]; then
      if rg -q 'neoism_ui::|use neoism_ui' "$file"; then
        printf '%6s %s\n' "$lines" "$file"
      fi
    elif grep -Eq 'neoism_ui::|use neoism_ui' "$file"; then
      printf '%6s %s\n' "$lines" "$file"
    fi
  fi
done | sort -nr | head -50

section "Small shim/delegate candidates"
find neoism-frontend/desktop/src -name '*.rs' -print0 | while IFS= read -r -d '' file; do
  lines="$(wc -l < "$file")"
  [[ "$lines" -le 120 ]] || continue
  if [[ "$have_rg" -eq 1 ]]; then
    rg -q 'pub use neoism_ui|neoism_ui::.*!' "$file" || continue
  else
    grep -Eq 'pub use neoism_ui|neoism_ui::.*!' "$file" || continue
  fi
  printf '%6s %s\n' "$lines" "$file"
done | sort -nr

section "Same-basename desktop/shared pairs"
tmp_desktop="$(mktemp)"
tmp_shared="$(mktemp)"
trap 'rm -f "$tmp_desktop" "$tmp_shared"' EXIT
find neoism-frontend/desktop/src -name '*.rs' ! -name 'mod.rs' -printf '%f %p\n' | sort > "$tmp_desktop"
find neoism-frontend/shared/src -name '*.rs' ! -name 'mod.rs' -printf '%f %p\n' | sort > "$tmp_shared"
join -j1 "$tmp_desktop" "$tmp_shared" \
  | awk '{printf "%-28s %-62s %s\n", $1, $2, $3}' \
  | head -120

section "Desktop test files still present"
find neoism-frontend/desktop/src -name '*tests*.rs' -print0 \
  | xargs -0 wc -l 2>/dev/null \
  | sort -nr

section "Desktop dirs by Rust LOC"
find neoism-frontend/desktop/src -name '*.rs' -print0 | while IFS= read -r -d '' file; do
  dir="${file#neoism-frontend/desktop/src/}"
  dir="${dir%/*}"
  [[ "$dir" == "$file" ]] && dir="."
  printf '%s %s\n' "$(wc -l < "$file")" "$dir"
done | awk '{sum[$2]+=$1} END {for (dir in sum) printf "%6d %s\n", sum[dir], dir}' | sort -nr | head -60

section "Fork guard"
bash scripts/check_no_ui_in_desktop_fork.sh

section "Git dirty summary"
git status --short | sed -n '1,120p'

if [[ "$RUN_CHECKS" -eq 1 ]]; then
  section "Checks"
  cargo check -p neoism
  cargo check -p neoism-ui
  cargo check -p neoism-terminal-wasm
  cargo check -p neoism-workspace-daemon
  (cd neoism-frontend/web && npm run typecheck)
fi
