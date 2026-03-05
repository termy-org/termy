use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PLUGIN_MANIFEST_FILE_NAME: &str = "termy-plugin.json";
pub const PLUGIN_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub minimum_host_version: Option<String>,
    #[serde(default)]
    pub api_version: Option<u32>,
    #[serde(default)]
    pub runtime: PluginRuntime,
    pub entrypoint: String,
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    #[serde(default)]
    pub contributes: PluginContributions,
}

fn default_autostart() -> bool {
    true
}

impl PluginManifest {
    pub fn from_json_str(contents: &str) -> Result<Self, PluginManifestError> {
        let manifest: Self = serde_json::from_str(contents)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), PluginManifestError> {
        if self.schema_version != 1 {
            return Err(PluginManifestError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.id.trim().is_empty() {
            return Err(PluginManifestError::MissingField("id"));
        }
        if self.name.trim().is_empty() {
            return Err(PluginManifestError::MissingField("name"));
        }
        if self.version.trim().is_empty() {
            return Err(PluginManifestError::MissingField("version"));
        }
        if self.entrypoint.trim().is_empty() {
            return Err(PluginManifestError::MissingField("entrypoint"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntime {
    #[default]
    Executable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermission {
    FilesystemRead,
    FilesystemWrite,
    Network,
    Shell,
    Clipboard,
    Notifications,
    TerminalRead,
    TerminalWrite,
    UiPanels,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginContributions {
    #[serde(default)]
    pub commands: Vec<PluginCommandContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum HostRpcMessage {
    Hello(HostHello),
    Shutdown,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostHello {
    pub protocol_version: u32,
    pub host_name: String,
    pub host_version: String,
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum PluginRpcMessage {
    Hello(PluginHello),
    Log(PluginLogMessage),
    Toast(PluginToastMessage),
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginHello {
    pub protocol_version: u32,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    CommandProvider,
    EventSubscriber,
    UiPanel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginLogMessage {
    pub level: PluginLogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginToastMessage {
    pub level: PluginToastLevel,
    pub message: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPlugin {
    pub root_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: PluginManifest,
}

impl DiscoveredPlugin {
    pub fn resolved_entrypoint(&self) -> PathBuf {
        let entrypoint = Path::new(&self.manifest.entrypoint);
        if entrypoint.is_absolute() {
            entrypoint.to_path_buf()
        } else {
            self.root_dir.join(entrypoint)
        }
    }
}

#[derive(Debug, Error)]
pub enum PluginManifestError {
    #[error("failed to parse plugin manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("plugin manifest field `{0}` is required")]
    MissingField(&'static str),
    #[error("plugin manifest schema_version {0} is unsupported")]
    UnsupportedSchemaVersion(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_manifest() {
        let manifest = PluginManifest::from_json_str(
            r#"{
                "schema_version": 1,
                "id": "example.hello",
                "name": "Hello Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh",
                "permissions": ["network"],
                "contributes": {
                    "commands": [
                        { "id": "hello.run", "title": "Run Hello" }
                    ]
                }
            }"#,
        )
        .expect("manifest should parse");

        assert_eq!(manifest.runtime, PluginRuntime::Executable);
        assert!(manifest.autostart);
        assert_eq!(manifest.permissions, vec![PluginPermission::Network]);
        assert_eq!(manifest.contributes.commands.len(), 1);
    }

    #[test]
    fn rejects_invalid_schema_version() {
        let error = PluginManifest::from_json_str(
            r#"{
                "schema_version": 2,
                "id": "example.hello",
                "name": "Hello Plugin",
                "version": "0.1.0",
                "entrypoint": "./plugin.sh"
            }"#,
        )
        .expect_err("schema version should fail");

        assert!(matches!(
            error,
            PluginManifestError::UnsupportedSchemaVersion(2)
        ));
    }

    #[test]
    fn resolves_relative_entrypoint_against_plugin_root() {
        let manifest = PluginManifest::from_json_str(
            r#"{
                "schema_version": 1,
                "id": "example.hello",
                "name": "Hello Plugin",
                "version": "0.1.0",
                "entrypoint": "bin/plugin"
            }"#,
        )
        .expect("manifest should parse");
        let discovered = DiscoveredPlugin {
            root_dir: PathBuf::from("/tmp/plugins/hello"),
            manifest_path: PathBuf::from("/tmp/plugins/hello/termy-plugin.json"),
            manifest,
        };

        assert_eq!(
            discovered.resolved_entrypoint(),
            PathBuf::from("/tmp/plugins/hello/bin/plugin")
        );
    }
}
