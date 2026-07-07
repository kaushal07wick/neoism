---
sidebar_position: 3
title: Terminal Settings
---

# Terminal Settings

Neoism keeps a mature terminal foundation. Terminal configuration includes rendering, fonts, colors, cursor behavior, navigation, shell integration, protocols, and window behavior.

## Terminal Capabilities

- True color and theme customization.
- Font and cursor configuration.
- Splits, tabs, and navigation behavior.
- Shell integration and hyperlink handling.
- Sixel, Kitty graphics, and iTerm2 image protocol support from the terminal core.

## Verified Config Areas

The current config model includes these terminal-facing areas:

- `fonts` and `line-height` for font stack and cell sizing.
- `cursor` for cursor shape, blinking, and blink interval.
- `scrollback-history-limit`, `scroll`, and `enable-scroll-bar` for history and scroll behavior.
- `shell`, `working-dir`, `env-vars`, and `use-fork` for process launch behavior.
- `renderer`, `window`, `navigation`, `keyboard`, `bindings`, `hints`, `bell`, and `effects` for terminal interaction and rendering behavior.
- `theme`, `colors`, `adaptive-theme`, `force-theme`, and `draw-bold-text-with-light-colors` for appearance.
- `copy-on-select`, `confirm-before-quit`, `hide-mouse-cursor-when-typing`, and selection color behavior for workflow preferences.

## Example Terminal Config

```toml
line-height = 1.0
scrollback-history-limit = 10000
copy-on-select = false
confirm-before-quit = true
hide-mouse-cursor-when-typing = false
draw-bold-text-with-light-colors = false

[cursor]
shape = "block"
blinking = false
blinking-interval = 530

[navigation]
mode = "plain"
clickable = false
unfocused-split-opacity = 0.7
hide-if-single = true

[keyboard]
disable-ctlseqs-alt = false
ime-cursor-positioning = true

[window]
mode = "windowed"
width = 800
height = 490
opacity = 1.0
blur = false
decorations = "enabled"
colorspace = "srgb"

[renderer]
backend = "automatic"
disable-unfocused-render = false
disable-occluded-render = false
strategy = "events"

[bell]
audio = false

[effects]
custom-mouse-cursor = false
trail-cursor = true

[panel]
row-gap = 10.0
column-gap = 10.0
border-width = 1.0
border-radius = 10.0
```

Platform defaults can differ. For example, audio bell defaults on for macOS and Windows, `keyboard.disable-ctlseqs-alt` defaults on for macOS, and the default shell/editor commands are platform-specific.

## Renderer Backends

The renderer backend accepts `automatic`, `vulkan`, `gl`, and `dx12` where supported. macOS also supports `metal`; with the `wgpu` feature, `wgpumetal` is available.

Prefer `automatic` unless you are debugging a renderer-specific issue.