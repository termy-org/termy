use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use termy_plugin_core::{
    PluginCapability, PluginCommandContribution, PluginManifest, PluginPermission,
};
use termy_plugin_host::{default_plugins_dir, discover_plugins, PluginHost};

static PLUGIN_HOST: OnceLock<Mutex<PluginHost>> = OnceLock::new();

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
    pub(crate) recent_logs: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PluginCommandPaletteEntry {
    pub(crate) plugin_id: String,
    pub(crate) command_id: String,
    pub(crate) title: String,
    pub(crate) keywords: String,
    pub(crate) enabled: bool,
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

    let _ = PLUGIN_HOST.set(Mutex::new(host));
}

pub(crate) fn plugin_inventory() -> Result<PluginInventory, String> {
    let root_dir =
        default_plugins_dir().ok_or_else(|| "Plugin directory is unavailable".to_string())?;
    fs::create_dir_all(&root_dir).map_err(|error| {
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
            recent_logs: Vec::new(),
        })
        .collect::<Vec<_>>();

    if let Some(host) = host_lock() {
        let host = host
            .lock()
            .map_err(|_| "Plugin host lock poisoned".to_string())?;
        for running in host.running_plugins() {
            if let Some(entry) = entries.iter_mut().find(|entry| entry.id == running.id()) {
                entry.is_running = true;
                entry.capabilities = running.capabilities().to_vec();
                entry.recent_logs = host.recent_logs(running.id());
            }
        }

        for failure in host.failures() {
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.id == failure.plugin_id())
            {
                entry.load_error = Some(failure.message().to_string());
                entry.recent_logs = host.recent_logs(failure.plugin_id());
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
    let mut manifest: PluginManifest =
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

pub(crate) fn install_plugin_from_folder(source_dir: &Path) -> Result<String, String> {
    let root_dir =
        default_plugins_dir().ok_or_else(|| "Plugin directory is unavailable".to_string())?;
    fs::create_dir_all(&root_dir).map_err(|error| {
        format!(
            "Failed to create plugin directory {}: {error}",
            root_dir.display()
        )
    })?;

    let source_dir = source_dir.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve plugin source {}: {error}",
            source_dir.display()
        )
    })?;
    let manifest_path = source_dir.join(termy_plugin_core::PLUGIN_MANIFEST_FILE_NAME);
    let contents = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Failed to read {}: {error}", manifest_path.display()))?;
    let manifest: PluginManifest =
        PluginManifest::from_json_str(&contents).map_err(|error| error.to_string())?;
    let destination_dir = root_dir.join(&manifest.id);

    if destination_dir.exists() {
        return Err(format!(
            "A plugin with id `{}` is already installed at {}",
            manifest.id,
            destination_dir.display()
        ));
    }

    copy_directory_recursive(&source_dir, &destination_dir)?;
    Ok(format!(
        "Installed plugin `{}` into {}",
        manifest.id,
        destination_dir.display()
    ))
}

pub(crate) fn remove_plugin(plugin_id: &str) -> Result<String, String> {
    let inventory = plugin_inventory()?;
    let entry = inventory
        .entries
        .into_iter()
        .find(|entry| entry.id == plugin_id)
        .ok_or_else(|| format!("Plugin `{plugin_id}` not found"))?;

    let _ = stop_plugin(plugin_id);
    fs::remove_dir_all(&entry.root_dir)
        .map_err(|error| format!("Failed to remove {}: {error}", entry.root_dir.display()))?;

    Ok(format!("Removed plugin `{plugin_id}`"))
}

pub(crate) fn start_plugin(plugin_id: &str) -> Result<(), String> {
    let Some(host) = host_lock() else {
        return Err("Plugin host is unavailable".to_string());
    };
    let mut host = host
        .lock()
        .map_err(|_| "Plugin host lock poisoned".to_string())?;
    host.start_plugin(plugin_id)
        .map_err(|error| error.to_string())
}

pub(crate) fn stop_plugin(plugin_id: &str) -> Result<(), String> {
    let Some(host) = host_lock() else {
        return Err("Plugin host is unavailable".to_string());
    };
    let mut host = host
        .lock()
        .map_err(|_| "Plugin host lock poisoned".to_string())?;
    host.stop_plugin(plugin_id)
}

pub(crate) fn invoke_plugin_command(plugin_id: &str, command_id: &str) -> Result<(), String> {
    let Some(host) = host_lock() else {
        return Err("Plugin host is unavailable".to_string());
    };
    let mut host = host
        .lock()
        .map_err(|_| "Plugin host lock poisoned".to_string())?;
    host.invoke_command(plugin_id, command_id)
}

pub(crate) fn command_palette_entries() -> Result<Vec<PluginCommandPaletteEntry>, String> {
    let inventory = plugin_inventory()?;
    let mut entries = Vec::new();

    for plugin in inventory.entries {
        for command in plugin.commands {
            let description = command.description.clone().unwrap_or_default();
            let keywords = format!(
                "{} {} {}",
                plugin.name,
                command.id.replace('.', " "),
                description
            )
            .trim()
            .to_string();
            let enabled = plugin.is_running
                && plugin
                    .capabilities
                    .contains(&PluginCapability::CommandProvider);
            entries.push(PluginCommandPaletteEntry {
                plugin_id: plugin.id.clone(),
                command_id: command.id,
                title: format!("{}: {}", plugin.name, command.title),
                keywords,
                enabled,
            });
        }
    }

    Ok(entries)
}

fn host_lock() -> Option<&'static Mutex<PluginHost>> {
    PLUGIN_HOST.get()
}

fn copy_directory_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::metadata(source)
        .map_err(|error| format!("Failed to read {}: {error}", source.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "Plugin source {} is not a directory",
            source.display()
        ));
    }

    fs::create_dir_all(destination)
        .map_err(|error| format!("Failed to create {}: {error}", destination.display()))?;

    for entry in fs::read_dir(source)
        .map_err(|error| format!("Failed to read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            copy_directory_recursive(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "Failed to copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn running_plugin_count() -> usize {
    PLUGIN_HOST
        .get()
        .and_then(|host| host.lock().ok().map(|host| host.running_plugins().len()))
        .unwrap_or(0)
}
