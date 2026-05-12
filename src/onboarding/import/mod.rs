use std::path::{Path, PathBuf};
use termy_config_core::RootSettingId;
use termy_themes::ThemeColors;

pub(crate) mod alacritty;
pub(crate) mod ghostty;
pub(crate) mod iterm2;
pub(crate) mod kitty;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ImportSourceKind {
    Ghostty,
    Alacritty,
    Kitty,
    ITerm2,
}

impl ImportSourceKind {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Ghostty => "Ghostty",
            Self::Alacritty => "Alacritty",
            Self::Kitty => "Kitty",
            Self::ITerm2 => "iTerm2",
        }
    }

    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Ghostty => "ghostty",
            Self::Alacritty => "alacritty",
            Self::Kitty => "kitty",
            Self::ITerm2 => "iterm2",
        }
    }

    pub(crate) fn all() -> [Self; 4] {
        [Self::Ghostty, Self::Alacritty, Self::Kitty, Self::ITerm2]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DetectedSource {
    pub(crate) kind: ImportSourceKind,
    pub(crate) app_installed: bool,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) status_hint: Option<String>,
    pub(crate) icon_path: Option<PathBuf>,
}

impl DetectedSource {
    pub(crate) fn importable(&self) -> bool {
        self.config_path.is_some()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ImportedConfig {
    pub(crate) source: ImportSourceKind,
    pub(crate) theme: Option<ThemeColors>,
    pub(crate) settings: Vec<(RootSettingId, String)>,
    pub(crate) warnings: Vec<String>,
}

impl ImportedConfig {
    pub(crate) fn new(source: ImportSourceKind) -> Self {
        Self {
            source,
            theme: None,
            settings: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

pub(crate) fn detect_sources() -> Vec<DetectedSource> {
    ImportSourceKind::all()
        .into_iter()
        .map(|kind| match kind {
            ImportSourceKind::Ghostty => ghostty::detect(),
            ImportSourceKind::Alacritty => alacritty::detect(),
            ImportSourceKind::Kitty => kitty::detect(),
            ImportSourceKind::ITerm2 => iterm2::detect(),
        })
        .collect()
}

pub(crate) fn run_import(source: &DetectedSource) -> Result<ImportedConfig, String> {
    let path = source
        .config_path
        .clone()
        .ok_or_else(|| format!("No {} config found", source.kind.display_name()))?;

    match source.kind {
        ImportSourceKind::Ghostty => ghostty::import(&path),
        ImportSourceKind::Alacritty => alacritty::import(&path),
        ImportSourceKind::Kitty => kitty::import(&path),
        ImportSourceKind::ITerm2 => iterm2::import(&path),
    }
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

pub(crate) fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|path| path.exists()).cloned()
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_app_installed(bundle: &str) -> bool {
    macos_app_bundle_path(bundle).is_some()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn macos_app_installed(_bundle: &str) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_app_bundle_path(bundle: &str) -> Option<PathBuf> {
    let system = PathBuf::from("/Applications").join(bundle);
    if system.exists() {
        return Some(system);
    }
    if let Some(home) = home_dir() {
        let user = home.join("Applications").join(bundle);
        if user.exists() {
            return Some(user);
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn macos_app_bundle_path(_bundle: &str) -> Option<PathBuf> {
    None
}

fn icon_cache_dir() -> Option<PathBuf> {
    let path = termy_config_core::config_path()?;
    let parent = path.parent()?.to_path_buf();
    let dir = parent.join("import_icons");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

pub(crate) fn extract_app_icon(slug: &str, app_path: &Path) -> Option<PathBuf> {
    let cache_dir = icon_cache_dir()?;
    let icon_path = cache_dir.join(format!("{slug}.png"));
    // Re-extract if the .app bundle is newer than the cached PNG (covers app updates).
    let app_modified = std::fs::metadata(app_path).and_then(|m| m.modified()).ok();
    let cached_modified = std::fs::metadata(&icon_path)
        .and_then(|m| m.modified())
        .ok();
    let cache_is_fresh = match (app_modified, cached_modified) {
        (Some(app), Some(cached)) => cached >= app,
        _ => icon_path.exists(),
    };
    if cache_is_fresh {
        return Some(icon_path);
    }

    let png = termy_native_sdk::app_icon_png_for_path(&app_path.to_string_lossy(), 96.0)?;
    if png.is_empty() {
        return None;
    }
    std::fs::write(&icon_path, &png).ok()?;
    Some(icon_path)
}

pub(crate) fn parse_hex_color(value: &str) -> Option<termy_themes::Rgb8> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(termy_themes::Rgb8::new(r, g, b))
}

pub(crate) fn float_to_rgb(component: f64) -> u8 {
    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub(crate) fn clamp_opacity(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub(crate) fn map_cursor_shape(raw: &str, warnings: &mut Vec<String>) -> Option<&'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "block" | "steady_block" | "steadyblock" | "blinking_block" | "blinkingblock" => {
            Some("block")
        }
        "bar" | "beam" | "ibeam" | "steady_bar" | "steadybar" | "blinking_bar" | "blinkingbar"
        | "line" => Some("line"),
        "underline" | "underscore" | "steady_underline" | "blinking_underline" => {
            warnings.push(format!(
                "Cursor shape '{}' has no Termy equivalent; using 'line'",
                raw.trim()
            ));
            Some("line")
        }
        _ => None,
    }
}
