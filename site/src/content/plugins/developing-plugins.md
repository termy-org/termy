---
title: Developing Plugins
description: Build Termy plugins with Rust, Go, or TypeScript bindings
order: 2
category: Plugins
---

Termy plugin processes communicate with the host over newline-delimited JSON on stdio.

For strongly-typed APIs use:

- Rust: `crates/plugin_sdk`
- Go: `bindings/go-bindings`
- TypeScript: `bindings/ts-bindings`

Detailed API references:

- [plugin_sdk (Rust) Reference](/docs/plugins/plugin-sdk-rust)
- [go-bindings Reference](/docs/plugins/go-bindings)
- [ts-bindings Reference](/docs/plugins/ts-bindings)
- [plugin_core Reference](/docs/plugins/plugin-core)
- [plugin_host Reference](/docs/plugins/plugin-host)

Reference implementation:

- `crates/plugin_example_status`
- `examples/plugin-full`

## Rust plugin quickstart

```rust
use termy_plugin_core::{HostRpcMessage, PluginCapability, PluginLogLevel, PluginToastLevel};
use termy_plugin_sdk::{PluginMetadata, PluginSession};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = PluginMetadata::new("example.hello", "Hello Plugin", "0.1.0")
        .with_capabilities(vec![PluginCapability::CommandProvider]);

    let mut session = PluginSession::stdio(metadata)?;

    session.run_until_shutdown(|message, session| {
        match message {
            HostRpcMessage::Ping => {
                session.send_pong()?;
            }
            HostRpcMessage::InvokeCommand(payload) => {
                session.send_log(
                    PluginLogLevel::Info,
                    format!("command invoked: {}", payload.command_id),
                )?;
                session.send_toast(
                    PluginToastLevel::Success,
                    "Hello from plugin",
                    Some(1500),
                )?;
            }
            HostRpcMessage::Shutdown => {}
            HostRpcMessage::Hello(_) => {}
        }
        Ok(())
    })?;

    Ok(())
}
```

## TypeScript plugin quickstart

```ts
import { PluginSession, type PluginMetadata } from "@termy-oss/ts-bindings";

async function main(): Promise<void> {
  const metadata: PluginMetadata = {
    pluginId: "example.hello",
    name: "Hello Plugin",
    version: "0.1.0",
    capabilities: ["command_provider"],
  };

  const session = await PluginSession.stdio(metadata);

  await session.runUntilShutdown(async (message, activeSession) => {
    if (message.type === "ping") {
      await activeSession.sendPong();
      return;
    }

    if (message.type === "invoke_command") {
      await activeSession.sendLog("info", `command invoked: ${message.payload.command_id}`);
      await activeSession.sendToast("success", "Hello from plugin", 1500);
    }
  });
}

void main();
```

## Go plugin quickstart

```go
package main

import (
	"log"

	termybindings "github.com/lassejlv/termy/bindings/go-bindings"
)

func main() {
	session, err := termybindings.NewStdioSession(termybindings.PluginMetadata{
		PluginID: "example.hello",
		Name:     "Hello Plugin",
		Version:  "0.1.0",
	})
	if err != nil {
		log.Fatal(err)
	}

	err = session.RunUntilShutdown(func(message termybindings.HostRPCMessage, current *termybindings.PluginSession) error {
		switch message.Type {
		case "ping":
			return current.SendPong()
		case "invoke_command":
			return current.SendLog(termybindings.PluginLogLevelInfo, "command invoked")
		}
		return nil
	})
	if err != nil {
		log.Fatal(err)
	}
}
```

## Handshake rules

- Host sends `hello` first.
- Plugin must answer with `hello` using the same `plugin_id`.
- `protocol_version` must match `PLUGIN_PROTOCOL_VERSION` (`1`).

SDKs enforce this and return a typed error when invalid.

## Command contributions

Expose commands in your manifest:

```json
{
  "contributes": {
    "commands": [
      { "id": "example.hello", "title": "Example: Hello" }
    ]
  }
}
```

When invoked, host sends `invoke_command` with the `command_id`.

## Logging and toasts

- Use `send_log` / `sendLog` for diagnostics.
- Use `send_toast` / `sendToast` for user-visible feedback.
- Runtime logs are visible via `Settings -> Plugins -> View Logs`.

## Panels and panel actions

- Use `send_panel` for read-only settings panel content.
- Use `send_panel_with_actions` to attach buttons that invoke contributed commands.
- See `crates/plugin_example_status` for a full Rust example using commands, events, toasts, and panel actions.

## Environment variables

The host starts plugins with:

- `TERMY_PLUGIN_ID`
- `TERMY_PLUGIN_ROOT`

Use these for runtime context or locating plugin-local assets.
