---
title: go-bindings Reference
description: Go bindings for Termy plugin protocol and runtime loop
order: 7
category: Plugins
---

`github.com/lassejlv/termy/bindings/go-bindings` provides Go equivalents of `plugin_core` + session helpers.

## Install

```bash
go get github.com/lassejlv/termy/bindings/go-bindings
```

## Exported API (complete)

### Constants

```go
PluginManifestFileName
PluginProtocolVersion
```

### Types

```go
PluginRuntime
PluginPermission
PluginContributions
PluginCommandContribution
PluginManifest
HostHello
HostCommandInvocation
HostRPCMessage
PluginCapability
PluginHello
PluginLogLevel
PluginLogMessage
PluginToastLevel
PluginToastMessage
PluginRPCMessage
PluginMetadata
PluginSession
ProtocolVersionMismatchError
PluginIDMismatchError
UnexpectedMessageError
```

### Variables

```go
ErrHostClosedStream
```

### `HostRPCMessage` methods

```go
(HostRPCMessage).AsHello() (HostHello, error)
(HostRPCMessage).AsInvokeCommand() (HostCommandInvocation, error)
```

### Error methods

```go
(ProtocolVersionMismatchError).Error() string
(PluginIDMismatchError).Error() string
(UnexpectedMessageError).Error() string
```

### Session and protocol functions

```go
NewStdioSession(metadata PluginMetadata) (*PluginSession, error)
NewSession(reader *bufio.Reader, writer io.Writer, metadata PluginMetadata) (*PluginSession, error)
CommandID(message HostRPCMessage) (string, bool)
ReadHostHello(reader *bufio.Reader) (HostHello, error)
ReadHostMessage(reader *bufio.Reader) (HostRPCMessage, error)
ParseHostRPCMessage(line []byte) (HostRPCMessage, error)
ParsePluginRPCMessage(line []byte) (PluginRPCMessage, error)
SerializePluginRPCMessage(message PluginRPCMessage) ([]byte, error)
NewPluginHelloMessage(payload PluginHello) PluginRPCMessage
NewPluginLogMessage(level PluginLogLevel, message string) PluginRPCMessage
NewPluginToastMessage(level PluginToastLevel, message string, durationMS *uint64) PluginRPCMessage
NewPluginPongMessage() PluginRPCMessage
```

### `PluginSession` methods

```go
(*PluginSession).Recv() (HostRPCMessage, error)
(*PluginSession).Send(message PluginRPCMessage) error
(*PluginSession).SendLog(level PluginLogLevel, message string) error
(*PluginSession).SendPong() error
(*PluginSession).SendToast(level PluginToastLevel, message string, durationMS *uint64) error
(*PluginSession).RunUntilShutdown(onMessage func(message HostRPCMessage, session *PluginSession) error) error
```

## Session usage

```go
package main

import (
	"log"

	termybindings "github.com/lassejlv/termy/bindings/go-bindings"
)

func main() {
	session, err := termybindings.NewStdioSession(termybindings.PluginMetadata{
		PluginID:     "example.hello",
		Name:         "Hello Plugin",
		Version:      "0.1.0",
		Capabilities: []termybindings.PluginCapability{termybindings.PluginCapabilityCommandProvider},
	})
	if err != nil {
		log.Fatal(err)
	}

	err = session.RunUntilShutdown(func(message termybindings.HostRPCMessage, current *termybindings.PluginSession) error {
		switch message.Type {
		case "ping":
			return current.SendPong()
		case "invoke_command":
			commandID, ok := termybindings.CommandID(message)
			if ok {
				return current.SendLog(termybindings.PluginLogLevelInfo, "invoke "+commandID)
			}
		}
		return nil
	})
	if err != nil {
		log.Fatal(err)
	}
}
```

## Errors

- `ErrHostClosedStream`
- `ProtocolVersionMismatchError`
- `PluginIDMismatchError`
- `UnexpectedMessageError`
