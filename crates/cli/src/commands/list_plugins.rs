use termy_plugin_host::{default_plugins_dir, discover_plugins};

pub fn run() {
    let Some(root_dir) = default_plugins_dir() else {
        eprintln!(
            "Plugin directory is unavailable because the Termy config path could not be resolved."
        );
        return;
    };

    println!("Plugin directory: {}", root_dir.display());

    match discover_plugins(&root_dir) {
        Ok(plugins) => {
            if plugins.is_empty() {
                println!("No plugins discovered.");
                return;
            }

            for plugin in plugins {
                println!();
                println!("{} ({})", plugin.manifest.name, plugin.manifest.id);
                println!("  version: {}", plugin.manifest.version);
                println!("  entrypoint: {}", plugin.resolved_entrypoint().display());
                println!("  autostart: {}", plugin.manifest.autostart);
                if plugin.manifest.permissions.is_empty() {
                    println!("  permissions: none");
                } else {
                    let permissions = plugin
                        .manifest
                        .permissions
                        .iter()
                        .map(|permission| format!("{permission:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("  permissions: {permissions}");
                }
                if plugin.manifest.contributes.commands.is_empty() {
                    println!("  commands: none");
                } else {
                    println!("  commands:");
                    for command in &plugin.manifest.contributes.commands {
                        println!("    {} - {}", command.id, command.title);
                    }
                }
            }
        }
        Err(error) => {
            eprintln!("Failed to discover plugins: {error:#}");
        }
    }
}
