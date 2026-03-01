use std::path::PathBuf;

use termy_command_core::{
    CommandCapabilities, CommandId, KeybindLineRef, default_resolved_keybinds,
    parse_keybind_directives_from_iter, resolve_keybinds,
};
use termy_config_core::{AppConfig, config_path};
use termy_theme_core::{ANSI_COLOR_NAMES, format_hex};

pub fn config_file_path() -> Result<PathBuf, String> {
    config_path().ok_or_else(|| "Could not determine config directory".to_string())
}

pub fn list_action_lines() -> Vec<String> {
    action_lines_for_tmux_enabled(load_config_for_providers().tmux_enabled)
}

pub fn list_keybind_lines() -> Vec<String> {
    let config = load_config_for_providers();
    keybind_lines_for_tmux_enabled(&config.keybind_lines, config.tmux_enabled)
}

pub fn list_theme_lines() -> Vec<String> {
    termy_themes::available_theme_ids()
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

pub fn list_color_lines() -> Vec<String> {
    let theme_id = active_theme_id();
    let mut lines = vec![format!("Theme: {}", theme_id), String::new()];

    let theme_colors = termy_themes::resolve_theme(&theme_id);
    let colors = match theme_colors {
        Some(colors) => colors,
        None => {
            lines.push(format!("Unknown theme: {}", theme_id));
            lines.push("Using built-in fallback: termy".to_string());
            lines.push(String::new());
            termy_themes::termy()
        }
    };

    lines.push(format!("foreground = {}", format_hex(colors.foreground)));
    lines.push(format!("background = {}", format_hex(colors.background)));
    lines.push(format!("cursor = {}", format_hex(colors.cursor)));

    for (index, name) in ANSI_COLOR_NAMES.iter().enumerate() {
        lines.push(format!("{} = {}", name, format_hex(colors.ansi[index])));
    }

    lines
}

pub fn show_config_lines() -> Result<Vec<String>, String> {
    let path = config_file_path()?;
    let mut lines = vec![format!("Config file: {}", path.display()), String::new()];

    if !path.exists() {
        lines.push("(not created yet - using defaults)".to_string());
        lines.push(String::new());
        lines.extend(
            termy_config_core::DEFAULT_CONFIG_TEMPLATE
                .lines()
                .map(ToString::to_string),
        );
        return Ok(lines);
    }

    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read config file: {}", error))?;
    if contents.trim().is_empty() {
        lines.push("(empty file - using defaults)".to_string());
        lines.push(String::new());
        lines.extend(
            termy_config_core::DEFAULT_CONFIG_TEMPLATE
                .lines()
                .map(ToString::to_string),
        );
        return Ok(lines);
    }

    lines.extend(contents.lines().map(ToString::to_string));
    Ok(lines)
}

pub fn list_fonts_lines() -> Vec<String> {
    list_fonts_impl()
}

pub fn active_theme_id() -> String {
    load_config_for_providers().theme
}

fn load_config_for_providers() -> AppConfig {
    if let Some(path) = config_path()
        && let Ok(contents) = std::fs::read_to_string(path)
    {
        return AppConfig::from_contents(&contents);
    }

    AppConfig::default()
}

fn action_lines_for_tmux_enabled(tmux_enabled: bool) -> Vec<String> {
    let capabilities = command_capabilities(tmux_enabled);

    CommandId::all()
        .map(|id| {
            let (available, tmux_required, restart_required) =
                command_metadata_for_id(id, capabilities);
            format!(
                "{}\tavailable={}\ttmux_required={}\trestart_required={}",
                id.config_name(),
                available,
                tmux_required,
                restart_required
            )
        })
        .collect()
}

fn keybind_lines_for_tmux_enabled(
    lines: &[termy_config_core::KeybindConfigLine],
    tmux_enabled: bool,
) -> Vec<String> {
    let capabilities = command_capabilities(tmux_enabled);

    resolve_keybinds_for_lines(lines)
        .into_iter()
        .map(|binding| {
            let (available, tmux_required, restart_required) =
                command_metadata_for_id(binding.action, capabilities);
            format!(
                "{} = {}\tavailable={}\ttmux_required={}\trestart_required={}",
                binding.trigger,
                binding.action.config_name(),
                available,
                tmux_required,
                restart_required
            )
        })
        .collect()
}

fn resolve_keybinds_for_lines(
    lines: &[termy_config_core::KeybindConfigLine],
) -> Vec<termy_command_core::ResolvedKeybind> {
    let (directives, _warnings) =
        parse_keybind_directives_from_iter(lines.iter().map(|line| KeybindLineRef {
            line_number: line.line_number,
            value: line.value.as_str(),
        }));
    resolve_keybinds(default_resolved_keybinds(), &directives)
}

fn command_capabilities(tmux_enabled: bool) -> CommandCapabilities {
    CommandCapabilities {
        tmux_runtime_active: tmux_enabled,
        install_cli_available: true,
    }
}

fn command_metadata_for_id(
    id: CommandId,
    capabilities: CommandCapabilities,
) -> (bool, bool, bool) {
    let availability = id.availability(capabilities);
    let tmux_required = id.is_tmux_only();
    (availability.enabled, tmux_required, tmux_required)
}

#[cfg(target_os = "macos")]
fn list_fonts_impl() -> Vec<String> {
    use core_text::font_collection::create_for_all_families;

    let collection = create_for_all_families();
    let descriptors = collection.get_descriptors();

    let mut fonts: Vec<String> = Vec::new();

    if let Some(descriptors) = descriptors {
        for index in 0..descriptors.len() {
            if let Some(descriptor) = descriptors.get(index) {
                let family_name = descriptor.family_name();
                if !fonts.contains(&family_name) {
                    fonts.push(family_name);
                }
            }
        }
    }

    fonts.sort();
    fonts
}

#[cfg(target_os = "linux")]
fn list_fonts_impl() -> Vec<String> {
    use std::process::Command;

    let output = Command::new("fc-list")
        .args([":spacing=mono", "-f", "%{family}\\n"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut fonts: Vec<String> = stdout.lines().map(ToString::to_string).collect();
            fonts.sort();
            fonts.dedup();
            fonts.into_iter().filter(|font| !font.is_empty()).collect()
        }
        _ => common_monospace_fonts(),
    }
}

#[cfg(target_os = "linux")]
fn common_monospace_fonts() -> Vec<String> {
    vec![
        "DejaVu Sans Mono".to_string(),
        "Liberation Mono".to_string(),
        "Fira Code".to_string(),
        "JetBrains Mono".to_string(),
        "Source Code Pro".to_string(),
        "Hack".to_string(),
        "Inconsolata".to_string(),
        "Ubuntu Mono".to_string(),
        "Droid Sans Mono".to_string(),
        "Roboto Mono".to_string(),
        "Cascadia Code".to_string(),
        "IBM Plex Mono".to_string(),
    ]
}

#[cfg(target_os = "windows")]
fn list_fonts_impl() -> Vec<String> {
    vec![
        "Consolas".to_string(),
        "Courier New".to_string(),
        "Lucida Console".to_string(),
        "Cascadia Code".to_string(),
        "Cascadia Mono".to_string(),
        "JetBrains Mono".to_string(),
        "Fira Code".to_string(),
        "Source Code Pro".to_string(),
        String::new(),
        "Note: This is a partial list of common monospace fonts.".to_string(),
    ]
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn list_fonts_impl() -> Vec<String> {
    vec!["Font listing is not supported on this platform".to_string()]
}

#[cfg(test)]
mod tests {
    use super::{
        action_lines_for_tmux_enabled, keybind_lines_for_tmux_enabled, list_theme_lines,
        resolve_keybinds_for_lines,
    };
    use termy_command_core::{
        CommandId, KeybindLineRef, default_resolved_keybinds, parse_keybind_directives_from_iter,
        resolve_keybinds,
    };
    use termy_config_core::KeybindConfigLine;

    fn fixture_keybind_lines() -> Vec<KeybindConfigLine> {
        vec![
            KeybindConfigLine {
                line_number: 1,
                value: "Secondary-P=toggle_command_palette".to_string(),
            },
            KeybindConfigLine {
                line_number: 2,
                value: "Control-Shift-C=copy".to_string(),
            },
            KeybindConfigLine {
                line_number: 3,
                value: "cmd-=zoom_in".to_string(),
            },
            KeybindConfigLine {
                line_number: 4,
                value: "secondary-p=unbind".to_string(),
            },
        ]
    }

    #[test]
    fn list_actions_includes_tmux_metadata_when_runtime_is_disabled() {
        let actions = action_lines_for_tmux_enabled(false);
        let split_pane_line = actions
            .iter()
            .find(|line| line.starts_with(CommandId::SplitPaneVertical.config_name()))
            .expect("missing split_pane_vertical action metadata");
        assert!(split_pane_line.contains("available=false"));
        assert!(split_pane_line.contains("tmux_required=true"));
        assert!(split_pane_line.contains("restart_required=true"));
    }

    #[test]
    fn keybinds_include_secondary_comma_mapping() {
        let keybinds = resolve_keybinds_for_lines(&[]);
        assert!(
            keybinds.iter().any(|binding| {
                binding.trigger == "secondary-," && binding.action.config_name() == "open_settings"
            }),
            "expected secondary-, default keybind to map to open_settings"
        );
    }

    #[test]
    fn keybind_resolution_matches_command_core_for_same_fixture() {
        let lines = fixture_keybind_lines();
        let resolved_from_provider = resolve_keybinds_for_lines(&lines);

        let (directives, warnings) =
            parse_keybind_directives_from_iter(lines.iter().map(|line| KeybindLineRef {
                line_number: line.line_number,
                value: line.value.as_str(),
            }));
        assert!(warnings.is_empty());

        let resolved_from_core = resolve_keybinds(default_resolved_keybinds(), &directives);
        assert_eq!(resolved_from_provider, resolved_from_core);
    }

    #[test]
    fn list_keybinds_includes_tmux_metadata_when_runtime_is_disabled() {
        let keybind_lines = keybind_lines_for_tmux_enabled(&[], false);
        let split_pane_line = keybind_lines
            .iter()
            .find(|line| line.starts_with("secondary-d = split_pane_vertical"))
            .expect("missing secondary-d split pane keybind metadata");
        assert!(split_pane_line.contains("available=false"));
        assert!(split_pane_line.contains("tmux_required=true"));
        assert!(split_pane_line.contains("restart_required=true"));
    }

    #[test]
    fn themes_are_sourced_from_theme_registry() {
        let themes = list_theme_lines();
        assert!(themes.iter().any(|theme| theme == "termy"));
    }
}
