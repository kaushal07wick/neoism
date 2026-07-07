# Configuration

Neoism reads a single TOML file:

```text
Linux    ~/.config/neoism/config.toml   (or $NEOISM_CONFIG_HOME)
macOS    ~/.config/neoism/config.toml
Windows  %LOCALAPPDATA%\neoism\config.toml
```

On first launch Neoism writes a default `config.toml` (just a comment pointing at the docs) — **every key is optional** and falls back to a sensible default. Open it from the command palette (search "config") or edit it directly.

## The essentials

```toml
[neoism]
theme = "pastel_dark"      # unified theme for chrome + terminal + nvim
minimap = false
display-name = "your-name" # what collaborators see in multiplayer presence

[fonts]
family = "cascadiacode"
size = 14.0

[cursor]
shape = "block"            # block | underline | beam | hidden
blinking = false

[scroll]
multiplier = 3.0
```

Also useful at the top level (all kebab-case): `line-height`, `copy-on-select`, `confirm-before-quit`, `hide-mouse-cursor-when-typing`, `enable-scroll-bar`, `scrollback-history-limit`, `working-dir`, `env-vars = ["KEY=VALUE"]`, and `force-theme = "dark"`.

## The rest of the tree

- [[Themes, Cursor and Fonts|Themes, Cursor & Fonts]] — pick a theme, color your cursor, choose a font.
- [[Shaders]] — optional CRT and post-process filters.
- [[../Keybindings|Keybindings]] — remap keys via the `[bindings]` table.

Other sections you can set: `[window]`, `[navigation]`, `[keyboard]`, `[bell]`, `[hints]`, `[renderer]` (see [[Shaders]]), and `[developer]` (`log-level`, `enable-fps-counter`).

> Changes made from the UI (theme picker, preferences) are written back into `[neoism]` in this same file.
