use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use termy_plugin_core::{
    PluginCapability, PluginCommandContribution, PluginManifest, PluginPermission,
};
use termy_plugin_host::{PluginHost, default_plugins_dir, discover_plugins};

static PLUGIN_HOST: OnceLock<PluginHost> = OnceLock::new();

#[derive(Clone, Debug)]
pub(crate) struct PluginInventory {
    pub(crate) root_dir: PathBuf,
    pub(crate) entries: Vec<PluginInventoryEntry>,
}

#[derive(Clone, Debug)]
pub(crate) struct PluginInventoryEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) description: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) root_dir: PathBuf,
    pub(crate) manifest_path: PathBuf,
    pub(crate) entrypoint: PathBuf,
    pub(crate) autostart: bool,
    pub(crate) permissions: Vec<PluginPermission>,
    pub(crate) commands: Vec<PluginCommandContribution>,
    pub(crate) capabilities: Vec<PluginCapability>,
    pub(crate) is_running: bool,
    pub(crate) load_error: Option<String>,
}

pub(crate) fn initialize_plugins(host_version: &str) {
    if PLUGIN_HOST.get().is_some() {
        return;
    }

    let host = match PluginHost::load_default(host_version) {
        Ok(host) => host,
        Err(error) => {
            log::error!("Failed to initialize plugin host: {error:#}");
            termy_toast::enqueue_toast(
                termy_toast::ToastKind::Warning,
                format!("Plugin host failed to initialize: {error}"),
                None,
            );
            return;
        }
    };

    let plugin_root = host.root_dir().display().to_string();
    for plugin in host.running_plugins() {
        log::info!(
            "Started plugin {} v{} from {}",
            plugin.id(),
            plugin.version(),
            plugin.root_dir().display()
        );
    }

    if !host.failures().is_empty() {
        for failure in host.failures() {
            log::warn!(
                "Plugin {} failed to load: {}",
                failure.plugin_id(),
                failure.message()
            );
        }
        termy_toast::enqueue_toast(
            termy_toast::ToastKind::Warning,
            format!(
                "{} plugin(s) failed to load. Check logs. Plugin directory: {}",
                host.failures().len(),
                plugin_root
            ),
            None,
        );
    }

    if !host.running_plugins().is_empty() {
        log::info!(
            "Plugin host ready with {} running plugin(s) from {}",
            host.running_plugins().len(),
            plugin_root
        );
    }

    let _ = PLUGIN_HOST.set(host);
}

pub(crate) fn plugin_inventory() -> Result<PluginInventory, String> {
    let root_dir =
        default_plugins_dir().ok_or_else(|| "Plugin directory is unavailable".to_string())?;
    std::fs::create_dir_all(&root_dir).map_err(|error| {
        format!(
            "Failed to create plugin directory {}: {error}",
            root_dir.display()
        )
    })?;
    let discovered = discover_plugins(&root_dir).map_err(|error| error.to_string())?;
    let mut entries = discovered
        .into_iter()
        .map(|plugin| PluginInventoryEntry {
            id: plugin.manifest.id.clone(),
            name: plugin.manifest.name.clone(),
            version: plugin.manifest.version.clone(),
            description: plugin.manifest.description.clone(),
            author: plugin.manifest.author.clone(),
            root_dir: plugin.root_dir.clone(),
            manifest_path: plugin.manifest_path.clone(),
            entrypoint: plugin.resolved_entrypoint(),
            autostart: plugin.manifest.autostart,
            permissions: plugin.manifest.permissions.clone(),
            commands: plugin.manifest.contributes.commands.clone(),
            capabilities: Vec::new(),
            is_running: false,
            load_error: None,
        })
        .collect::<Vec<_>>();

    if let Some(host) = PLUGIN_HOST.get() {
        for running in host.running_plugins() {
            if let Some(entry) = entries.iter_mut().find(|entry| entry.id == running.id()) {
                entry.is_running = true;
                entry.capabilities = running.capabilities().to_vec();
            }
        }

        for failure in host.failures() {
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.id == failure.plugin_id())
            {
                entry.load_error = Some(failure.message().to_string());
            } else {
                entries.push(PluginInventoryEntry {
                    id: failure.plugin_id().to_string(),
                    name: failure.plugin_id().to_string(),
                    version: "unknown".to_string(),
                    description: None,
                    author: None,
                    root_dir: root_dir.join(failure.plugin_id()),
                    manifest_path: root_dir.join(failure.plugin_id()).join("termy-plugin.json"),
                    entrypoint: root_dir.join(failure.plugin_id()),
                    autostart: false,
                    permissions: Vec::new(),
                    commands: Vec::new(),
                    capabilities: Vec::new(),
                    is_running: false,
                    load_error: Some(failure.message().to_string()),
                });
            }
        }
    }

    entries.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    Ok(PluginInventory { root_dir, entries })
}

pub(crate) fn set_plugin_autostart(plugin_id: &str, autostart: bool) -> Result<(), String> {
    let inventory = plugin_inventory()?;
    let entry = inventory
        .entries
        .into_iter()
        .find(|entry| entry.id == plugin_id)
        .ok_or_else(|| format!("Plugin `{plugin_id}` not found"))?;

    let contents = fs::read_to_string(&entry.manifest_path)
        .map_err(|error| format!("Failed to read {}: {error}", entry.manifest_path.display()))?;
    let mut manifest =
        PluginManifest::from_json_str(&contents).map_err(|error| error.to_string())?;
    manifest.autostart = autostart;
    let updated = serde_json::to_string_pretty(&manifest).map_err(|error| {
        format!(
            "Failed to serialize manifest {}: {error}",
            entry.manifest_path.display()
        )
    })?;
    fs::write(&entry.manifest_path, format!("{updated}\n"))
        .map_err(|error| format!("Failed to write {}: {error}", entry.manifest_path.display()))?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn running_plugin_count() -> usize {
    PLUGIN_HOST
        .get()
        .map(|host| host.running_plugins().len())
        .unwrap_or(0)
}
