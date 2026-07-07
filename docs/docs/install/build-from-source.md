---
sidebar_position: 2
title: Build From Source
---

# Build From Source

Neoism is a Rust workspace. The main app package is `neoism`; the local agent CLI package is `neoism-agent`.

## Clone

```bash
git clone https://github.com/parkersettle/neoism.git
cd neoism
```

If you are already inside this repository, build directly from the workspace root.

## Install Dependencies

The helper script handles the common path:

```bash
./install.sh
```

The script installs or checks for:

- Rust and Cargo.
- Neovim.
- C/C++ build tools.
- `cmake`, `pkg-config`, `python3`, `git`, and `curl`.
- Linux font/windowing dependencies where applicable.
- Treesitter parsers used by the embedded editor workflow.
- Neoism terminfo.

## Build The App

```bash
cargo build -p neoism
```

For an optimized local build:

```bash
cargo build -p neoism --release
```

## Run The App

```bash
cargo run -p neoism
```

## Build And Run The Agent

```bash
cargo build -p neoism-agent
cargo run -p neoism-agent -- serve
```

The server prints its bind address, for example:

```text
neoism agent listening on http://127.0.0.1:4096
```

## Build The Docs

```bash
cd docs
npm install
npm run build
```

Use `npm start` from `docs/` for local Docusaurus development.