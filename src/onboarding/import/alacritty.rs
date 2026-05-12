use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use termy_config_core::RootSettingId;
use termy_themes::{Rgb8, ThemeColors};

use super::{
    DetectedSource, ImportSourceKind, ImportedConfig, clamp_opacity, extract_app_icon, home_dir,
    macos_app_bundle_path, map_cursor_shape, parse_hex_color,
};

const MAX_IMPORT_DEPTH: usize = 4;

pub(crate) fn detect() -> DetectedSource {
    let app_path = macos_app_bundle_path("Alacritty.app");
    let app_installed = app_path.is_some();
    let icon_path = app_path
        .as_ref()
        .and_then(|path| extract_app_icon("alacritty", path));
    let candidates = candidates();
    let config_path = candidates.into_iter().find(|path| path.exists());
    let mut status_hint = None;
    if let Some(path) = config_path.as_ref()
        && path.extension().and_then(|ext| ext.to_str()) == Some("yml")
    {
        status_hint = Some("Legacy YAML config detected — run `alacritty migrate`".into());
    }
    DetectedSource {
        kind: ImportSourceKind::Alacritty,
        app_installed,
        config_path,
        status_hint,
        icon_path,
    }
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home_dir() {
        paths.push(home.join(".config/alacritty/alacritty.toml"));
        paths.push(home.join("Library/Application Support/alacritty/alacritty.toml"));
        paths.push(home.join(".alacritty.toml"));
        paths.push(home.join(".config/alacritty/alacritty.yml"));
    }
    paths
}

pub(crate) fn import(path: &Path) -> Result<ImportedConfig, String> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("yml") {
        let mut imported = ImportedConfig::new(ImportSourceKind::Alacritty);
        imported.warnings.push(
            "Legacy YAML config is not supported. Run `alacritty migrate` to convert to TOML."
                .into(),
        );
        return Ok(imported);
    }

    let mut imported = ImportedConfig::new(ImportSourceKind::Alacritty);
    let mut merged = AlacrittyConfig::default();
    let mut visited = HashSet::new();
    parse_recursive(path, 0, &mut imported, &mut merged, &mut visited)?;

    apply_settings(&merged, &mut imported);
    imported.theme = build_theme(&merged.colors);
    if imported.theme.is_some() && !merged.colors.is_complete() {
        imported.warnings.push(
            "Alacritty palette was partially specified; missing slots filled with black".into(),
        );
    }
    Ok(imported)
}

fn parse_recursive(
    path: &Path,
    depth: usize,
    imported: &mut ImportedConfig,
    merged: &mut AlacrittyConfig,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    if depth > MAX_IMPORT_DEPTH {
        imported.warnings.push(format!(
            "Skipped import past depth limit: {}",
            path.display()
        ));
        return Ok(());
    }

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(());
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    let parsed: AlacrittyConfig = toml::from_str(&contents)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;

    let imports = parsed
        .general
        .as_ref()
        .and_then(|g| g.import.clone())
        .unwrap_or_default();

    merged.merge(parsed);

    for import_path in imports {
        let resolved = expand_user(&import_path, path);
        if resolved.exists() {
            parse_recursive(&resolved, depth + 1, imported, merged, visited)?;
        } else {
            imported
                .warnings
                .push(format!("Alacritty import not found: {}", import_path));
        }
    }

    Ok(())
}

fn apply_settings(config: &AlacrittyConfig, imported: &mut ImportedConfig) {
    if let Some(font) = config.font.as_ref() {
        if let Some(family) = font.normal.as_ref().and_then(|n| n.family.clone()) {
            imported.settings.push((RootSettingId::FontFamily, family));
        }
        if let Some(size) = font.size {
            imported
                .settings
                .push((RootSettingId::FontSize, format!("{size}")));
        }
    }

    if let Some(window) = config.window.as_ref() {
        if let Some(padding) = window.padding.as_ref() {
            imported
                .settings
                .push((RootSettingId::PaddingX, padding.x.to_string()));
            imported
                .settings
                .push((RootSettingId::PaddingY, padding.y.to_string()));
        }
        if let Some(opacity) = window.opacity {
            imported.settings.push((
                RootSettingId::BackgroundOpacity,
                format!("{:.3}", clamp_opacity(opacity)),
            ));
        }
        if let Some(blur) = window.blur {
            imported
                .settings
                .push((RootSettingId::BackgroundBlur, blur.to_string()));
        }
    }

    if let Some(cursor) = config.cursor.as_ref()
        && let Some(style) = cursor.style.as_ref()
    {
        if let Some(shape) = style.shape.as_ref()
            && let Some(mapped) = map_cursor_shape(shape, &mut imported.warnings)
        {
            imported
                .settings
                .push((RootSettingId::CursorStyle, mapped.to_string()));
        }
        if let Some(blinking) = style.blinking.as_ref() {
            let normalized = blinking.to_ascii_lowercase();
            let blink = matches!(normalized.as_str(), "on" | "always");
            imported
                .settings
                .push((RootSettingId::CursorBlink, blink.to_string()));
        }
    }

    if let Some(scrolling) = config.scrolling.as_ref()
        && let Some(history) = scrolling.history
    {
        imported
            .settings
            .push((RootSettingId::ScrollbackHistory, history.to_string()));
    }

    if let Some(terminal) = config.terminal.as_ref()
        && let Some(shell) = terminal.shell.as_ref()
        && let Some(program) = shell.program.as_ref()
    {
        imported
            .settings
            .push((RootSettingId::Shell, program.clone()));
    }
}

fn build_theme(colors: &ColorsConfig) -> Option<ThemeColors> {
    let primary = colors.primary.as_ref()?;
    let foreground = parse_hex_color(primary.foreground.as_deref()?)?;
    let background = parse_hex_color(primary.background.as_deref()?)?;
    let cursor = colors
        .cursor
        .as_ref()
        .and_then(|c| c.cursor.as_deref())
        .and_then(parse_hex_color)
        .unwrap_or(foreground);

    let normal = colors.normal.as_ref();
    let bright = colors.bright.as_ref();

    let resolve = |slot: Option<&AnsiColors>, key: &str| -> Option<Rgb8> {
        let value = slot?;
        match key {
            "black" => value.black.as_deref(),
            "red" => value.red.as_deref(),
            "green" => value.green.as_deref(),
            "yellow" => value.yellow.as_deref(),
            "blue" => value.blue.as_deref(),
            "magenta" => value.magenta.as_deref(),
            "cyan" => value.cyan.as_deref(),
            "white" => value.white.as_deref(),
            _ => None,
        }
        .and_then(parse_hex_color)
    };

    let names = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    let mut ansi = [Rgb8::new(0, 0, 0); 16];
    for (i, name) in names.iter().enumerate() {
        if let Some(color) = resolve(normal, name) {
            ansi[i] = color;
        }
        if let Some(color) = resolve(bright, name) {
            ansi[i + 8] = color;
        }
    }
    Some(ThemeColors {
        ansi,
        foreground,
        background,
        cursor,
    })
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

#[derive(Default, Deserialize)]
struct AlacrittyConfig {
    #[serde(default)]
    general: Option<GeneralSection>,
    #[serde(default)]
    font: Option<FontSection>,
    #[serde(default)]
    window: Option<WindowSection>,
    #[serde(default)]
    cursor: Option<CursorSection>,
    #[serde(default)]
    scrolling: Option<ScrollingSection>,
    #[serde(default)]
    terminal: Option<TerminalSection>,
    #[serde(default)]
    colors: ColorsConfig,
}

impl AlacrittyConfig {
    fn merge(&mut self, other: AlacrittyConfig) {
        if other.general.is_some() {
            self.general = other.general;
        }
        if let Some(font) = other.font {
            self.font = Some(match self.font.take() {
                Some(mut existing) => {
                    if font.size.is_some() {
                        existing.size = font.size;
                    }
                    if font.normal.is_some() {
                        existing.normal = font.normal;
                    }
                    existing
                }
                None => font,
            });
        }
        if other.window.is_some() {
            self.window = other.window;
        }
        if other.cursor.is_some() {
            self.cursor = other.cursor;
        }
        if other.scrolling.is_some() {
            self.scrolling = other.scrolling;
        }
        if other.terminal.is_some() {
            self.terminal = other.terminal;
        }
        self.colors.merge(other.colors);
    }
}

#[derive(Default, Deserialize)]
struct GeneralSection {
    #[serde(default)]
    import: Option<Vec<String>>,
}

#[derive(Default, Deserialize)]
struct FontSection {
    #[serde(default)]
    size: Option<f32>,
    #[serde(default)]
    normal: Option<FontVariant>,
}

#[derive(Default, Deserialize)]
struct FontVariant {
    #[serde(default)]
    family: Option<String>,
}

#[derive(Default, Deserialize)]
struct WindowSection {
    #[serde(default)]
    padding: Option<PaddingConfig>,
    #[serde(default)]
    opacity: Option<f32>,
    #[serde(default)]
    blur: Option<bool>,
}

#[derive(Default, Deserialize)]
struct PaddingConfig {
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
}

#[derive(Default, Deserialize)]
struct CursorSection {
    #[serde(default)]
    style: Option<CursorStyle>,
}

#[derive(Default, Deserialize)]
struct CursorStyle {
    #[serde(default)]
    shape: Option<String>,
    #[serde(default)]
    blinking: Option<String>,
}

#[derive(Default, Deserialize)]
struct ScrollingSection {
    #[serde(default)]
    history: Option<u32>,
}

#[derive(Default, Deserialize)]
struct TerminalSection {
    #[serde(default)]
    shell: Option<ShellConfig>,
}

#[derive(Default, Deserialize)]
struct ShellConfig {
    #[serde(default)]
    program: Option<String>,
}

#[derive(Default, Deserialize)]
struct ColorsConfig {
    #[serde(default)]
    primary: Option<PrimaryColors>,
    #[serde(default)]
    cursor: Option<CursorColors>,
    #[serde(default)]
    normal: Option<AnsiColors>,
    #[serde(default)]
    bright: Option<AnsiColors>,
}

impl ColorsConfig {
    fn merge(&mut self, other: ColorsConfig) {
        if other.primary.is_some() {
            self.primary = other.primary;
        }
        if other.cursor.is_some() {
            self.cursor = other.cursor;
        }
        if other.normal.is_some() {
            self.normal = other.normal;
        }
        if other.bright.is_some() {
            self.bright = other.bright;
        }
    }

    fn is_complete(&self) -> bool {
        self.primary.is_some()
            && self.normal.is_some()
            && self.bright.is_some()
    }
}

#[derive(Default, Deserialize)]
struct PrimaryColors {
    #[serde(default)]
    foreground: Option<String>,
    #[serde(default)]
    background: Option<String>,
}

#[derive(Default, Deserialize)]
struct CursorColors {
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Default, Deserialize)]
struct AnsiColors {
    #[serde(default)]
    black: Option<String>,
    #[serde(default)]
    red: Option<String>,
    #[serde(default)]
    green: Option<String>,
    #[serde(default)]
    yellow: Option<String>,
    #[serde(default)]
    blue: Option<String>,
    #[serde(default)]
    magenta: Option<String>,
    #[serde(default)]
    cyan: Option<String>,
    #[serde(default)]
    white: Option<String>,
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
    fn parses_full_config() {
        let toml = r##"
[font]
size = 13.5

[font.normal]
family = "FiraCode Nerd Font"

[window]
opacity = 0.95
blur = true

[window.padding]
x = 8
y = 10

[cursor.style]
shape = "Beam"
blinking = "On"

[scrolling]
history = 10000

[terminal.shell]
program = "/bin/zsh"

[colors.primary]
foreground = "#c5c8c6"
background = "#1d1f21"

[colors.cursor]
cursor = "#aeafad"

[colors.normal]
black = "#000000"
red = "#cc6666"
green = "#b5bd68"
yellow = "#f0c674"
blue = "#81a2be"
magenta = "#b294bb"
cyan = "#8abeb7"
white = "#c5c8c6"

[colors.bright]
black = "#666666"
red = "#d54e53"
green = "#b9ca4a"
yellow = "#e7c547"
blue = "#7aa6da"
magenta = "#c397d8"
cyan = "#70c0b1"
white = "#eaeaea"
"##;
        let file = write_file(toml);
        let imported = import(file.path()).unwrap();
        let settings: Vec<_> = imported
            .settings
            .iter()
            .map(|(id, value)| (*id, value.clone()))
            .collect();
        assert!(settings.contains(&(
            RootSettingId::FontFamily,
            "FiraCode Nerd Font".to_string()
        )));
        assert!(settings.contains(&(RootSettingId::FontSize, "13.5".to_string())));
        assert!(settings.contains(&(RootSettingId::PaddingX, "8".to_string())));
        assert!(settings.contains(&(RootSettingId::BackgroundOpacity, "0.950".to_string())));
        assert!(settings.contains(&(RootSettingId::BackgroundBlur, "true".to_string())));
        assert!(settings.contains(&(RootSettingId::CursorStyle, "line".to_string())));
        assert!(settings.contains(&(RootSettingId::CursorBlink, "true".to_string())));
        assert!(settings.contains(&(RootSettingId::Shell, "/bin/zsh".to_string())));
        let theme = imported.theme.expect("theme");
        assert_eq!(theme.foreground, Rgb8::new(0xc5, 0xc8, 0xc6));
        assert_eq!(theme.background, Rgb8::new(0x1d, 0x1f, 0x21));
        assert_eq!(theme.ansi[1], Rgb8::new(0xcc, 0x66, 0x66));
        assert_eq!(theme.ansi[9], Rgb8::new(0xd5, 0x4e, 0x53));
    }

    #[test]
    fn yml_emits_warning_only() {
        let mut file = NamedTempFile::new().unwrap();
        let yml_path = file.path().with_extension("yml");
        std::fs::copy(file.path(), &yml_path).unwrap();
        let imported = import(&yml_path).unwrap();
        assert!(imported.settings.is_empty());
        assert!(imported.theme.is_none());
        assert!(
            imported
                .warnings
                .iter()
                .any(|w| w.contains("Legacy YAML"))
        );
        // suppress unused
        let _ = file.as_file_mut();
        let _ = std::fs::remove_file(yml_path);
    }
}
