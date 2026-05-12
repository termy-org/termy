use std::collections::HashMap;
use std::path::{Path, PathBuf};

use termy_config_core::RootSettingId;
use termy_themes::{Rgb8, ThemeColors};

use super::{
    DetectedSource, ImportSourceKind, ImportedConfig, clamp_opacity, extract_app_icon,
    first_existing, home_dir, macos_app_bundle_path, map_cursor_shape, parse_hex_color,
};

const MAX_INCLUDE_DEPTH: usize = 4;

pub(crate) fn detect() -> DetectedSource {
    let app_path = macos_app_bundle_path("Ghostty.app");
    let app_installed = app_path.is_some();
    let icon_path = app_path
        .as_ref()
        .and_then(|path| extract_app_icon("ghostty", path));
    let config_path = config_candidates()
        .into_iter()
        .find(|path| path.exists());
    let status_hint = if config_path.is_none() && app_installed {
        Some("App installed but no config file yet".into())
    } else {
        None
    };
    DetectedSource {
        kind: ImportSourceKind::Ghostty,
        app_installed,
        config_path,
        status_hint,
        icon_path,
    }
}

fn config_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        paths.push(PathBuf::from(xdg).join("ghostty").join("config"));
    }
    if let Some(home) = home_dir() {
        paths.push(home.join(".config/ghostty/config"));
        paths.push(home.join("Library/Application Support/com.mitchellh.ghostty/config"));
    }
    paths
}

pub(crate) fn import(path: &Path) -> Result<ImportedConfig, String> {
    let mut imported = ImportedConfig::new(ImportSourceKind::Ghostty);
    let mut palette: HashMap<u8, Rgb8> = HashMap::new();
    let mut named: HashMap<String, Rgb8> = HashMap::new();
    let mut theme_ref: Option<String> = None;

    parse_file(path, 0, &mut imported, &mut palette, &mut named, &mut theme_ref)?;

    let mut have_inline = !palette.is_empty() || !named.is_empty();
    if let Some(name) = theme_ref.as_ref()
        && !have_inline
        && let Some(theme_path) = resolve_theme_file(name)
    {
        parse_file(
            &theme_path,
            0,
            &mut imported,
            &mut palette,
            &mut named,
            &mut theme_ref,
        )?;
        have_inline = !palette.is_empty() || !named.is_empty();
    }

    if have_inline {
        imported.theme = build_theme(&palette, &named, &mut imported.warnings);
    }

    Ok(imported)
}

fn parse_file(
    path: &Path,
    depth: usize,
    imported: &mut ImportedConfig,
    palette: &mut HashMap<u8, Rgb8>,
    named: &mut HashMap<String, Rgb8>,
    theme_ref: &mut Option<String>,
) -> Result<(), String> {
    if depth > MAX_INCLUDE_DEPTH {
        imported.warnings.push(format!(
            "Skipped include past depth limit: {}",
            path.display()
        ));
        return Ok(());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "config-file" => {
                let included = expand_user(value, path);
                parse_file(&included, depth + 1, imported, palette, named, theme_ref)?;
            }
            "theme" => {
                let primary = value.split(',').next().unwrap_or(value).trim();
                let name = primary
                    .strip_prefix("dark:")
                    .or_else(|| primary.strip_prefix("light:"))
                    .unwrap_or(primary)
                    .trim();
                if !name.is_empty() {
                    *theme_ref = Some(name.to_string());
                }
            }
            "palette" => {
                if let Some((index_str, hex)) = value.split_once('=')
                    && let Ok(index) = index_str.trim().parse::<u8>()
                    && index < 16
                    && let Some(color) = parse_hex_color(hex)
                {
                    palette.insert(index, color);
                }
            }
            "foreground" | "background" | "cursor-color" => {
                let bucket = if key == "cursor-color" { "cursor" } else { key };
                if let Some(color) = parse_hex_color(value) {
                    named.insert(bucket.to_string(), color);
                }
            }
            "font-family" => {
                imported
                    .settings
                    .push((RootSettingId::FontFamily, value.to_string()));
            }
            "font-size" => {
                if value.parse::<f32>().is_ok() {
                    imported
                        .settings
                        .push((RootSettingId::FontSize, value.to_string()));
                }
            }
            "window-padding-x" => {
                if value.parse::<f32>().is_ok() {
                    imported
                        .settings
                        .push((RootSettingId::PaddingX, value.to_string()));
                }
            }
            "window-padding-y" => {
                if value.parse::<f32>().is_ok() {
                    imported
                        .settings
                        .push((RootSettingId::PaddingY, value.to_string()));
                }
            }
            "background-opacity" => {
                if let Ok(opacity) = value.parse::<f32>() {
                    imported.settings.push((
                        RootSettingId::BackgroundOpacity,
                        format!("{:.3}", clamp_opacity(opacity)),
                    ));
                }
            }
            "background-blur-radius" => {
                if let Ok(radius) = value.parse::<f32>() {
                    imported.settings.push((
                        RootSettingId::BackgroundBlur,
                        (radius > 0.0).to_string(),
                    ));
                }
            }
            "cursor-style" => {
                if let Some(mapped) = map_cursor_shape(value, &mut imported.warnings) {
                    imported
                        .settings
                        .push((RootSettingId::CursorStyle, mapped.to_string()));
                }
            }
            "cursor-style-blink" => {
                if let Some(b) = parse_bool(value) {
                    imported
                        .settings
                        .push((RootSettingId::CursorBlink, b.to_string()));
                }
            }
            "scrollback-limit" => {
                if let Ok(bytes) = value.parse::<u64>() {
                    let lines = (bytes / 100).clamp(0, 1_000_000);
                    if bytes > 0 {
                        imported.warnings.push(
                            "Ghostty scrollback-limit is in bytes; approximated to lines".into(),
                        );
                    }
                    imported
                        .settings
                        .push((RootSettingId::ScrollbackHistory, lines.to_string()));
                }
            }
            "command" => {
                if !value.is_empty() {
                    imported
                        .settings
                        .push((RootSettingId::Shell, value.to_string()));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn resolve_theme_file(name: &str) -> Option<PathBuf> {
    let home = home_dir()?;
    let candidates = vec![
        home.join(".config/ghostty/themes").join(name),
        home.join("Library/Application Support/com.mitchellh.ghostty/themes")
            .join(name),
    ];
    first_existing(&candidates)
}

fn expand_user(value: &str, relative_to: &Path) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if let Some(parent) = relative_to.parent() {
        parent.join(path)
    } else {
        path
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => Some(true),
        "false" | "no" | "0" | "off" => Some(false),
        _ => None,
    }
}

fn build_theme(
    palette: &HashMap<u8, Rgb8>,
    named: &HashMap<String, Rgb8>,
    warnings: &mut Vec<String>,
) -> Option<ThemeColors> {
    let mut ansi = [Rgb8::new(0, 0, 0); 16];
    let mut missing = Vec::new();
    for index in 0..16u8 {
        if let Some(color) = palette.get(&index) {
            ansi[index as usize] = *color;
        } else {
            missing.push(index);
        }
    }
    if !missing.is_empty() {
        warnings.push(format!(
            "Ghostty palette missing indices {:?}; filled with black",
            missing
        ));
    }

    let foreground = *named.get("foreground")?;
    let background = *named.get("background")?;
    let cursor = named.get("cursor").copied().unwrap_or(foreground);
    Some(ThemeColors {
        ansi,
        foreground,
        background,
        cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_file(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", contents).unwrap();
        file
    }

    #[test]
    fn parses_palette_and_settings() {
        let palette = (0..16u8)
            .map(|i| format!("palette = {i}=#{:02x}{:02x}{:02x}", i, i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let config = format!(
            "# header\nfont-family = JetBrains Mono\nfont-size = 15\nbackground = #1a1b26\nforeground = #c0caf5\ncursor-color = #f7768e\nbackground-opacity = 0.85\ncursor-style = bar\ncursor-style-blink = false\n{palette}\n"
        );
        let file = write_file(&config);
        let imported = import(file.path()).unwrap();
        let theme = imported.theme.expect("expected theme");
        assert_eq!(theme.foreground, Rgb8::new(0xc0, 0xca, 0xf5));
        assert_eq!(theme.background, Rgb8::new(0x1a, 0x1b, 0x26));
        assert_eq!(theme.cursor, Rgb8::new(0xf7, 0x76, 0x8e));
        assert_eq!(theme.ansi[5], Rgb8::new(5, 5, 5));
        let settings: Vec<_> = imported
            .settings
            .iter()
            .map(|(id, value)| (*id, value.clone()))
            .collect();
        assert!(settings.contains(&(RootSettingId::FontFamily, "JetBrains Mono".to_string())));
        assert!(settings.contains(&(RootSettingId::FontSize, "15".to_string())));
        assert!(settings.contains(&(RootSettingId::BackgroundOpacity, "0.850".to_string())));
        assert!(settings.contains(&(RootSettingId::CursorStyle, "line".to_string())));
        assert!(settings.contains(&(RootSettingId::CursorBlink, "false".to_string())));
    }

    #[test]
    fn missing_palette_indices_become_warnings() {
        let config =
            "foreground = #ffffff\nbackground = #000000\ncursor-color = #ff0000\npalette = 0=#000000\n";
        let file = write_file(config);
        let imported = import(file.path()).unwrap();
        assert!(imported.theme.is_some());
        assert!(
            imported
                .warnings
                .iter()
                .any(|warning| warning.contains("palette missing"))
        );
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let config = "definitely-not-a-real-key = foo\nfont-size = 12\n";
        let file = write_file(config);
        let imported = import(file.path()).unwrap();
        assert_eq!(imported.settings.len(), 1);
        assert!(imported.theme.is_none());
    }
}
