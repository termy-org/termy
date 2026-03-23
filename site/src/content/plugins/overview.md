---
title: Plugin System Overview
description: Install, manage, and troubleshoot Termy plugins
order: 1
category: Plugins
---

Termy supports local plugins with a JSON manifest and a simple JSON-RPC protocol over stdio.

## References

- [plugin_core Reference](/docs/plugins/plugin-core)
- [plugin_host Reference](/docs/plugins/plugin-host)
- [plugin_sdk (Rust) Reference](/docs/plugins/plugin-sdk-rust)
- [go-bindings Reference](/docs/plugins/go-bindings)
- [ts-bindings Reference](/docs/plugins/ts-bindings)

## Where plugins live

- Plugin root directory: `~/.config/termy/plugins`
- One plugin per folder
- Required manifest filename: `termy-plugin.json`

## Install and manage plugins

Open `Settings -> Plugins` and use:

- `Install From Folder` to copy a plugin folder into your plugin directory
- `Reload` to re-scan installed plugins
- `Open Folder` to open the plugin root
- Per-plugin controls for `Start/Stop`, `Enable/Disable Autostart`, `Open Folder`, `View Logs`, and `Remove`

## Registry foundation

- Public registry browse page: `/plugins`
- Publishing dashboard: `/plugins/add`
- Current scope: public browsing plus authenticated metadata/version publishing
- Planned next step: native install flows backed by the same API

## Example plugin

- Rust example crate: `crates/plugin_example_status`
- Starter scaffold: `cargo run -p termy_cli -- -plugin-init`

## Manifest format

The schema is defined in `crates/plugin_core` (`PluginManifest`).

```json
{
  "schema_version": 1,
  "id": "example.hello",
  "name": "Hello Plugin",
  "version": "0.1.0",
  "description": "My first Termy plugin",
  "author": "Your Name",
  "runtime": "executable",
  "entrypoint": "./plugin",
  "autostart": true,
  "permissions": ["network", "host_events"],
  "subscribes": {
    "events": ["app_started", "theme_changed"]
  },
  "contributes": {
    "commands": [
      { "id": "hello.run", "title": "Run Hello" }
    ]
  }
}
```

## Supported runtime and protocol

- Runtime: `executable` (current)
- Protocol version: `1`
- Host sends:
  - `hello`
  - `invoke_command`
  - `event`
  - `ping`
  - `shutdown`
- Plugin sends:
  - `hello`
  - `log`
  - `toast`
  - `panel`
  - `pong`

## Permissions

Available permissions from `plugin_core`:

- `filesystem_read`
- `filesystem_write`
- `host_events`
- `network`
- `shell`
- `clipboard`
- `notifications`
- `terminal_read`
- `terminal_write`
- `ui_panels`

## Common failures

- `entrypoint does not exist`: `entrypoint` path is wrong relative to the plugin folder.
- `plugin id mismatch`: plugin runtime reports a different id than the manifest.
- Protocol mismatch: plugin and host `protocol_version` do not match.

Use `View Logs` in `Settings -> Plugins` to inspect runtime logs for each plugin.
