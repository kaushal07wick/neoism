---
title: 'Contributing'
language: 'en'
---

# Contributing

Neoism is a Rust workspace for a terminal-first Neovim IDE, shared desktop/web frontend architecture, daemon-backed workspace access, and a local agent runtime. Contributions are welcome when they keep the project moving toward that product direction.

## Before Changing Code

- Build context from the current source, not from old Rio-era docs.
- Keep shared UI behavior in `neoism-frontend/shared` unless it is truly host-specific.
- Keep protocol shapes in `neoism-protocol` when desktop, web, daemon, or cloud-style clients need the same data.
- Keep generated files out of git.
- Prefer small, verified changes with focused tests.
- Document user-facing behavior when adding commands, settings, or agent flows.

## Useful Commands

```bash
cargo build -p neoism
cargo check -p neoism -p neoism-ui
cargo run -p neoism-workspace-daemon
cargo test -p neoism agent::
cargo run -p neoism-agent -- serve
cd docs && npm run build
```

## Documentation

Docs should describe Neoism as its own product. Historical Rio details belong only where they explain inherited terminal compatibility or attribution.