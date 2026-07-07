#!/usr/bin/env bash
# Cut a Neoism release: bump the workspace version, commit, tag, push.
#
# The tag triggers .github/workflows/release-neoism.yml, which builds the
# stack per-OS and publishes the tarballs to the GitHub Releases of
# parkers0405/neoism, which `neoism update` and the curl installer pull
# from. The tag MUST match the crate version (`neoism update` compares
# `v<CARGO_PKG_VERSION>` against the release tag), which is why this script
# owns the bump.
#
# Usage: scripts/release.sh 0.4.1
set -euo pipefail

VERSION="${1:?usage: release.sh X.Y.Z (no leading v)}"
case "$VERSION" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "error: version must look like X.Y.Z (got: $VERSION)" >&2; exit 1 ;;
esac

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)"
cd "$ROOT"

[ -z "$(git status --porcelain)" ] || { echo "error: working tree not clean" >&2; exit 1; }
git rev-parse "v$VERSION" >/dev/null 2>&1 && { echo "error: tag v$VERSION already exists" >&2; exit 1; }

CURRENT="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
echo "==> bumping workspace version: $CURRENT -> $VERSION"
sed -i "0,/^version = \"$CURRENT\"/s//version = \"$VERSION\"/" Cargo.toml

echo "==> refreshing Cargo.lock (workspace members only)"
cargo +1.92 update --workspace --quiet

git add Cargo.toml Cargo.lock
git commit -m "release: v$VERSION"
git tag "v$VERSION"

echo "==> pushing main + v$VERSION (this triggers the release build)"
git push origin main "v$VERSION"

cat <<EOF

Release v$VERSION is building:
  https://github.com/parkers0405/neoism/actions/workflows/release-neoism.yml
When green, it publishes to:
  https://github.com/parkers0405/neoism/releases
Users then get it with:  neoism update
EOF
