# Plugin System

Termy plugins are isolated executables discovered from the Termy config directory.

## Current shape

- Plugins live under `<config-dir>/plugins/<plugin-id>/`
- Each plugin directory must contain `termy-plugin.json`
- `entrypoint` may be relative to the plugin directory or absolute
- Plugins are launched out-of-process over stdio using newline-delimited JSON messages
- Unresponsive or invalid plugins are rejected during startup handshake
- Rust plugin authors can use `termy_plugin_sdk` for the handshake and message loop

## Manifest

Example:

```json
{
  "schema_version": 1,
  "id": "example.hello",
  "name": "Hello Plugin",
  "version": "0.1.0",
  "description": "Minimal example plugin",
  "runtime": "executable",
  "entrypoint": "./plugin.sh",
  "autostart": true,
  "permissions": ["network"],
  "contributes": {
    "commands": [
      {
        "id": "example.hello.run",
        "title": "Run Hello"
      }
    ]
  }
}
```

## Handshake

Host to plugin:

```json
{"type":"hello","payload":{"protocol_version":1,"host_name":"termy","host_version":"0.1.44","plugin_id":"example.hello"}}
```

Plugin to host:

```json
{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.hello","name":"Hello Plugin","version":"0.1.0","capabilities":["command_provider"]}}
```

Shutdown:

```json
{"type":"shutdown"}
```

## SDK

`termy_plugin_sdk` currently provides:

- stdio session bootstrap
- host hello validation
- plugin hello emission
- typed receive/send helpers
- toast helper
- a `run_until_shutdown` loop

Rust example:

```rust
use termy_plugin_core::{HostRpcMessage, PluginCapability};
use termy_plugin_sdk::{PluginMetadata, PluginSession};

let metadata = PluginMetadata::new("example.hello-rust", "Hello Rust Plugin", "0.1.0")
    .with_capabilities(vec![PluginCapability::CommandProvider]);
let mut session = PluginSession::stdio(metadata)?;

session.run_until_shutdown(|message, session| {
    if matches!(message, HostRpcMessage::Ping) {
        session.send_pong()?;
    }
    Ok(())
})?;
```

Plugins can request host toasts by declaring `"notifications"` in their manifest permissions and calling:

```rust
session.send_toast(PluginToastLevel::Info, "hello from plugin", Some(2500))?;
```

## First-phase capabilities

- plugin discovery
- manifest validation
- stdio process launch
- protocol version check
- runtime plugin log consumption
- startup failure isolation
- plugin shutdown on host drop

## CLI inspection

Use the CLI to inspect discovered manifests without starting the UI:

```bash
cargo run -p termy_cli -- -list-plugins
```

## Not implemented yet

- command dispatch into plugins
- UI panels
- event subscriptions
- permission enforcement beyond manifest declaration
- plugin installation UX
- registry/marketplace
