use std::collections::HashMap;
use std::path::{Path, PathBuf};

use termy_config_core::RootSettingId;
use termy_themes::{Rgb8, ThemeColors};

use super::{
    DetectedSource, ImportSourceKind, ImportedConfig, clamp_opacity, extract_app_icon, home_dir,
    macos_app_bundle_path, map_cursor_shape, parse_hex_color,
};

const MAX_INCLUDE_DEPTH: usize = 4;

pub(crate) fn detect() -> DetectedSource {
    let app_path = macos_app_bundle_path("kitty.app");
    let app_installed = app_path.is_some();
    let icon_path = app_path
        .as_ref()
        .and_then(|path| extract_app_icon("kitty", path));
    let config_path = candidates().into_iter().find(|path| path.exists());
    DetectedSource {
        kind: ImportSourceKind::Kitty,
        app_installed,
        config_path,
        status_hint: None,
        icon_path,
    }
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home_dir() {
        paths.push(home.join("Library/Preferences/kitty/kitty.conf"));
        paths.push(home.join(".config/kitty/kitty.conf"));
    }
    paths
}

pub(crate) fn import(path: &Path) -> Result<ImportedConfig, String> {
    let mut imported = ImportedConfig::new(ImportSourceKind::Kitty);
    let mut palette: HashMap<u8, Rgb8> = HashMap::new();
    let mut named: HashMap<&'static str, Rgb8> = HashMap::new();

    parse_file(path, 0, &mut imported, &mut palette, &mut named)?;

    if !palette.is_empty() || !named.is_empty() {
        imported.theme = build_theme(&palette, &named, &mut imported.warnings);
    }
    Ok(imported)
}

fn parse_file(
    path: &Path,
    depth: usize,
    imported: &mut ImportedConfig,
    palette: &mut HashMap<u8, Rgb8>,
    named: &mut HashMap<&'static str, Rgb8>,
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
        let (key, value) = match split_kv(trimmed) {
            Some(parts) => parts,
            None => continue,
        };

        match key {
            "include" => {
                let included = resolve_path(value, path);
                parse_file(&included, depth + 1, imported, palette, named)?;
            }
            "globinclude" => {
                let base = resolve_path(value, path);
                let parent = base.parent().unwrap_or_else(|| Path::new(""));
                let pattern = base
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Ok(entries) = std::fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        if !entry_path.is_file() {
                            continue;
                        }
                        let entry_name = entry_path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if simple_glob_match(&pattern, &entry_name) {
                            parse_file(&entry_path, depth + 1, imported, palette, named)?;
                        }
                    }
                }
            }
            "geninclude" => {
                imported
                    .warnings
                    .push(format!("Ignored geninclude (runs script): {}", value));
            }
            "envinclude" => {
                imported
                    .warnings
                    .push(format!("Ignored envinclude: {}", value));
            }
            "font_family" => {
                imported
                    .settings
                    .push((RootSettingId::FontFamily, value.to_string()));
            }
            "font_size" => {
                if value.parse::<f32>().is_ok() {
                    imported
                        .settings
                        .push((RootSettingId::FontSize, value.to_string()));
                }
            }
            "window_padding_width" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if let Some(first) = parts.first()
                    && first.parse::<f32>().is_ok()
                {
                    imported
                        .settings
                        .push((RootSettingId::PaddingX, (*first).to_string()));
                    let y = parts.get(1).copied().unwrap_or(first);
                    if y.parse::<f32>().is_ok() {
                        imported
                            .settings
                            .push((RootSettingId::PaddingY, y.to_string()));
                    }
                }
            }
            "background_opacity" => {
                if let Ok(opacity) = value.parse::<f32>() {
                    imported.settings.push((
                        RootSettingId::BackgroundOpacity,
                        format!("{:.3}", clamp_opacity(opacity)),
                    ));
                }
            }
            "background_blur" => {
                if let Ok(blur) = value.parse::<i32>() {
                    imported
                        .settings
                        .push((RootSettingId::BackgroundBlur, (blur > 0).to_string()));
                }
            }
            "cursor_shape" => {
                if let Some(mapped) = map_cursor_shape(value, &mut imported.warnings) {
                    imported
                        .settings
                        .push((RootSettingId::CursorStyle, mapped.to_string()));
                }
            }
            "cursor_blink_interval" => {
                if let Ok(interval) = value.parse::<f32>() {
                    imported
                        .settings
                        .push((RootSettingId::CursorBlink, (interval > 0.0).to_string()));
                }
            }
            "scrollback_lines" => {
                if let Ok(lines) = value.parse::<i64>() {
                    let clamped = lines.max(0);
                    imported
                        .settings
                        .push((RootSettingId::ScrollbackHistory, clamped.to_string()));
                }
            }
            "shell" => {
                if !value.is_empty() && !value.eq_ignore_ascii_case(".") {
                    imported
                        .settings
                        .push((RootSettingId::Shell, value.to_string()));
                }
            }
            "foreground" => {
                if let Some(color) = parse_hex_color(value) {
                    named.insert("foreground", color);
                }
            }
            "background" => {
                if let Some(color) = parse_hex_color(value) {
                    named.insert("background", color);
                }
            }
            "cursor" => {
                if let Some(color) = parse_hex_color(value) {
                    named.insert("cursor", color);
                }
            }
            other if other.starts_with("color") => {
                if let Ok(index) = other.trim_start_matches("color").parse::<u8>()
                    && index < 16
                    && let Some(color) = parse_hex_color(value)
                {
                    palette.insert(index, color);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut split = line.splitn(2, char::is_whitespace);
    let key = split.next()?;
    let value = split.next().unwrap_or("").trim();
    Some((key, value))
}

fn resolve_path(value: &str, relative_to: &Path) -> PathBuf {
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

fn simple_glob_match(pattern: &str, name: &str) -> bool {
    if let Some(rest) = pattern.strip_prefix('*') {
        name.ends_with(rest)
    } else if let Some(rest) = pattern.strip_suffix('*') {
        name.starts_with(rest)
    } else {
        pattern == name
    }
}

fn build_theme(
    palette: &HashMap<u8, Rgb8>,
    named: &HashMap<&'static str, Rgb8>,
    warnings: &mut Vec<String>,
) -> Option<ThemeColors> {
    let foreground = *named.get("foreground")?;
    let background = *named.get("background")?;
    let cursor = named.get("cursor").copied().unwrap_or(foreground);

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
            "Kitty palette missing indices {:?}; filled with black",
            missing
        ));
    }
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
    fn parses_basic_settings_and_palette() {
        let palette = (0..16u8)
            .map(|i| format!("color{i} #{:02x}{:02x}{:02x}", i, i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let config = format!(
            "# kitty\nfont_family JetBrains Mono\nfont_size 14\nbackground_opacity 0.92\nbackground_blur 30\ncursor_shape beam\ncursor_blink_interval 0.5\nwindow_padding_width 6 10\nforeground #d8dee9\nbackground #2e3440\ncursor #e5e9f0\n{palette}\n"
        );
        let file = write_file(&config);
        let imported = import(file.path()).unwrap();
        assert!(imported.theme.is_some());
        let settings: Vec<_> = imported
            .settings
            .iter()
            .map(|(id, value)| (*id, value.clone()))
            .collect();
        assert!(settings.contains(&(RootSettingId::FontSize, "14".to_string())));
        assert!(settings.contains(&(RootSettingId::PaddingX, "6".to_string())));
        assert!(settings.contains(&(RootSettingId::PaddingY, "10".to_string())));
        assert!(settings.contains(&(RootSettingId::BackgroundBlur, "true".to_string())));
        assert!(settings.contains(&(RootSettingId::BackgroundOpacity, "0.920".to_string())));
        assert!(settings.contains(&(RootSettingId::CursorStyle, "line".to_string())));
        assert!(settings.contains(&(RootSettingId::CursorBlink, "true".to_string())));
    }

    #[test]
    fn geninclude_emits_warning() {
        let config = "geninclude /tmp/foo.py\nfont_size 12\n";
        let file = write_file(config);
        let imported = import(file.path()).unwrap();
        assert!(imported.warnings.iter().any(|w| w.contains("geninclude")));
    }
}
