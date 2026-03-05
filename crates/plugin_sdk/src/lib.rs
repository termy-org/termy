use std::io::{self, BufRead, BufReader, Stdin, Stdout, Write};

use termy_plugin_core::{
    HostHello, HostRpcMessage, PLUGIN_PROTOCOL_VERSION, PluginCapability, PluginHello,
    PluginLogLevel, PluginLogMessage, PluginRpcMessage, PluginToastLevel, PluginToastMessage,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMetadata {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<PluginCapability>,
}

impl PluginMetadata {
    pub fn new(
        plugin_id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            name: name.into(),
            version: version.into(),
            capabilities: Vec::new(),
        }
    }

    pub fn with_capabilities(mut self, capabilities: Vec<PluginCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }
}

pub struct PluginSession<R, W> {
    reader: BufReader<R>,
    writer: W,
    host_hello: HostHello,
    plugin_id: String,
}

impl PluginSession<Stdin, Stdout> {
    pub fn stdio(metadata: PluginMetadata) -> Result<Self, PluginSessionError> {
        Self::initialize(io::stdin(), io::stdout(), metadata)
    }
}

impl<R, W> PluginSession<R, W>
where
    R: io::Read,
    W: Write,
{
    pub fn initialize(
        reader: R,
        mut writer: W,
        metadata: PluginMetadata,
    ) -> Result<Self, PluginSessionError> {
        let mut reader = BufReader::new(reader);
        let host_hello = read_host_hello(&mut reader)?;

        if host_hello.protocol_version != PLUGIN_PROTOCOL_VERSION {
            return Err(PluginSessionError::ProtocolVersionMismatch {
                expected: PLUGIN_PROTOCOL_VERSION,
                actual: host_hello.protocol_version,
            });
        }

        if host_hello.plugin_id != metadata.plugin_id {
            return Err(PluginSessionError::PluginIdMismatch {
                expected: metadata.plugin_id,
                actual: host_hello.plugin_id,
            });
        }

        let plugin_id = host_hello.plugin_id.clone();
        write_message(
            &mut writer,
            &PluginRpcMessage::Hello(PluginHello {
                protocol_version: PLUGIN_PROTOCOL_VERSION,
                plugin_id: plugin_id.clone(),
                name: metadata.name,
                version: metadata.version,
                capabilities: metadata.capabilities,
            }),
        )?;

        Ok(Self {
            reader,
            writer,
            host_hello,
            plugin_id,
        })
    }

    pub fn host_hello(&self) -> &HostHello {
        &self.host_hello
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn recv(&mut self) -> Result<HostRpcMessage, PluginSessionError> {
        read_host_message(&mut self.reader)
    }

    pub fn send(&mut self, message: PluginRpcMessage) -> Result<(), PluginSessionError> {
        write_message(&mut self.writer, &message)
    }

    pub fn send_log(
        &mut self,
        level: PluginLogLevel,
        message: impl Into<String>,
    ) -> Result<(), PluginSessionError> {
        self.send(PluginRpcMessage::Log(PluginLogMessage {
            level,
            message: message.into(),
        }))
    }

    pub fn send_pong(&mut self) -> Result<(), PluginSessionError> {
        self.send(PluginRpcMessage::Pong)
    }

    pub fn send_toast(
        &mut self,
        level: PluginToastLevel,
        message: impl Into<String>,
        duration_ms: Option<u64>,
    ) -> Result<(), PluginSessionError> {
        self.send(PluginRpcMessage::Toast(PluginToastMessage {
            level,
            message: message.into(),
            duration_ms,
        }))
    }

    pub fn run_until_shutdown(
        &mut self,
        mut on_message: impl FnMut(&HostRpcMessage, &mut Self) -> Result<(), PluginSessionError>,
    ) -> Result<(), PluginSessionError> {
        loop {
            let message = self.recv()?;
            let shutdown = matches!(message, HostRpcMessage::Shutdown);
            on_message(&message, self)?;
            if shutdown {
                return Ok(());
            }
        }
    }
}

fn read_host_hello<R: BufRead>(reader: &mut R) -> Result<HostHello, PluginSessionError> {
    match read_host_message(reader)? {
        HostRpcMessage::Hello(hello) => Ok(hello),
        other => Err(PluginSessionError::UnexpectedMessage(format!(
            "expected host hello, got {other:?}"
        ))),
    }
}

fn read_host_message<R: BufRead>(reader: &mut R) -> Result<HostRpcMessage, PluginSessionError> {
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        return Err(PluginSessionError::HostClosedStream);
    }
    serde_json::from_str(line.trim_end()).map_err(PluginSessionError::Json)
}

fn write_message<W: Write>(
    writer: &mut W,
    message: &PluginRpcMessage,
) -> Result<(), PluginSessionError> {
    serde_json::to_writer(&mut *writer, message)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum PluginSessionError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("host closed the plugin stream")]
    HostClosedStream,
    #[error("protocol version mismatch: expected {expected}, got {actual}")]
    ProtocolVersionMismatch { expected: u32, actual: u32 },
    #[error("plugin id mismatch: expected `{expected}`, got `{actual}`")]
    PluginIdMismatch { expected: String, actual: String },
    #[error("unexpected message: {0}")]
    UnexpectedMessage(String),
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use termy_plugin_core::{HostHello, HostRpcMessage};

    #[test]
    fn initializes_session_and_sends_plugin_hello() {
        let input = Cursor::new(
            b"{\"type\":\"hello\",\"payload\":{\"protocol_version\":1,\"host_name\":\"termy\",\"host_version\":\"0.1.44\",\"plugin_id\":\"example.hello\"}}\n"
                .to_vec(),
        );
        let mut output = Vec::new();

        let session = PluginSession::initialize(
            input,
            &mut output,
            PluginMetadata::new("example.hello", "Hello Plugin", "0.1.0")
                .with_capabilities(vec![PluginCapability::CommandProvider]),
        )
        .expect("session should initialize");

        assert_eq!(session.plugin_id(), "example.hello");
        let sent = String::from_utf8(output).expect("output utf8");
        assert!(sent.contains("\"type\":\"hello\""));
        assert!(sent.contains("\"plugin_id\":\"example.hello\""));
        assert!(sent.contains("command_provider"));
    }

    #[test]
    fn rejects_mismatched_plugin_id() {
        let input = Cursor::new(
            b"{\"type\":\"hello\",\"payload\":{\"protocol_version\":1,\"host_name\":\"termy\",\"host_version\":\"0.1.44\",\"plugin_id\":\"wrong.id\"}}\n"
                .to_vec(),
        );
        let mut output = Vec::new();

        let error = match PluginSession::initialize(
            input,
            &mut output,
            PluginMetadata::new("example.hello", "Hello Plugin", "0.1.0"),
        ) {
            Ok(_) => panic!("session should reject mismatched plugin id"),
            Err(error) => error,
        };

        assert!(matches!(error, PluginSessionError::PluginIdMismatch { .. }));
    }

    #[test]
    fn runs_until_shutdown() {
        let input = Cursor::new(
            [
                b"{\"type\":\"hello\",\"payload\":{\"protocol_version\":1,\"host_name\":\"termy\",\"host_version\":\"0.1.44\",\"plugin_id\":\"example.hello\"}}\n"
                    .as_slice(),
                b"{\"type\":\"ping\"}\n".as_slice(),
                b"{\"type\":\"shutdown\"}\n".as_slice(),
            ]
            .concat(),
        );
        let mut output = Vec::new();
        let mut session = PluginSession::initialize(
            input,
            &mut output,
            PluginMetadata::new("example.hello", "Hello Plugin", "0.1.0"),
        )
        .expect("session should initialize");
        let mut seen = Vec::new();

        session
            .run_until_shutdown(|message, session| {
                seen.push(match message {
                    HostRpcMessage::Ping => "ping",
                    HostRpcMessage::Shutdown => "shutdown",
                    HostRpcMessage::Hello(_) => "hello",
                });
                if matches!(message, HostRpcMessage::Ping) {
                    session.send_pong()?;
                }
                Ok(())
            })
            .expect("session loop should complete");

        assert_eq!(seen, vec!["ping", "shutdown"]);
        let sent = String::from_utf8(output).expect("output utf8");
        assert!(sent.contains("\"type\":\"pong\""));
    }

    #[test]
    fn sends_toast_message() {
        let input = Cursor::new(
            b"{\"type\":\"hello\",\"payload\":{\"protocol_version\":1,\"host_name\":\"termy\",\"host_version\":\"0.1.44\",\"plugin_id\":\"example.hello\"}}\n"
                .to_vec(),
        );
        let mut output = Vec::new();
        let mut session = PluginSession::initialize(
            input,
            &mut output,
            PluginMetadata::new("example.hello", "Hello Plugin", "0.1.0"),
        )
        .expect("session should initialize");

        session
            .send_toast(PluginToastLevel::Success, "toast body", Some(1200))
            .expect("toast should send");

        let sent = String::from_utf8(output).expect("output utf8");
        assert!(sent.contains("\"type\":\"toast\""));
        assert!(sent.contains("\"level\":\"success\""));
        assert!(sent.contains("\"message\":\"toast body\""));
        assert!(sent.contains("\"duration_ms\":1200"));
    }

    #[test]
    fn exposes_host_hello() {
        let input = Cursor::new(
            b"{\"type\":\"hello\",\"payload\":{\"protocol_version\":1,\"host_name\":\"termy\",\"host_version\":\"0.1.44\",\"plugin_id\":\"example.hello\"}}\n"
                .to_vec(),
        );
        let mut output = Vec::new();
        let session = PluginSession::initialize(
            input,
            &mut output,
            PluginMetadata::new("example.hello", "Hello Plugin", "0.1.0"),
        )
        .expect("session should initialize");

        assert_eq!(
            session.host_hello(),
            &HostHello {
                protocol_version: 1,
                host_name: "termy".to_string(),
                host_version: "0.1.44".to_string(),
                plugin_id: "example.hello".to_string(),
            }
        );
    }
}
