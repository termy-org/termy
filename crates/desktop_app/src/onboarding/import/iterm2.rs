use std::path::{Path, PathBuf};

use plist::Value;
use termy_config_core::RootSettingId;
use termy_themes::{Rgb8, ThemeColors};

use super::{
    DetectedSource, ImportSourceKind, ImportedConfig, clamp_opacity, extract_app_icon,
    float_to_rgb, home_dir, macos_app_bundle_path,
};

pub(crate) fn detect() -> DetectedSource {
    let app_path = macos_app_bundle_path("iTerm.app");
    let app_installed = app_path.is_some();
    let icon_path = app_path
        .as_ref()
        .and_then(|path| extract_app_icon("iterm2", path));
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = home_dir() {
        candidates.push(home.join("Library/Preferences/com.googlecode.iterm2.plist"));
        candidates.push(home.join("Library/Application Support/iTerm2/DynamicProfiles"));
    }
    let config_path = candidates.into_iter().find(|path| path.exists());
    DetectedSource {
        kind: ImportSourceKind::ITerm2,
        app_installed,
        config_path,
        status_hint: None,
        icon_path,
    }
}

pub(crate) fn import(path: &Path) -> Result<ImportedConfig, String> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("itermcolors") {
        return import_color_preset(path);
    }
    if path.is_dir() {
        return import_dynamic_profiles_dir(path);
    }
    import_main_plist(path)
}

fn import_main_plist(path: &Path) -> Result<ImportedConfig, String> {
    let value = plist::Value::from_file(path)
        .map_err(|error| format!("Failed to read plist {}: {error}", path.display()))?;
    let mut imported = ImportedConfig::new(ImportSourceKind::ITerm2);

    let Some(dict) = value.as_dictionary() else {
        imported
            .warnings
            .push("iTerm2 plist root is not a dictionary".into());
        return Ok(imported);
    };

    let bookmarks = dict.get("New Bookmarks").and_then(|v| v.as_array());
    let Some(bookmark) = bookmarks.and_then(|list| pick_default_bookmark(list)) else {
        imported
            .warnings
            .push("No iTerm2 profile found in plist".into());
        return Ok(imported);
    };

    apply_bookmark_to_imported(bookmark, &mut imported);
    Ok(imported)
}

fn import_color_preset(path: &Path) -> Result<ImportedConfig, String> {
    let value = plist::Value::from_file(path)
        .map_err(|error| format!("Failed to read .itermcolors {}: {error}", path.display()))?;
    let mut imported = ImportedConfig::new(ImportSourceKind::ITerm2);
    let dict = value
        .as_dictionary()
        .ok_or_else(|| "iTerm2 color preset root is not a dictionary".to_string())?;
    imported.theme = extract_theme_from_dict(dict, &mut imported.warnings);
    Ok(imported)
}

fn import_dynamic_profiles_dir(dir: &Path) -> Result<ImportedConfig, String> {
    let mut imported = ImportedConfig::new(ImportSourceKind::ITerm2);
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("Failed to read {}: {error}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(value) = plist::Value::from_file(&path) else {
            continue;
        };
        let Some(top) = value.as_dictionary() else {
            continue;
        };
        let bookmarks = top
            .get("Profiles")
            .and_then(|v| v.as_array())
            .or_else(|| top.get("New Bookmarks").and_then(|v| v.as_array()));
        if let Some(bookmarks) = bookmarks
            && let Some(bookmark) = pick_default_bookmark(bookmarks)
        {
            apply_bookmark_to_imported(bookmark, &mut imported);
            return Ok(imported);
        }
    }
    imported
        .warnings
        .push("No usable iTerm2 dynamic profile found".into());
    Ok(imported)
}

fn pick_default_bookmark(bookmarks: &[Value]) -> Option<&plist::Dictionary> {
    let mut first = None;
    for value in bookmarks {
        let Some(dict) = value.as_dictionary() else {
            continue;
        };
        if first.is_none() {
            first = Some(dict);
        }
        let is_default = dict
            .get("Default Bookmark")
            .and_then(|v| v.as_string())
            .is_some_and(|s| s.eq_ignore_ascii_case("yes"));
        if is_default {
            return Some(dict);
        }
    }
    first
}

fn apply_bookmark_to_imported(bookmark: &plist::Dictionary, imported: &mut ImportedConfig) {
    if let Some(font) = bookmark.get("Normal Font").and_then(|v| v.as_string()) {
        let (family, size) = split_font(font);
        if !family.is_empty() {
            imported.settings.push((RootSettingId::FontFamily, family));
        }
        if let Some(size) = size {
            imported
                .settings
                .push((RootSettingId::FontSize, format!("{size}")));
        }
    }

    if let Some(spacing) = bookmark.get("Vertical Spacing").and_then(value_as_f64) {
        let clamped = spacing.clamp(0.8, 2.5);
        imported
            .settings
            .push((RootSettingId::LineHeight, format!("{clamped:.3}")));
    }

    if let Some(transparency) = bookmark.get("Transparency").and_then(value_as_f64) {
        let opacity = clamp_opacity((1.0 - transparency as f32).max(0.0));
        imported
            .settings
            .push((RootSettingId::BackgroundOpacity, format!("{opacity:.3}")));
    }
    if let Some(blur) = bookmark.get("Blur").and_then(|v| v.as_boolean()) {
        imported
            .settings
            .push((RootSettingId::BackgroundBlur, blur.to_string()));
    }

    if let Some(cursor_type) = bookmark.get("Cursor Type").and_then(value_as_i64) {
        let style = match cursor_type {
            2 => Some("block"),
            0 | 1 => Some("line"),
            _ => None,
        };
        if cursor_type == 0 {
            imported
                .warnings
                .push("iTerm2 underline cursor has no Termy equivalent; using 'line'".into());
        }
        if let Some(style) = style {
            imported
                .settings
                .push((RootSettingId::CursorStyle, style.to_string()));
        }
    }
    if let Some(blink) = bookmark.get("Blinking Cursor").and_then(|v| v.as_boolean()) {
        imported
            .settings
            .push((RootSettingId::CursorBlink, blink.to_string()));
    }

    let unlimited = bookmark
        .get("Unlimited Scrollback")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);
    if !unlimited
        && let Some(lines) = bookmark.get("Scrollback Lines").and_then(value_as_i64)
        && lines >= 0
    {
        imported
            .settings
            .push((RootSettingId::ScrollbackHistory, lines.to_string()));
    }

    let custom_cmd = bookmark
        .get("Custom Command")
        .and_then(|v| v.as_string())
        .is_some_and(|s| s.eq_ignore_ascii_case("yes"));
    if custom_cmd
        && let Some(command) = bookmark.get("Command").and_then(|v| v.as_string())
        && !command.is_empty()
    {
        imported
            .settings
            .push((RootSettingId::Shell, command.to_string()));
    }

    imported.theme = extract_theme_from_dict(bookmark, &mut imported.warnings);
}

fn extract_theme_from_dict(
    dict: &plist::Dictionary,
    warnings: &mut Vec<String>,
) -> Option<ThemeColors> {
    let foreground = parse_color_dict(dict.get("Foreground Color"))?;
    let background = parse_color_dict(dict.get("Background Color"))?;
    let cursor = parse_color_dict(dict.get("Cursor Color")).unwrap_or(foreground);

    let mut ansi = [Rgb8::new(0, 0, 0); 16];
    let mut missing = Vec::new();
    for (index, ansi_color) in ansi.iter_mut().enumerate() {
        let key = format!("Ansi {index} Color");
        match parse_color_dict(dict.get(&key)) {
            Some(color) => *ansi_color = color,
            None => missing.push(index),
        }
    }
    if !missing.is_empty() {
        warnings.push(format!(
            "iTerm2 palette missing indices {missing:?}; filled with black"
        ));
    }
    Some(ThemeColors {
        ansi,
        foreground,
        background,
        cursor,
    })
}

fn parse_color_dict(value: Option<&Value>) -> Option<Rgb8> {
    let dict = value?.as_dictionary()?;
    let r = dict.get("Red Component").and_then(value_as_f64)?;
    let g = dict.get("Green Component").and_then(value_as_f64)?;
    let b = dict.get("Blue Component").and_then(value_as_f64)?;
    Some(Rgb8::new(float_to_rgb(r), float_to_rgb(g), float_to_rgb(b)))
}

fn value_as_f64(value: &Value) -> Option<f64> {
    if let Some(real) = value.as_real() {
        return Some(real);
    }
    if let Some(int) = value.as_signed_integer() {
        return Some(int as f64);
    }
    None
}

fn value_as_i64(value: &Value) -> Option<i64> {
    if let Some(int) = value.as_signed_integer() {
        return Some(int);
    }
    if let Some(real) = value.as_real() {
        return Some(real as i64);
    }
    None
}

fn split_font(font: &str) -> (String, Option<f32>) {
    let parts: Vec<&str> = font.rsplitn(2, ' ').collect();
    if parts.len() == 2
        && let Ok(size) = parts[0].parse::<f32>()
    {
        let raw_family = parts[1].trim();
        let family = raw_family
            .split('-')
            .next()
            .unwrap_or(raw_family)
            .replace('_', " ");
        return (family, Some(size));
    }
    (font.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const ITERMCOLORS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Foreground Color</key>
    <dict>
        <key>Red Component</key><real>0.8</real>
        <key>Green Component</key><real>0.8</real>
        <key>Blue Component</key><real>0.8</real>
    </dict>
    <key>Background Color</key>
    <dict>
        <key>Red Component</key><real>0.1</real>
        <key>Green Component</key><real>0.1</real>
        <key>Blue Component</key><real>0.1</real>
    </dict>
    <key>Cursor Color</key>
    <dict>
        <key>Red Component</key><real>1.0</real>
        <key>Green Component</key><real>0.0</real>
        <key>Blue Component</key><real>0.0</real>
    </dict>
    ANSI_PLACEHOLDER
</dict>
</plist>"#;

    fn build_itermcolors() -> String {
        let mut ansi = String::new();
        for i in 0..16u32 {
            let component = (i as f32) / 15.0;
            ansi.push_str(&format!(
                "<key>Ansi {i} Color</key><dict><key>Red Component</key><real>{component}</real><key>Green Component</key><real>{component}</real><key>Blue Component</key><real>{component}</real></dict>"
            ));
        }
        ITERMCOLORS_XML.replace("ANSI_PLACEHOLDER", &ansi)
    }

    #[test]
    fn parses_itermcolors_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.as_file_mut()
            .write_all(build_itermcolors().as_bytes())
            .unwrap();
        let target = file.path().with_extension("itermcolors");
        std::fs::copy(file.path(), &target).unwrap();
        let imported = import(&target).unwrap();
        let theme = imported.theme.expect("theme");
        assert_eq!(theme.foreground, Rgb8::new(204, 204, 204));
        assert_eq!(theme.background, Rgb8::new(26, 26, 26));
        assert_eq!(theme.cursor, Rgb8::new(255, 0, 0));
        assert_eq!(theme.ansi[0], Rgb8::new(0, 0, 0));
        assert_eq!(theme.ansi[15], Rgb8::new(255, 255, 255));
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn split_font_extracts_family_and_size() {
        let (family, size) = split_font("Menlo-Regular 14");
        assert_eq!(family, "Menlo");
        assert_eq!(size, Some(14.0));
    }
}
