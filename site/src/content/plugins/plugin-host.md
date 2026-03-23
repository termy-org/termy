---
title: plugin_host Reference
description: Loading, lifecycle, and logging behavior in termy_plugin_host
order: 4
category: Plugins
---

`termy_plugin_host` owns plugin discovery, process lifecycle, command invocation, and runtime log capture.

## Exported API (complete)

### Types

- `PluginHost`
- `RunningPlugin`
- `PluginLoadFailure`
- `DiscoveredPlugin` (re-exported dependency type in signatures)

### `PluginHost` methods

```rust
PluginHost::load_default(host_version: &str) -> Result<PluginHost>
PluginHost::load_from_dir(root_dir: PathBuf, host_version: &str) -> Result<PluginHost>
PluginHost::root_dir(&self) -> &Path
PluginHost::running_plugins(&self) -> &[RunningPlugin]
PluginHost::failures(&self) -> &[PluginLoadFailure]
PluginHost::recent_logs(&self, plugin_id: &str) -> Vec<String>
PluginHost::start_plugin(&mut self, plugin_id: &str) -> Result<(), PluginLoadFailure>
PluginHost::stop_plugin(&mut self, plugin_id: &str) -> Result<(), String>
PluginHost::invoke_command(&mut self, plugin_id: &str, command_id: &str) -> Result<(), String>
```

### `RunningPlugin` methods

```rust
RunningPlugin::id(&self) -> &str
RunningPlugin::name(&self) -> &str
RunningPlugin::version(&self) -> &str
RunningPlugin::root_dir(&self) -> &Path
RunningPlugin::permissions(&self) -> &[PluginPermission]
RunningPlugin::capabilities(&self) -> &[PluginCapability]
RunningPlugin::shutdown(&mut self) -> Result<()>
RunningPlugin::invoke_command(&mut self, command_id: &str) -> Result<()>
```

### `PluginLoadFailure` methods

```rust
PluginLoadFailure::plugin_id(&self) -> &str
PluginLoadFailure::message(&self) -> &str
```

### Free functions

```rust
default_plugins_dir() -> Option<PathBuf>
discover_plugins(root_dir: &Path) -> Result<Vec<DiscoveredPlugin>>
```

## Discovery

Host discovers plugin folders containing `termy-plugin.json`.

## Startup behavior

- Plugins with `autostart: true` are started during host load.
- Entrypoint is resolved from manifest and must exist.
- Runtime handshake requires matching:
  - `protocol_version`
  - `plugin_id`

## Runtime logging

- Host stores recent plugin logs in-memory (bounded history per plugin).
- Logs come from plugin `log` messages and host-side lifecycle events.
- UI surfaces these via `Settings -> Plugins -> View Logs`.

## Failures

Load/startup failures are captured as `PluginLoadFailure` and visible through:

- `PluginHost::failures()`
- plugin inventory in settings
