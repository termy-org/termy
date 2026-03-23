---
title: plugin_sdk (Rust) Reference
description: Rust SDK usage for plugin handshake and message handling
order: 5
category: Plugins
---

`termy_plugin_sdk` is the Rust helper crate for implementing plugin processes.

## Exported API (complete)

Core types:

- `PluginMetadata`
- `PluginSession<R, W>`
- `PluginSessionError`

### `PluginMetadata` methods

```rust
PluginMetadata::new(plugin_id, name, version) -> PluginMetadata
PluginMetadata::with_capabilities(capabilities) -> PluginMetadata
```

### `PluginSession<Stdin, Stdout>` methods

```rust
PluginSession::stdio(metadata) -> Result<PluginSession<Stdin, Stdout>, PluginSessionError>
```

### `PluginSession<R, W>` methods

```rust
PluginSession::initialize(reader, writer, metadata) -> Result<PluginSession<R, W>, PluginSessionError>
PluginSession::host_hello(&self) -> &HostHello
PluginSession::plugin_id(&self) -> &str
PluginSession::recv(&mut self) -> Result<HostRpcMessage, PluginSessionError>
PluginSession::send(&mut self, message: PluginRpcMessage) -> Result<(), PluginSessionError>
PluginSession::send_log(&mut self, level: PluginLogLevel, message) -> Result<(), PluginSessionError>
PluginSession::send_pong(&mut self) -> Result<(), PluginSessionError>
PluginSession::send_toast(&mut self, level: PluginToastLevel, message, duration_ms) -> Result<(), PluginSessionError>
PluginSession::send_panel(&mut self, title, body) -> Result<(), PluginSessionError>
PluginSession::send_panel_with_actions(&mut self, title, body, actions) -> Result<(), PluginSessionError>
PluginSession::command_id(message: &HostRpcMessage) -> Option<&str>
PluginSession::event(message: &HostRpcMessage) -> Option<&HostEvent>
PluginSession::run_until_shutdown(&mut self, on_message) -> Result<(), PluginSessionError>
```

### `PluginSessionError` variants

```rust
Io
Json
HostClosedStream
ProtocolVersionMismatch
PluginIdMismatch
UnexpectedMessage
```

## Session setup

Use stdio transport:

```rust
use termy_plugin_sdk::{PluginMetadata, PluginSession};

let metadata = PluginMetadata::new("example.hello", "Hello Plugin", "0.1.0");
let mut session = PluginSession::stdio(metadata)?;
```

Initialization validates:

- host `protocol_version`
- `plugin_id` match

Then SDK sends plugin `hello` automatically.

## Receive / send

- `recv()` -> `HostRpcMessage`
- `send(PluginRpcMessage)`
- `send_log(level, message)`
- `send_toast(level, message, duration_ms)`
- `send_panel(title, body)`
- `send_panel_with_actions(title, body, actions)`
- `send_pong()`

Convenience:

- `PluginSession::command_id(&HostRpcMessage)`
- `PluginSession::event(&HostRpcMessage)`
- `run_until_shutdown(handler)`

## Typical handler pattern

```rust
session.run_until_shutdown(|message, session| {
    match message {
        termy_plugin_core::HostRpcMessage::Ping => session.send_pong()?,
        termy_plugin_core::HostRpcMessage::InvokeCommand(payload) => {
            session.send_log(termy_plugin_core::PluginLogLevel::Info, format!("invoke {}", payload.command_id))?;
        }
        _ => {}
    }
    Ok(())
})?;
```
