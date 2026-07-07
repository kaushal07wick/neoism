# The Editor

Neoism embeds **managed Neovim** inside the workspace instead of replacing it. You get real nvim, wrapped in Rust-owned chrome — file tree, buffer tabs, command palette, finder, diagnostics, and workspace tabs.

Two pages go deeper:

- [[Neovim]] — how the managed runtime works and what's handled for you.
- [[Languages and LSP|Languages & LSP]] — syntax and language servers.

## Opening files

- **Alt + E** toggles the file tree; click a file (or press Enter) to open it.
- **Alt + S** opens project search.
- The **command palette** (`Ctrl + P`) has a fuzzy file finder — start typing a filename.
- `:tabnew`, `:enew`, and `:new` create Neoism-owned editor tabs when no path is given.

## Buffer tabs & panes

Open files show as buffer tabs. Move between them and split the view:

```text
Ctrl+Shift+Left / Right     previous / next buffer tab
Ctrl+Shift+R                split right
Ctrl+Shift+D                split down
```

(macOS uses `Cmd`; see [[../Keybindings|Keybindings]].)

## Diagnostics

Errors and warnings surface inline and in a status-bar pill. Syntax highlighting (Treesitter) and language intelligence (LSP) install on demand — details in [[Languages and LSP|Languages & LSP]].

The goal isn't a fake VS Code — it's terminal-native editing with enough chrome that navigation, diagnostics, agents, and notes feel like one workspace.
