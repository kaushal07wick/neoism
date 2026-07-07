# Managed Neovim

Neoism runs **real Neovim** as its editor engine — but a *managed*, curated instance, not your personal setup.

## What you need

Just **`nvim` on your `PATH`**. Neoism doesn't bundle the binary; if it can't find `nvim`, it tells you rather than starting a broken editor. Everything else — the runtime, syntax, and language servers — Neoism handles.

## How Neoism runs nvim

Neoism launches `nvim --embed --headless --clean`, anchored to your workspace root. `--clean` means **your own `init.lua` / `init.vim` and personal plugins are *not* loaded**. Instead Neoism injects its own curated runtime and calls `require('rio').setup()`.

This is deliberate: a consistent, fast editing core that behaves the same for everyone, wired directly into Neoism's chrome (file tree, tabs, diagnostics, minimap) rather than fighting a pile of plugins.

## What's managed for you

The bundled runtime provides options, events, theme, clipboard, completion, commands, change signs, search, a minimap, and indent/chunk highlighting — written to `~/.local/share/rio/nvim-runtime/` on startup.

- **Theme** — your `[neoism] theme` applies live to the nvim syntax palette too, so chrome, terminal, and editor all match. See [[../Configuration/Themes, Cursor and Fonts|Themes, Cursor & Fonts]].
- **Syntax (Treesitter)** — parsers install **automatically**: open a file whose grammar is missing and Neoism fetches + builds it (needs `git`, a C compiler, and `tree-sitter` available), then re-highlights. 40+ languages are supported. If a build fails you get a **Retry Install** prompt.

## Customizing

Because the runtime is curated, you customize through **Neoism's** config (theme, cursor, fonts, keybindings) rather than a personal nvim config. See [[../Configuration/Configuration|Configuration]] and [[../Keybindings|Keybindings]].

For semantic features (go-to-definition, rename, hover), see [[Languages and LSP|Languages & LSP]].
