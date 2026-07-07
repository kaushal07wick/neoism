#!/usr/bin/env bash
# Build the managed Tree-sitter parsers + highlight queries into a staging
# directory suitable for shipping inside the release tarball as `runtime/`.
#
# The desktop app's first-run bootstrap copies this into
# `<data_home>/rio/nvim-runtime` (see neoism-frontend/desktop/src/bootstrap.rs),
# which is the same layout ./install.sh installs directly.
#
# Usage: scripts/build-treesitter-runtime.sh OUT_DIR [VERSION]
#   OUT_DIR gets: parser/<lang>.so, queries/<lang>/*.scm, RUNTIME_VERSION
set -euo pipefail

OUT_DIR="${1:?usage: build-treesitter-runtime.sh OUT_DIR [VERSION]}"
VERSION="${2:-dev}"
LANGS=(rust python javascript typescript tsx go lua json toml yaml markdown nix)
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarn:\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

command -v git >/dev/null || die "git is required"
command -v cc >/dev/null || die "cc is required"

ts_repo() {
  case "$1" in
    rust) printf '%s' 'https://github.com/tree-sitter/tree-sitter-rust' ;;
    python) printf '%s' 'https://github.com/tree-sitter/tree-sitter-python' ;;
    javascript) printf '%s' 'https://github.com/tree-sitter/tree-sitter-javascript' ;;
    typescript|tsx) printf '%s' 'https://github.com/tree-sitter/tree-sitter-typescript' ;;
    go) printf '%s' 'https://github.com/tree-sitter/tree-sitter-go' ;;
    lua) printf '%s' 'https://github.com/tree-sitter-grammars/tree-sitter-lua' ;;
    json) printf '%s' 'https://github.com/tree-sitter/tree-sitter-json' ;;
    toml) printf '%s' 'https://github.com/tree-sitter-grammars/tree-sitter-toml' ;;
    yaml) printf '%s' 'https://github.com/tree-sitter-grammars/tree-sitter-yaml' ;;
    markdown) printf '%s' 'https://github.com/tree-sitter-grammars/tree-sitter-markdown' ;;
    nix) printf '%s' 'https://github.com/nix-community/tree-sitter-nix' ;;
    *) return 1 ;;
  esac
}

ts_subdir() {
  case "$1" in
    typescript) printf '%s' 'typescript' ;;
    tsx) printf '%s' 'tsx' ;;
    markdown) printf '%s' 'tree-sitter-markdown' ;;
    *) printf '%s' '.' ;;
  esac
}

compile_parser() {
  local lang="$1" grammar_root="$2" output="$3"
  local src_dir="$grammar_root/src"
  local build_root="$WORK_DIR/build/$lang"
  local objects=() needs_cxx=0

  [ -f "$src_dir/parser.c" ] || die "$grammar_root has no src/parser.c"
  mkdir -p "$build_root"
  cc -fPIC -O2 -I "$src_dir" -c "$src_dir/parser.c" -o "$build_root/parser.o"
  objects+=("$build_root/parser.o")

  if [ -f "$src_dir/scanner.c" ]; then
    cc -fPIC -O2 -I "$src_dir" -c "$src_dir/scanner.c" -o "$build_root/scanner.o"
    objects+=("$build_root/scanner.o")
  fi
  if [ -f "$src_dir/scanner.cc" ]; then
    command -v c++ >/dev/null || die "c++ required for $lang scanner"
    c++ -fPIC -O2 -I "$src_dir" -c "$src_dir/scanner.cc" -o "$build_root/scanner_cc.o"
    objects+=("$build_root/scanner_cc.o")
    needs_cxx=1
  fi

  if [ "$needs_cxx" -eq 1 ]; then
    c++ -shared -o "$output" "${objects[@]}"
  else
    cc -shared -o "$output" "${objects[@]}"
  fi
}

copy_queries() {
  local grammar_root="$1" lang="$2" source_root="$3"
  local source_queries="$grammar_root/queries"
  local dest_queries="$OUT_DIR/queries/$lang"
  local highlight_files=()

  if [ ! -d "$source_queries" ] && [ -d "$source_root/queries" ]; then
    source_queries="$source_root/queries"
  fi
  if [ ! -f "$source_queries/highlights.scm" ]; then
    warn "$lang has no highlights.scm; parser shipped without queries"
    return 0
  fi

  mkdir -p "$dest_queries"
  cp "$source_queries"/*.scm "$dest_queries"/

  highlight_files=("$source_queries/highlights.scm")
  case "$lang" in
    javascript)
      [ -f "$source_queries/highlights-jsx.scm" ] && highlight_files+=("$source_queries/highlights-jsx.scm")
      [ -f "$source_queries/highlights-params.scm" ] && highlight_files+=("$source_queries/highlights-params.scm")
      ;;
    typescript)
      local js_queries
      js_queries="$(dirname "$source_root")/javascript/queries"
      [ -f "$js_queries/highlights.scm" ] && highlight_files+=("$js_queries/highlights.scm")
      ;;
    tsx)
      local js_queries
      js_queries="$(dirname "$source_root")/javascript/queries"
      [ -f "$js_queries/highlights-jsx.scm" ] && highlight_files+=("$js_queries/highlights-jsx.scm")
      [ -f "$js_queries/highlights.scm" ] && highlight_files+=("$js_queries/highlights.scm")
      ;;
  esac

  : > "$dest_queries/highlights.scm"
  local query
  for query in "${highlight_files[@]}"; do
    printf '\n; ---- %s ----\n' "$(basename "$query")" >> "$dest_queries/highlights.scm"
    cat "$query" >> "$dest_queries/highlights.scm"
    printf '\n' >> "$dest_queries/highlights.scm"
  done
}

mkdir -p "$OUT_DIR/parser"

for lang in "${LANGS[@]}"; do
  repo="$(ts_repo "$lang")" || die "no repo mapping for $lang"
  subdir="$(ts_subdir "$lang")"

  # Clone per lang into a dir NAMED after the lang — install.sh layout.
  # typescript/tsx query merging looks up the sibling `javascript/` clone
  # by that name, so the layout is load-bearing.
  clone_dir="$WORK_DIR/src/$lang"
  if [ ! -d "$clone_dir" ]; then
    log "Fetching $lang grammar"
    git clone --quiet --depth 1 "$repo" "$clone_dir"
  fi

  if [ "$subdir" = "." ]; then
    grammar_root="$clone_dir"
  else
    grammar_root="$clone_dir/$subdir"
  fi

  log "Building parser: $lang"
  compile_parser "$lang" "$grammar_root" "$OUT_DIR/parser/$lang.so"
  copy_queries "$grammar_root" "$lang" "$clone_dir"
done

printf '%s\n' "$VERSION" > "$OUT_DIR/RUNTIME_VERSION"
log "Runtime staged at $OUT_DIR (version $VERSION)"
