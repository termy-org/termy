---
title: Command Boundary
description: Ownership boundaries for command and keybind behavior
order: 1
category: Architecture
---

This document defines ownership boundaries for command and keybind behavior.

## Ownership

- `termy_command_core` owns:
  - Command IDs and config-facing command names.
  - Command config-name parsing and normalization.
  - Keybind defaults.
  - Keybind directive parsing (`clear`, bind, unbind).
  - Deterministic keybind resolution order.
- App/CLI adapters own:
  - UI labels, keywords, and command-palette presentation.
  - Platform-specific visibility policy for palette entries.
  - UI trigger canonicalization and validation (for example GPUI keystroke parsing).

## Dependency Rule

- `termy_command_core` must remain a pure domain crate.
- `termy_command_core` must not depend on:
  - `termy_config_core`
  - `gpui`
  - other UI or presentation crates

## Integration Pattern

- Adapters convert parsed config keybind lines into `termy_command_core::KeybindLineRef`.
- Adapters call `parse_keybind_directives_from_iter`; trigger canonicalization happens in `termy_command_core`.
- Adapters call `resolve_keybinds` over `default_resolved_keybinds`.

This keeps one canonical command/keybind engine while preserving thin and readable adapter code.
