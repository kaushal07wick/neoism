---
sidebar_position: 1
title: Install
---

# Install Neoism

Neoism is currently documented as a source-built project. The repository includes an installer script that can install common system dependencies, ensure Rust and Neovim are available, build the app, and install terminal support files.

## Quick Path

From a checkout of the repository:

```bash
./install.sh
```

The installer is intended for local development builds. It detects common Linux package managers, checks for Rust, checks for Neovim, installs Treesitter parsers used by the managed editor flow, installs terminfo, and builds Neoism.

## Manual Build

```bash
cargo build -p neoism
```

Run the app from the debug build:

```bash
cargo run -p neoism
```

Run the local agent server separately when developing agent features:

```bash
cargo run -p neoism-agent -- serve
```

By default the agent listens on `http://127.0.0.1:4096`.

## Requirements

- Rust toolchain matching the workspace MSRV in `Cargo.toml`.
- Neovim for managed editor panes.
- C/C++ build tools and `cmake` for native dependencies.
- Fontconfig/Freetype/XCB/XKB libraries on Linux.
- Node.js and npm for the docs site.

See [Build From Source](./build-from-source.md) for platform notes and explicit package examples.