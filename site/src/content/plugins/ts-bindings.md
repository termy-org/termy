---
title: ts-bindings Reference
description: TypeScript bindings for Termy plugin protocol and runtime loop
order: 6
category: Plugins
---

`@termy-oss/ts-bindings` provides TypeScript equivalents of `plugin_core` + session helpers.

## Install

```bash
bun add @termy-oss/ts-bindings
```

## Core exports

- Constants:
  - `PLUGIN_MANIFEST_FILE_NAME`
  - `PLUGIN_PROTOCOL_VERSION`
- Types:
  - `PluginManifest`
  - `HostRpcMessage`
  - `PluginRpcMessage`
  - `PluginMetadata`
- Runtime class:
  - `PluginSession`
- Parsing helpers:
  - `parseHostRpcMessage`
  - `parsePluginRpcMessage`
  - `serializePluginRpcMessage`

## Exported API (complete)

### Constants

```ts
PLUGIN_MANIFEST_FILE_NAME
PLUGIN_PROTOCOL_VERSION
```

### Type aliases

```ts
PluginRuntime
PluginPermission
HostRpcMessage
PluginCapability
PluginLogLevel
PluginToastLevel
PluginRpcMessage
```

### Interfaces

```ts
PluginContributions
PluginCommandContribution
PluginManifest
HostHello
HostCommandInvocation
PluginHello
PluginLogMessage
PluginToastMessage
PluginMetadata
LineReader
LineWriter
```

### Classes

```ts
PluginSessionError
HostClosedStreamError
ProtocolVersionMismatchError
PluginIdMismatchError
InvalidMessageError
UnexpectedMessageError
PluginSession
```

### `PluginSession` static methods

```ts
PluginSession.stdio(metadata)
PluginSession.initialize(reader, writer, metadata)
PluginSession.commandId(message)
```

### `PluginSession` instance members

```ts
readonly hostHello
readonly pluginId
recv()
send(message)
sendLog(level, message)
sendPong()
sendToast(level, message, durationMs?)
runUntilShutdown(onMessage)
```

### Free functions

```ts
readHostHello(reader)
readHostMessage(reader)
parseHostRpcMessage(line)
parsePluginRpcMessage(line)
serializePluginRpcMessage(message)
```

## Session usage

```ts
import { PluginSession, type PluginMetadata } from "@termy-oss/ts-bindings";

const metadata: PluginMetadata = {
  pluginId: "example.hello",
  name: "Hello Plugin",
  version: "0.1.0",
  capabilities: ["command_provider"],
};

const session = await PluginSession.stdio(metadata);

await session.runUntilShutdown(async (message, s) => {
  if (message.type === "ping") {
    await s.sendPong();
    return;
  }
  if (message.type === "invoke_command") {
    await s.sendLog("info", `invoke ${message.payload.command_id}`);
    await s.sendToast("success", "Done", 1200);
  }
});
```

## Error classes

- `HostClosedStreamError`
- `ProtocolVersionMismatchError`
- `PluginIdMismatchError`
- `InvalidMessageError`
- `UnexpectedMessageError`
