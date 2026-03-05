use termy_plugin_core::{HostRpcMessage, PluginCapability, PluginLogLevel, PluginToastLevel};
use termy_plugin_sdk::{PluginMetadata, PluginSession};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = PluginMetadata::new("example.hello-rust", "Hello Rust Plugin", "0.1.0")
        .with_capabilities(vec![PluginCapability::CommandProvider]);
    let mut session = PluginSession::stdio(metadata)?;

    session.send_log(
        PluginLogLevel::Info,
        format!(
            "connected to {} {}",
            session.host_hello().host_name,
            session.host_hello().host_version
        ),
    )?;
    session.send_toast(
        PluginToastLevel::Info,
        "hello from the Rust plugin",
        Some(2500),
    )?;

    session.run_until_shutdown(|message, session| {
        if matches!(message, HostRpcMessage::Ping) {
            session.send_pong()?;
        }
        Ok(())
    })?;

    Ok(())
}
