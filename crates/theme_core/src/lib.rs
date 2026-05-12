#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeColors {
    pub ansi: [Rgb8; 16],
    pub foreground: Rgb8,
    pub background: Rgb8,
    pub cursor: Rgb8,
}

pub const BUILTIN_THEME_IDS: &[&str] = &[];

pub const ANSI_COLOR_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright_black",
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
    "bright_white",
];

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ThemeRegistryIndex {
    #[serde(default = "default_registry_version")]
    pub version: u32,
    #[serde(default)]
    pub themes: Vec<ThemeRegistryEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRegistryEntry {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    pub latest_version: String,
    pub file: String,
    #[serde(default)]
    pub checksum_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeMetadata {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    pub latest_version: String,
    #[serde(default)]
    pub versions: Vec<ThemeMetadataVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeMetadataVersion {
    pub version: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
}

fn default_registry_version() -> u32 {
    1
}

pub fn normalize_theme_id(theme_id: &str) -> String {
    let mut normalized = String::new();
    let mut last_dash = false;

    for ch in theme_id.trim().chars() {
        let ch = ch.to_ascii_lowercase();
        match ch {
            'a'..='z' | '0'..='9' => {
                normalized.push(ch);
                last_dash = false;
            }
            '-' | '_' | ' ' if !normalized.is_empty() && !last_dash => {
                normalized.push('-');
                last_dash = true;
            }
            _ => {}
        }
    }

    while normalized.ends_with('-') {
        normalized.pop();
    }

    normalized
}

pub fn canonical_builtin_theme_id(theme_id: &str) -> Option<&'static str> {
    let _ = theme_id;
    None
}

pub fn format_hex(color: Rgb8) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

pub fn parse_theme_colors_json(contents: &str) -> Result<ThemeColors, String> {
    let json: serde_json::Value =
        serde_json::from_str(contents).map_err(|error| format!("Invalid JSON: {error}"))?;
    parse_theme_colors_value(&json)
}

pub fn parse_theme_colors_value(json: &serde_json::Value) -> Result<ThemeColors, String> {
    let object = json
        .as_object()
        .ok_or_else(|| "Theme JSON must be an object".to_string())?;

    let ansi = [
        parse_required_color(object, "black")?,
        parse_required_color(object, "red")?,
        parse_required_color(object, "green")?,
        parse_required_color(object, "yellow")?,
        parse_required_color(object, "blue")?,
        parse_required_color(object, "magenta")?,
        parse_required_color(object, "cyan")?,
        parse_required_color(object, "white")?,
        parse_required_color(object, "bright_black")?,
        parse_required_color(object, "bright_red")?,
        parse_required_color(object, "bright_green")?,
        parse_required_color(object, "bright_yellow")?,
        parse_required_color(object, "bright_blue")?,
        parse_required_color(object, "bright_magenta")?,
        parse_required_color(object, "bright_cyan")?,
        parse_required_color(object, "bright_white")?,
    ];

    Ok(ThemeColors {
        ansi,
        foreground: parse_required_color(object, "foreground")?,
        background: parse_required_color(object, "background")?,
        cursor: parse_required_color(object, "cursor")?,
    })
}

pub fn theme_colors_json_value(colors: &ThemeColors, schema: Option<&str>) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    if let Some(schema) = schema {
        object.insert(
            "$schema".to_string(),
            serde_json::Value::String(schema.to_string()),
        );
    }
    object.insert(
        "foreground".to_string(),
        serde_json::Value::String(format_hex(colors.foreground)),
    );
    object.insert(
        "background".to_string(),
        serde_json::Value::String(format_hex(colors.background)),
    );
    object.insert(
        "cursor".to_string(),
        serde_json::Value::String(format_hex(colors.cursor)),
    );
    for (index, name) in ANSI_COLOR_NAMES.iter().enumerate() {
        object.insert(
            (*name).to_string(),
            serde_json::Value::String(format_hex(colors.ansi[index])),
        );
    }
    serde_json::Value::Object(object)
}

pub fn theme_colors_json_pretty(
    colors: &ThemeColors,
    schema: Option<&str>,
) -> Result<String, String> {
    serde_json::to_string_pretty(&theme_colors_json_value(colors, schema))
        .map_err(|error| format!("Failed to serialize theme colors: {error}"))
}

pub fn registry_file_url(index_url: &str, file: &str) -> String {
    if file.starts_with("http://") || file.starts_with("https://") {
        return file.to_string();
    }

    let base = index_url
        .rsplit_once('/')
        .map_or_else(|| index_url.trim_end_matches('/'), |(base, _)| base);
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        file.trim_start_matches('/')
    )
}

fn parse_required_color(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Rgb8, String> {
    let value = object
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("Theme JSON is missing '{key}'"))?;

    parse_hex_color(value).ok_or_else(|| format!("Theme color '{key}' must be a #RRGGBB hex"))
}

fn parse_hex_color(value: &str) -> Option<Rgb8> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }

    Some(Rgb8 {
        r: u8::from_str_radix(&hex[0..2], 16).ok()?,
        g: u8::from_str_radix(&hex[2..4], 16).ok()?,
        b: u8::from_str_radix(&hex[4..6], 16).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        Rgb8, canonical_builtin_theme_id, format_hex, normalize_theme_id, parse_theme_colors_json,
        registry_file_url, theme_colors_json_pretty,
    };

    #[test]
    fn formats_hex_in_lowercase() {
        assert_eq!(format_hex(Rgb8::new(0xAB, 0xCD, 0xEF)), "#abcdef");
    }

    #[test]
    fn normalize_theme_id_is_stable() {
        assert_eq!(normalize_theme_id("  Tokyo_Night  "), "tokyo-night");
        assert_eq!(normalize_theme_id("gruvbox---dark"), "gruvbox-dark");
    }

    #[test]
    fn builtin_aliases_are_disabled() {
        assert_eq!(canonical_builtin_theme_id("gruvbox"), None);
        assert_eq!(canonical_builtin_theme_id("tokyonight"), None);
        assert_eq!(canonical_builtin_theme_id("default"), None);
    }

    #[test]
    fn parses_theme_color_json() {
        let json = r##"{
            "foreground": "#e5e5e5",
            "background": "#111111",
            "cursor": "#ffffff",
            "black": "#000000",
            "red": "#111111",
            "green": "#222222",
            "yellow": "#333333",
            "blue": "#444444",
            "magenta": "#555555",
            "cyan": "#666666",
            "white": "#777777",
            "bright_black": "#888888",
            "bright_red": "#999999",
            "bright_green": "#aaaaaa",
            "bright_yellow": "#bbbbbb",
            "bright_blue": "#cccccc",
            "bright_magenta": "#dddddd",
            "bright_cyan": "#eeeeee",
            "bright_white": "#ffffff"
        }"##;
        let colors = parse_theme_colors_json(json).expect("valid colors");
        assert_eq!(colors.foreground, Rgb8::new(0xe5, 0xe5, 0xe5));
        assert_eq!(colors.ansi[3], Rgb8::new(0x33, 0x33, 0x33));
    }

    #[test]
    fn serializes_theme_color_json() {
        let colors = parse_theme_colors_json(
            r##"{
                "foreground": "#e5e5e5",
                "background": "#111111",
                "cursor": "#ffffff",
                "black": "#000000",
                "red": "#111111",
                "green": "#222222",
                "yellow": "#333333",
                "blue": "#444444",
                "magenta": "#555555",
                "cyan": "#666666",
                "white": "#777777",
                "bright_black": "#888888",
                "bright_red": "#999999",
                "bright_green": "#aaaaaa",
                "bright_yellow": "#bbbbbb",
                "bright_blue": "#cccccc",
                "bright_magenta": "#dddddd",
                "bright_cyan": "#eeeeee",
                "bright_white": "#ffffff"
            }"##,
        )
        .expect("valid colors");
        let serialized = theme_colors_json_pretty(&colors, Some("./theme.schema.json"))
            .expect("serialized colors");
        assert!(serialized.contains("\"$schema\": \"./theme.schema.json\""));
        assert!(serialized.contains("\"foreground\": \"#e5e5e5\""));
    }

    #[test]
    fn resolves_registry_relative_file_urls() {
        assert_eq!(
            registry_file_url(
                "https://raw.githubusercontent.com/termy-org/themes/main/index.json",
                "themes/tokyonight/files/1.0.0.json"
            ),
            "https://raw.githubusercontent.com/termy-org/themes/main/themes/tokyonight/files/1.0.0.json"
        );
    }
}
