use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use termy_config_core::config_path;
use termy_plugin_core::{
    DiscoveredPlugin, HostCommandInvocation, HostHello, HostRpcMessage, PluginCapability,
    PluginHello, PluginManifest, PluginPermission, PluginRpcMessage, PluginRuntime,
    PluginToastLevel, PluginToastMessage, PLUGIN_MANIFEST_FILE_NAME, PLUGIN_PROTOCOL_VERSION,
};
use thiserror::Error;

const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_LOG_LINES_PER_PLUGIN: usize = 40;

type SharedPluginLogs = Arc<Mutex<HashMap<String, VecDeque<String>>>>;

#[derive(Debug)]
pub struct PluginHost {
    root_dir: PathBuf,
    host_version: String,
    running_plugins: Vec<RunningPlugin>,
    failures: Vec<PluginLoadFailure>,
    logs: SharedPluginLogs,
}

impl PluginHost {
    pub fn load_default(host_version: &str) -> Result<Self> {
        let root_dir = default_plugins_dir().context("resolve default plugin directory")?;
        Self::load_from_dir(root_dir, host_version)
    }

    pub fn load_from_dir(root_dir: PathBuf, host_version: &str) -> Result<Self> {
        fs::create_dir_all(&root_dir)
            .with_context(|| format!("create plugin directory {}", root_dir.display()))?;

        let mut host = Self {
            root_dir,
            host_version: host_version.to_string(),
            running_plugins: Vec::new(),
            failures: Vec::new(),
            logs: Arc::new(Mutex::new(HashMap::new())),
        };

        for plugin in discover_plugins(&host.root_dir)? {
            if !plugin.manifest.autostart {
                continue;
            }
            let _ = host.start_plugin_from_discovered(plugin);
        }

        Ok(host)
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

    pub fn recent_logs(&self, plugin_id: &str) -> Vec<String> {
        self.logs
            .lock()
            .ok()
            .and_then(|logs| logs.get(plugin_id).cloned())
            .map(|logs| logs.into_iter().collect())
            .unwrap_or_default()
    }

    pub fn start_plugin(&mut self, plugin_id: &str) -> Result<(), PluginLoadFailure> {
        if self
            .running_plugins
            .iter()
            .any(|plugin| plugin.id() == plugin_id)
        {
            return Ok(());
        }

        let discovered = discover_plugins(&self.root_dir)
            .map_err(|error| PluginLoadFailure::new(plugin_id.to_string(), error))?
            .into_iter()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .ok_or_else(|| {
                PluginLoadFailure::new(plugin_id.to_string(), anyhow!("plugin manifest not found"))
            })?;

        self.start_plugin_from_discovered(discovered)
    }

    pub fn stop_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        let Some(index) = self
            .running_plugins
            .iter()
            .position(|plugin| plugin.id() == plugin_id)
        else {
            return Err(format!("Plugin `{plugin_id}` is not running"));
        };

        let mut plugin = self.running_plugins.remove(index);
        plugin.shutdown().map_err(|error| error.to_string())?;
        self.record_log(plugin_id, "plugin stopped by host".to_string());
        self.failures
            .retain(|failure| failure.plugin_id() != plugin_id);
        Ok(())
    }

    pub fn invoke_command(&mut self, plugin_id: &str, command_id: &str) -> Result<(), String> {
        let Some(plugin) = self
            .running_plugins
            .iter_mut()
            .find(|plugin| plugin.id() == plugin_id)
        else {
            return Err(format!("Plugin `{plugin_id}` is not running"));
        };

        if !plugin.has_capability(PluginCapability::CommandProvider) {
            return Err(format!(
                "Plugin `{plugin_id}` does not advertise the `command_provider` capability"
            ));
        }

        if !plugin.contributes_command(command_id) {
            return Err(format!(
                "Plugin `{plugin_id}` does not contribute command `{command_id}`"
            ));
        }

        plugin
            .invoke_command(command_id)
            .map_err(|error| error.to_string())?;
        self.record_log(plugin_id, format!("invoked command `{command_id}`"));
        Ok(())
    }

    fn start_plugin_from_discovered(
        &mut self,
        discovered: DiscoveredPlugin,
    ) -> Result<(), PluginLoadFailure> {
        let plugin_id = discovered.manifest.id.clone();
        self.failures
            .retain(|failure| failure.plugin_id() != plugin_id);

        match RunningPlugin::start(
            discovered,
            &self.host_version,
            DEFAULT_HANDSHAKE_TIMEOUT,
            self.logs.clone(),
        ) {
            Ok(plugin) => {
                self.record_log(&plugin_id, "plugin started".to_string());
                self.running_plugins.push(plugin);
                Ok(())
            }
            Err(error) => {
                self.record_log(
                    &plugin_id,
                    format!("plugin failed to start: {}", error.message()),
                );
                self.failures.push(error.clone());
                Err(error)
            }
        }
    }

    fn record_log(&self, plugin_id: &str, line: String) {
        record_plugin_log(&self.logs, plugin_id, line);
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
        logs: SharedPluginLogs,
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
        let (plugin_hello, runtime_thread) = start_runtime_thread(
            stdout,
            plugin_id.clone(),
            permissions,
            logs,
            handshake_timeout,
        )
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

        if !discovered.manifest.contributes.commands.is_empty()
            && !plugin_hello
                .capabilities
                .contains(&PluginCapability::CommandProvider)
        {
            let _ = child.kill();
            return Err(PluginLoadFailure::new(
                discovered.manifest.id.clone(),
                anyhow!("plugin contributes commands but does not advertise `command_provider`"),
            ));
        }

        if plugin_hello
            .capabilities
            .contains(&PluginCapability::UiPanel)
            && !discovered
                .manifest
                .permissions
                .contains(&PluginPermission::UiPanels)
        {
            let _ = child.kill();
            return Err(PluginLoadFailure::new(
                discovered.manifest.id.clone(),
                anyhow!(
                    "plugin advertises `ui_panel` but manifest is missing `ui_panels` permission"
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

    fn has_capability(&self, capability: PluginCapability) -> bool {
        self.hello.capabilities.contains(&capability)
    }

    fn contributes_command(&self, command_id: &str) -> bool {
        self.manifest
            .contributes
            .commands
            .iter()
            .any(|command| command.id == command_id)
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

    pub fn invoke_command(&mut self, command_id: &str) -> Result<()> {
        write_message(
            &mut self.stdin,
            &HostRpcMessage::InvokeCommand(HostCommandInvocation {
                command_id: command_id.to_string(),
            }),
        )
        .context("send plugin command invocation")
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
    logs: SharedPluginLogs,
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
                    record_plugin_log(&logs, &thread_plugin_id, "stdout closed".to_string());
                    log::debug!("Plugin {thread_plugin_id} closed stdout");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim_end();
                    match serde_json::from_str::<PluginRpcMessage>(trimmed) {
                        Ok(message) => log_plugin_runtime_message(
                            &thread_plugin_id,
                            &permissions,
                            &logs,
                            message,
                        ),
                        Err(error) => {
                            let entry = format!("invalid JSON message: {} ({})", trimmed, error);
                            record_plugin_log(&logs, &thread_plugin_id, entry.clone());
                            log::warn!("Plugin {} emitted {}", thread_plugin_id, entry);
                        }
                    }
                    line.clear();
                }
                Err(error) => {
                    let entry = format!("stdout read failed: {error}");
                    record_plugin_log(&logs, &thread_plugin_id, entry.clone());
                    log::warn!("Plugin {} {}", thread_plugin_id, entry);
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
    logs: &SharedPluginLogs,
    message: PluginRpcMessage,
) {
    match message {
        PluginRpcMessage::Hello(_) => {
            let entry = "duplicate hello after handshake".to_string();
            record_plugin_log(logs, plugin_id, entry.clone());
            log::warn!("Plugin {} {}", plugin_id, entry);
        }
        PluginRpcMessage::Log(log_message) => {
            record_plugin_log(logs, plugin_id, log_message.message.clone());
            match log_message.level {
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
            }
        }
        PluginRpcMessage::Pong => {
            record_plugin_log(logs, plugin_id, "pong".to_string());
            log::debug!("Plugin {} responded with pong", plugin_id);
        }
        PluginRpcMessage::Toast(toast_message) => {
            if !permissions.contains(&PluginPermission::Notifications) {
                let entry = "toast rejected: notifications permission missing".to_string();
                record_plugin_log(logs, plugin_id, entry.clone());
                log::warn!("Plugin {} {}", plugin_id, entry);
                return;
            }
            record_plugin_log(logs, plugin_id, format!("toast: {}", toast_message.message));
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

fn record_plugin_log(logs: &SharedPluginLogs, plugin_id: &str, line: String) {
    let Ok(mut logs) = logs.lock() else {
        return;
    };
    let queue = logs.entry(plugin_id.to_string()).or_default();
    queue.push_back(line);
    while queue.len() > MAX_LOG_LINES_PER_PLUGIN {
        queue.pop_front();
    }
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
        assert!(host
            .recent_logs("example.toast")
            .iter()
            .any(|line| line.contains("toast")));

        drop(host);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn can_stop_and_restart_plugin() {
        use std::os::unix::fs::PermissionsExt;

        let root = unique_temp_dir("restart");
        let plugin_dir = root.join("toggle-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        let script_path = plugin_dir.join("plugin.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.toggle","name":"Toggle Plugin","version":"0.1.0","capabilities":[]}}'
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
                "id": "example.toggle",
                "name": "Toggle Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh",
                "autostart": false
            }"#,
        )
        .expect("write manifest");

        let mut host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");
        assert!(host.running_plugins().is_empty());

        host.start_plugin("example.toggle").expect("start plugin");
        assert_eq!(host.running_plugins().len(), 1);

        host.stop_plugin("example.toggle").expect("stop plugin");
        assert!(host.running_plugins().is_empty());

        host.start_plugin("example.toggle").expect("restart plugin");
        assert_eq!(host.running_plugins().len(), 1);
        assert!(host
            .recent_logs("example.toggle")
            .iter()
            .any(|line| line.contains("started")));

        drop(host);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn invoking_plugin_command_reaches_plugin() {
        use std::os::unix::fs::PermissionsExt;

        let _ = termy_toast::drain_pending();

        let root = unique_temp_dir("invoke");
        let plugin_dir = root.join("invoke-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        let script_path = plugin_dir.join("plugin.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.invoke","name":"Invoke Plugin","version":"0.1.0","capabilities":["command_provider"]}}'
while read line; do
  if [ "$line" = '{"type":"invoke_command","payload":{"command_id":"example.invoke.run"}}' ]; then
    printf '%s\n' '{"type":"log","payload":{"level":"info","message":"invoke command received"}}'
    printf '%s\n' '{"type":"toast","payload":{"level":"success","message":"invoke worked","duration_ms":500}}'
  fi
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
                "id": "example.invoke",
                "name": "Invoke Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh",
                "permissions": ["notifications"],
                "contributes": {
                    "commands": [
                        { "id": "example.invoke.run", "title": "Run" }
                    ]
                }
            }"#,
        )
        .expect("write manifest");

        let mut host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");
        host.start_plugin("example.invoke").ok();
        host.invoke_command("example.invoke", "example.invoke.run")
            .expect("invoke command");
        thread::sleep(Duration::from_millis(50));
        let pending = termy_toast::drain_pending();

        assert!(pending
            .iter()
            .any(|toast| toast.message.contains("invoke worked")));
        assert!(host
            .recent_logs("example.invoke")
            .iter()
            .any(|line| line.contains("invoke")));

        drop(host);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_command_invocation_for_plugins_without_command_provider_capability() {
        use std::os::unix::fs::PermissionsExt;

        let root = unique_temp_dir("invoke-no-capability");
        let plugin_dir = root.join("invoke-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        let script_path = plugin_dir.join("plugin.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.invoke","name":"Invoke Plugin","version":"0.1.0","capabilities":[]}}'
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
                "id": "example.invoke",
                "name": "Invoke Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh"
            }"#,
        )
        .expect("write manifest");

        let mut host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");
        host.start_plugin("example.invoke").expect("start plugin");
        let error = host
            .invoke_command("example.invoke", "example.invoke.run")
            .expect_err("command invocation should fail");

        assert!(error.contains("does not advertise the `command_provider` capability"));

        drop(host);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_command_invocation_for_commands_not_declared_in_manifest() {
        use std::os::unix::fs::PermissionsExt;

        let root = unique_temp_dir("invoke-missing-command");
        let plugin_dir = root.join("invoke-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        let script_path = plugin_dir.join("plugin.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.invoke","name":"Invoke Plugin","version":"0.1.0","capabilities":["command_provider"]}}'
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
                "id": "example.invoke",
                "name": "Invoke Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh",
                "contributes": {
                    "commands": [
                        { "id": "example.invoke.run", "title": "Run" }
                    ]
                }
            }"#,
        )
        .expect("write manifest");

        let mut host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");
        host.start_plugin("example.invoke").expect("start plugin");
        let error = host
            .invoke_command("example.invoke", "example.invoke.other")
            .expect_err("command invocation should fail");

        assert!(error.contains("does not contribute command `example.invoke.other`"));

        drop(host);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_plugins_that_contribute_commands_without_command_provider_capability() {
        use std::os::unix::fs::PermissionsExt;

        let root = unique_temp_dir("invalid-command-provider");
        let plugin_dir = root.join("invalid-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        let script_path = plugin_dir.join("plugin.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
read line
printf '%s\n' '{"type":"hello","payload":{"protocol_version":1,"plugin_id":"example.invalid","name":"Invalid Plugin","version":"0.1.0","capabilities":[]}}'
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
                "id": "example.invalid",
                "name": "Invalid Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh",
                "contributes": {
                    "commands": [
                        { "id": "example.invalid.run", "title": "Run" }
                    ]
                }
            }"#,
        )
        .expect("write manifest");

        let host = PluginHost::load_from_dir(root.clone(), "0.1.0").expect("load host");

        assert!(host.running_plugins().is_empty());
        assert_eq!(host.failures().len(), 1);
        assert!(host.failures()[0]
            .message()
            .contains("does not advertise `command_provider`"));

        drop(host);
        let _ = fs::remove_dir_all(root);
    }
}
