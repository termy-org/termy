use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use termy_config_core::config_path;
use termy_plugin_core::{
    DiscoveredPlugin, HostHello, HostRpcMessage, PLUGIN_MANIFEST_FILE_NAME,
    PLUGIN_PROTOCOL_VERSION, PluginCapability, PluginHello, PluginManifest, PluginPermission,
    PluginRpcMessage, PluginRuntime, PluginToastLevel, PluginToastMessage,
};
use thiserror::Error;

const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub struct PluginHost {
    root_dir: PathBuf,
    running_plugins: Vec<RunningPlugin>,
    failures: Vec<PluginLoadFailure>,
}

impl PluginHost {
    pub fn load_default(host_version: &str) -> Result<Self> {
        let root_dir = default_plugins_dir().context("resolve default plugin directory")?;
        Self::load_from_dir(root_dir, host_version)
    }

    pub fn load_from_dir(root_dir: PathBuf, host_version: &str) -> Result<Self> {
        fs::create_dir_all(&root_dir)
            .with_context(|| format!("create plugin directory {}", root_dir.display()))?;

        let discovered = discover_plugins(&root_dir)?;
        let mut running_plugins = Vec::new();
        let mut failures = Vec::new();

        for plugin in discovered {
            if !plugin.manifest.autostart {
                continue;
            }

            match RunningPlugin::start(plugin, host_version, DEFAULT_HANDSHAKE_TIMEOUT) {
                Ok(plugin) => running_plugins.push(plugin),
                Err(error) => failures.push(error),
            }
        }

        Ok(Self {
            root_dir,
            running_plugins,
            failures,
        })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn running_plugins(&self) -> &[RunningPlugin] {
        &self.running_plugins
    }

    pub fn failures(&self) -> &[PluginLoadFailure] {
        &self.failures
    }
}

impl Drop for PluginHost {
    fn drop(&mut self) {
        for plugin in &mut self.running_plugins {
            let _ = plugin.shutdown();
        }
    }
}

#[derive(Debug)]
pub struct RunningPlugin {
    manifest: PluginManifest,
    root_dir: PathBuf,
    child: Child,
    stdin: ChildStdin,
    hello: PluginHello,
    runtime_thread: Option<JoinHandle<()>>,
}

impl RunningPlugin {
    fn start(
        discovered: DiscoveredPlugin,
        host_version: &str,
        handshake_timeout: Duration,
    ) -> Result<Self, PluginLoadFailure> {
        let entrypoint = discovered.resolved_entrypoint();
        if !entrypoint.exists() {
            return Err(PluginLoadFailure::new(
                discovered.manifest.id.clone(),
                anyhow!("entrypoint does not exist: {}", entrypoint.display()),
            ));
        }

        match discovered.manifest.runtime {
            PluginRuntime::Executable => {}
        }

        let mut child = Command::new(&entrypoint)
            .current_dir(&discovered.root_dir)
            .env("TERMY_PLUGIN_ID", &discovered.manifest.id)
            .env("TERMY_PLUGIN_ROOT", &discovered.root_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| PluginLoadFailure::new(discovered.manifest.id.clone(), error))?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            PluginLoadFailure::new(
                discovered.manifest.id.clone(),
                anyhow!("plugin stdin unavailable"),
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            PluginLoadFailure::new(
                discovered.manifest.id.clone(),
                anyhow!("plugin stdout unavailable"),
            )
        })?;

        let hello = HostRpcMessage::Hello(HostHello {
            protocol_version: PLUGIN_PROTOCOL_VERSION,
            host_name: "termy".to_string(),
            host_version: host_version.to_string(),
            plugin_id: discovered.manifest.id.clone(),
        });
        write_message(&mut stdin, &hello).map_err(|error| {
            let _ = child.kill();
            PluginLoadFailure::new(discovered.manifest.id.clone(), error)
        })?;

        let plugin_id = discovered.manifest.id.clone();
        let permissions = discovered.manifest.permissions.clone();
        let (plugin_hello, runtime_thread) =
            start_runtime_thread(stdout, plugin_id.clone(), permissions, handshake_timeout)
                .map_err(|error| {
                    let _ = child.kill();
                    PluginLoadFailure::new(plugin_id, error)
                })?;

        if plugin_hello.protocol_version != PLUGIN_PROTOCOL_VERSION {
            let _ = child.kill();
            return Err(PluginLoadFailure::new(
                discovered.manifest.id.clone(),
                anyhow!(
                    "protocol version mismatch: host={}, plugin={}",
                    PLUGIN_PROTOCOL_VERSION,
                    plugin_hello.protocol_version
                ),
            ));
        }

        if plugin_hello.plugin_id != discovered.manifest.id {
            let _ = child.kill();
            return Err(PluginLoadFailure::new(
                discovered.manifest.id.clone(),
                anyhow!(
                    "plugin reported unexpected id `{}` during handshake",
                    plugin_hello.plugin_id
                ),
            ));
        }

        Ok(Self {
            manifest: discovered.manifest,
            root_dir: discovered.root_dir,
            child,
            stdin,
            hello: plugin_hello,
            runtime_thread: Some(runtime_thread),
        })
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn permissions(&self) -> &[PluginPermission] {
        &self.manifest.permissions
    }

    pub fn capabilities(&self) -> &[PluginCapability] {
        &self.hello.capabilities
    }

    pub fn shutdown(&mut self) -> Result<()> {
        write_message(&mut self.stdin, &HostRpcMessage::Shutdown)
            .context("send plugin shutdown message")?;
        if self.child.try_wait().context("poll plugin exit")?.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }

        if let Some(handle) = self.runtime_thread.take() {
            let _ = handle.join();
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Error)]
#[error("failed to load plugin `{plugin_id}`: {message}")]
pub struct PluginLoadFailure {
    plugin_id: String,
    message: String,
}

impl PluginLoadFailure {
    fn new(plugin_id: String, error: impl std::fmt::Display) -> Self {
        Self {
            plugin_id,
            message: error.to_string(),
        }
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn default_plugins_dir() -> Option<PathBuf> {
    config_path().and_then(|path| path.parent().map(|parent| parent.join("plugins")))
}

pub fn discover_plugins(root_dir: &Path) -> Result<Vec<DiscoveredPlugin>> {
    if !root_dir.exists() {
        return Ok(Vec::new());
    }

    let mut discovered = Vec::new();
    for entry in fs::read_dir(root_dir)
        .with_context(|| format!("read plugin directory {}", root_dir.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        let plugin_root = entry.path();
        let manifest_path = plugin_root.join(PLUGIN_MANIFEST_FILE_NAME);
        if !manifest_path.is_file() {
            continue;
        }

        let contents = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read plugin manifest {}", manifest_path.display()))?;
        let manifest = PluginManifest::from_json_str(&contents)
            .with_context(|| format!("parse plugin manifest {}", manifest_path.display()))?;
        discovered.push(DiscoveredPlugin {
            root_dir: plugin_root,
            manifest_path,
            manifest,
        });
    }

    discovered.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
    Ok(discovered)
}

fn write_message<T: serde::Serialize>(stdin: &mut ChildStdin, message: &T) -> Result<()> {
    serde_json::to_writer(&mut *stdin, message).context("serialize plugin message")?;
    stdin.write_all(b"\n").context("terminate plugin message")?;
    stdin.flush().context("flush plugin message")?;
    Ok(())
}

fn start_runtime_thread(
    stdout: ChildStdout,
    plugin_id: String,
    permissions: Vec<PluginPermission>,
    timeout: Duration,
) -> Result<(PluginHello, JoinHandle<()>)> {
    let (sender, receiver) = mpsc::channel();
    let thread_plugin_id = plugin_id.clone();
    let handle = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = (|| -> Result<PluginHello> {
            let bytes = reader
                .read_line(&mut line)
                .context("read plugin handshake")?;
            if bytes == 0 {
                return Err(anyhow!("plugin exited before handshake completed"));
            }
            match serde_json::from_str::<PluginRpcMessage>(line.trim_end())
                .context("parse plugin handshake JSON")?
            {
                PluginRpcMessage::Hello(hello) => Ok(hello),
                other => Err(anyhow!("expected plugin hello message, got {other:?}")),
            }
        })();
        let _ = sender.send(result);

        line.clear();
        loop {
            match reader.read_line(&mut line) {
                Ok(0) => {
                    log::debug!("Plugin {thread_plugin_id} closed stdout");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim_end();
                    match serde_json::from_str::<PluginRpcMessage>(trimmed) {
                        Ok(message) => {
                            log_plugin_runtime_message(&thread_plugin_id, &permissions, message)
                        }
                        Err(error) => {
                            log::warn!(
                                "Plugin {} emitted invalid JSON message: {} ({})",
                                thread_plugin_id,
                                trimmed,
                                error
                            );
                        }
                    }
                    line.clear();
                }
                Err(error) => {
                    log::warn!("Plugin {} stdout read failed: {}", thread_plugin_id, error);
                    break;
                }
            }
        }
    });

    let hello = receiver.recv_timeout(timeout).map_err(|_| {
        anyhow!(
            "plugin handshake timed out after {} ms",
            timeout.as_millis()
        )
    })??;

    Ok((hello, handle))
}

fn log_plugin_runtime_message(
    plugin_id: &str,
    permissions: &[PluginPermission],
    message: PluginRpcMessage,
) {
    match message {
        PluginRpcMessage::Hello(_) => {
            log::warn!(
                "Plugin {} sent duplicate hello message after handshake",
                plugin_id
            );
        }
        PluginRpcMessage::Log(log_message) => match log_message.level {
            termy_plugin_core::PluginLogLevel::Trace => {
                log::trace!("Plugin {}: {}", plugin_id, log_message.message)
            }
            termy_plugin_core::PluginLogLevel::Debug => {
                log::debug!("Plugin {}: {}", plugin_id, log_message.message)
            }
            termy_plugin_core::PluginLogLevel::Info => {
                log::info!("Plugin {}: {}", plugin_id, log_message.message)
            }
            termy_plugin_core::PluginLogLevel::Warn => {
                log::warn!("Plugin {}: {}", plugin_id, log_message.message)
            }
            termy_plugin_core::PluginLogLevel::Error => {
                log::error!("Plugin {}: {}", plugin_id, log_message.message)
            }
        },
        PluginRpcMessage::Pong => {
            log::debug!("Plugin {} responded with pong", plugin_id);
        }
        PluginRpcMessage::Toast(toast_message) => {
            if !permissions.contains(&PluginPermission::Notifications) {
                log::warn!(
                    "Plugin {} attempted to send a toast without notifications permission",
                    plugin_id
                );
                return;
            }
            enqueue_plugin_toast(plugin_id, toast_message);
        }
    }
}

fn enqueue_plugin_toast(plugin_id: &str, toast_message: PluginToastMessage) {
    let kind = match toast_message.level {
        PluginToastLevel::Info => termy_toast::ToastKind::Info,
        PluginToastLevel::Success => termy_toast::ToastKind::Success,
        PluginToastLevel::Warning => termy_toast::ToastKind::Warning,
        PluginToastLevel::Error => termy_toast::ToastKind::Error,
    };
    let message = format!("{}: {}", plugin_id, toast_message.message);
    let duration = toast_message.duration_ms.map(Duration::from_millis);
    termy_toast::enqueue_toast(kind, message, duration);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("termy-plugin-host-{name}-{suffix}"))
    }

    #[test]
    fn discovers_plugins_from_directory() {
        let root = unique_temp_dir("discover");
        let plugin_dir = root.join("hello-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILE_NAME),
            r#"{
                "schema_version": 1,
                "id": "example.hello",
                "name": "Hello Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh"
            }"#,
        )
        .expect("write manifest");

        let discovered = discover_plugins(&root).expect("discover plugins");
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].manifest.id, "example.hello");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_from_dir_skips_non_autostart_plugins() {
        let root = unique_temp_dir("autostart");
        let plugin_dir = root.join("hello-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILE_NAME),
            r#"{
                "schema_version": 1,
                "id": "example.hello",
                "name": "Hello Plugin",
                "version": "0.1.0",
                "entrypoint": "./missing-plugin",
                "autostart": false
            }"#,
        )
        .expect("write manifest");

        let host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");
        assert!(host.running_plugins().is_empty());
        assert!(host.failures().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn starts_plugin_and_completes_handshake() {
        use std::os::unix::fs::PermissionsExt;

        let root = unique_temp_dir("handshake");
        let plugin_dir = root.join("hello-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        let script_path = plugin_dir.join("plugin.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.hello","name":"Hello Plugin","version":"0.1.0","capabilities":["command_provider"]}}'
while read line; do
  if [ "$line" = '{"type":"shutdown"}' ]; then
    exit 0
  fi
done
"#,
        )
        .expect("write plugin script");
        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set script mode");
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILE_NAME),
            r#"{
                "schema_version": 1,
                "id": "example.hello",
                "name": "Hello Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh",
                "permissions": ["network"]
            }"#,
        )
        .expect("write manifest");

        let host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");
        assert_eq!(host.running_plugins().len(), 1);
        assert_eq!(host.running_plugins()[0].id(), "example.hello");
        assert_eq!(
            host.running_plugins()[0].capabilities(),
            &[PluginCapability::CommandProvider]
        );

        drop(host);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn plugin_with_notifications_permission_can_enqueue_toast() {
        use std::os::unix::fs::PermissionsExt;

        let _ = termy_toast::drain_pending();

        let root = unique_temp_dir("toast");
        let plugin_dir = root.join("toast-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        let script_path = plugin_dir.join("plugin.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.toast","name":"Toast Plugin","version":"0.1.0","capabilities":[]}}'
printf '%s\n' '{"type":"toast","payload":{"level":"success","message":"toast from plugin","duration_ms":900}}'
while read line; do
  if [ "$line" = '{"type":"shutdown"}' ]; then
    exit 0
  fi
done
"#,
        )
        .expect("write plugin script");
        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set script mode");
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILE_NAME),
            r#"{
                "schema_version": 1,
                "id": "example.toast",
                "name": "Toast Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh",
                "permissions": ["notifications"]
            }"#,
        )
        .expect("write manifest");

        let host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");
        thread::sleep(Duration::from_millis(50));
        let pending = termy_toast::drain_pending();

        assert_eq!(host.running_plugins().len(), 1);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, termy_toast::ToastKind::Success);
        assert_eq!(pending[0].message, "example.toast: toast from plugin");

        drop(host);
        let _ = fs::remove_dir_all(root);
    }
}
