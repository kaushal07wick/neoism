#!/usr/bin/env python3
"""Check or rewrite WASM-reachable Rust sources to use `web_time`.

`std::time::{Instant,SystemTime}` compiles for wasm32-unknown-unknown but
panics in the browser. Runtime code shared with the web frontend should use
`web_time`, which delegates to native time on desktop and browser time on WASM.

Usage:
  scripts/wasm-safe-time.py        # check only
  scripts/wasm-safe-time.py --fix  # rewrite known-safe patterns
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

SCAN_ROOTS = [
    ROOT / "neoism-frontend" / "shared" / "src",
    ROOT / "neoism-frontend" / "wasm" / "src",
    ROOT / "sugarloaf" / "src",
]

# These are native-only tools/backends or tests where std::time is fine.
SKIP_PARTS = {
    "bin",
    "examples",
    "tests",
}

SKIP_SUFFIXES = (
    # Vulkan renderer code is not compiled into the browser frontend.
    Path("sugarloaf/src/renderer/vulkan.rs"),
)

REPLACEMENTS = [
    ("std::time::Instant", "web_time::Instant"),
    ("std::time::SystemTime", "web_time::SystemTime"),
    ("std::time::UNIX_EPOCH", "web_time::UNIX_EPOCH"),
    ("std::time::Duration", "web_time::Duration"),
    ("use std::time::", "use web_time::"),
]

FORBIDDEN_PATTERNS = [
    re.compile(r"\bstd::time::(?:Instant|SystemTime|UNIX_EPOCH|Duration)\b"),
    re.compile(r"^\s*use\s+std::time::", re.MULTILINE),
]


def iter_rust_files() -> list[Path]:
    files: list[Path] = []
    for root in SCAN_ROOTS:
        if not root.exists():
            continue
        for path in root.rglob("*.rs"):
            rel = path.relative_to(ROOT)
            if any(part in SKIP_PARTS for part in rel.parts):
                continue
            if any(rel == suffix for suffix in SKIP_SUFFIXES):
                continue
            files.append(path)
    return sorted(files)


def rewrite(text: str) -> str:
    for old, new in REPLACEMENTS:
        text = text.replace(old, new)
    return text


def forbidden_lines(text: str) -> list[tuple[int, str]]:
    hits: list[tuple[int, str]] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        if any(pattern.search(line) for pattern in FORBIDDEN_PATTERNS):
            hits.append((line_no, line.rstrip()))
    return hits


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fix", action="store_true", help="rewrite known-safe std::time uses")
    args = parser.parse_args()

    changed: list[Path] = []
    violations: list[tuple[Path, int, str]] = []

    for path in iter_rust_files():
        original = path.read_text()
        text = rewrite(original) if args.fix else original
        if args.fix and text != original:
            path.write_text(text)
            changed.append(path)
        for line_no, line in forbidden_lines(text):
            violations.append((path, line_no, line))

    if changed:
        print("rewrote WASM-safe time imports/usages:")
        for path in changed:
            print(f"  {path.relative_to(ROOT)}")

    if violations:
        print("forbidden std::time usage remains in WASM-reachable runtime sources:")
        for path, line_no, line in violations:
            print(f"  {path.relative_to(ROOT)}:{line_no}: {line}")
        print("run with --fix, or gate the source out of wasm if it is native-only")
        return 1

    print("WASM-reachable runtime sources use web_time for time APIs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())