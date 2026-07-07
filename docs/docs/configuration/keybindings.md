---
sidebar_position: 2
title: Key Bindings
---

# Key Bindings

Neoism supports default platform key bindings plus user-configured bindings. Key handling covers terminal interactions, pane navigation, editor workflows, and UI actions.

## Implementation Areas

- Default bindings live under `neoism-frontend/desktop/src/bindings`.
- Platform-specific bindings are split across `neoism-frontend/desktop/src/bindings/platform/linux.rs`, `macos.rs`, `windows.rs`, and `bsd.rs`.
- Configured bindings are merged by the binding layer before runtime use.

## Documentation Status

The inherited key binding reference needs a full Neoism-specific pass. Until the settings/config schema is stabilized, prefer documenting workflows and pointing contributors to the binding source files for exact defaults.

## Future Settings Page

The settings page should allow searching actions, viewing defaults, detecting conflicts, and editing bindings without hand-writing TOML.