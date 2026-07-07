---
sidebar_position: 1
title: Configuration
---

# Configuration

Neoism has two main configuration surfaces today: the native app TOML config and the agent JSON/JSONC config. The native app config covers terminal/editor/window behavior. The agent config covers providers, models, tools, permissions, MCP servers, and prompt/runtime behavior.

The native app config file is `config.toml` under the Neoism config directory. On Linux this defaults to `$XDG_CONFIG_HOME/neoism/config.toml` or `$HOME/.config/neoism/config.toml`; `NEOISM_CONFIG_HOME` overrides the directory. Agent configuration is separate and uses JSON/JSONC files loaded by `neoism-agent`.

## Native App Config

The native app reads `config.toml`. On Linux the default path is:

```text
$XDG_CONFIG_HOME/neoism/config.toml
```

If `XDG_CONFIG_HOME` is unset, Neoism falls back to:

```text
$HOME/.config/neoism/config.toml
```

Set `NEOISM_CONFIG_HOME` to override the config directory.

Common native config sections and fields include:

- `[neoism]` - app-specific theme, minimap, display name, cursor color/style, and blinking cursor alias.
- `[cursor]` - terminal cursor shape, blinking, and blink interval.
- `[window]` - window mode, dimensions, opacity, blur, decorations, colorspace, and IME behavior.
- `[renderer]` - backend, unfocused/occluded rendering behavior, filters, and render strategy.
- `[navigation]` - tab/plain navigation mode, split behavior, clickable navigation, and pane opacity.
- `[keyboard]` - alt control-sequence behavior and IME cursor positioning.
- `[bell]` and `[effects]` - audio bell, custom mouse cursor, and cursor trail behavior.
- `[panel]` - split/chrome margin, padding, gaps, border width, and border radius.
- Top-level fields such as `shell`, `editor`, `working-dir`, `line-height`, `env-vars`, `scrollback-history-limit`, `theme`, `adaptive-theme`, `force-theme`, `use-fork`, `copy-on-select`, and `confirm-before-quit`.

Example:

```toml
working-dir = "/home/me/projects"
line-height = 1.0
scrollback-history-limit = 10000
copy-on-select = true
confirm-before-quit = true

[cursor]
shape = "block"
blinking = true
blinking-interval = 530

[window]
mode = "windowed"
width = 1200
height = 800
opacity = 1.0

[renderer]
backend = "automatic"
disable-unfocused-render = false

[keyboard]
ime-cursor-positioning = true
```

## Agent Config

Agent configuration is separate from the native app config. Global agent config file names are:

- `config.json`
- `config.jsonc`
- `neoism.json`
- `neoism.jsonc`

Project config files are discovered upward from the workspace using:

- `neoism.json`
- `neoism.jsonc`

Set `NEOISM_AGENT_DISABLE_PROJECT_CONFIG=1` to ignore project config files while debugging.

Agent config can describe:

- Default model as `provider/model`.
- Thinking/reasoning variant.
- Enabled and disabled providers.
- Default agent.
- Permission defaults and per-tool rules.
- MCP server definitions.
- LSP and formatter integration.

## Current Practical Guidance

Use native TOML for app/terminal/editor behavior. Use agent JSON/JSONC for providers, MCP, permissions, and agent runtime behavior. When adding docs for a new setting, include the exact field name, default, accepted values, and whether changes require restart.

## Documentation Rule

Do not document guessed settings. If a setting is not verified in code, mark it as planned or leave it out.

## Settings Page Notes

The docs should eventually drive the settings UI: every setting should have a label, description, type, default, validation rule, and restart requirement. Keep the configuration reference structured so it can become schema-driven later.

Keep the settings page backed by config validation so bad model refs, zero-step agent configs, and malformed provider entries are caught before save.