#!/usr/bin/env bash
# Publish a "Neoism has moved" notice to the legacy public dist repo
# (parkers0405/neoism-dist). Neoism is now open source and releases live on the
# main repo (parkers0405/neoism); this points anyone still landing on the old
# dist repo to the new home. Run once after the first public release.
#
# Usage: scripts/dist-moved-notice.sh
set -euo pipefail

read -r -d '' BODY <<'MD' || true
# Neoism has moved

**Neoism is now open source. The project lives here:**

## → https://github.com/parkers0405/neoism

Source, releases, issues, and `neoism update` all happen on the main repo now.
This `neoism-dist` repo is archived and no longer updated.

## Install / update

```bash
curl -fsSL https://raw.githubusercontent.com/parkers0405/neoism/main/scripts/install.sh | bash
```

Then stay current with `neoism update`.
MD

sha="$(gh api repos/parkers0405/neoism-dist/contents/README.md --jq .sha 2>/dev/null || true)"
args=(-f message="Archived: Neoism is now open source at parkers0405/neoism" \
      -f content="$(printf '%s' "$BODY" | base64 -w0)")
[ -n "$sha" ] && args+=(-f sha="$sha")
gh api -X PUT repos/parkers0405/neoism-dist/contents/README.md "${args[@]}" --jq '.commit.sha[0:8]'
echo "published 'moved' notice to parkers0405/neoism-dist"
