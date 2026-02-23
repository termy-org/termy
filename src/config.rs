use gpui::Rgba;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const DEFAULT_TAB_TITLE_FALLBACK: &str = "Terminal";
const DEFAULT_TAB_TITLE_EXPLICIT_PREFIX: &str = "termy:tab:";
const DEFAULT_TAB_TITLE_PROMPT_FORMAT: &str = "{cwd}";
const DEFAULT_TAB_TITLE_COMMAND_FORMAT: &str = "{command}";
const DEFAULT_TERM: &str = "xterm-256color";
const DEFAULT_COLORTERM: &str = "truecolor";
const DEFAULT_MOUSE_SCROLL_MULTIPLIER: f32 = 3.0;
const DEFAULT_SCROLLBACK_HISTORY: usize = 2000;
const MAX_SCROLLBACK_HISTORY: usize = 100_000;
const DEFAULT_MAX_TABS: usize = 10;
const MAX_TABS_LIMIT: usize = 100;
const DEFAULT_INACTIVE_TAB_SCROLLBACK: Option<usize> = None;
const MIN_MOUSE_SCROLL_MULTIPLIER: f32 = 0.1;
const MAX_MOUSE_SCROLL_MULTIPLIER: f32 = 1_000.0;
const DEFAULT_CURSOR_BLINK: bool = true;

const DEFAULT_CONFIG: &str = "# Main settings\n\
theme = termy\n\
# TERM value for child shells and terminal apps\n\
term = xterm-256color\n\
# Startup directory for new terminal sessions (~ supported)\n\
# working_dir = ~/Documents\n\
# Show tab bar above the terminal grid\n\
# use_tabs = true\n\
# Maximum number of tabs (lower = less memory usage)\n\
# max_tabs = 10\n\
# Tab title mode. Supported values: smart, shell, explicit, static\n\
# smart = manual rename > explicit title > shell/app title > fallback\n\
tab_title_mode = smart\n\
# Export TERMY_* env vars for optional shell tab-title integration\n\
tab_title_shell_integration = true\n\
# Optional: static fallback tab title\n\
# tab_title_fallback = Terminal\n\
# Advanced tab-title options are documented in docs/configuration.md:\n\
# tab_title_priority = manual, explicit, shell, fallback\n\
# tab_title_explicit_prefix = termy:tab:\n\
# tab_title_prompt_format = {cwd}\n\
# tab_title_command_format = {command}\n\
# Startup window size in pixels\n\
window_width = 1280\n\
window_height = 820\n\
# Terminal font family\n\
font_family = JetBrains Mono\n\
# Terminal font size in pixels\n\
font_size = 14\n\
# Cursor style shared by terminal and inline inputs (line|block)\n\
# cursor_style = block\n\
# Enable cursor blink for terminal and inline inputs\n\
# cursor_blink = true\n\
# Terminal background opacity (0.0 = fully transparent, 1.0 = opaque)\n\
# background_opacity = 1.0\n\
# Enable/disable platform blur for transparent backgrounds\n\
# background_blur = false\n\
# Inner terminal padding in pixels\n\
padding_x = 12\n\
padding_y = 8\n\
# Mouse wheel scroll speed multiplier\n\
# mouse_scroll_multiplier = 3\n\
# Terminal scrollbar visibility: always | on_scroll | off\n\
# (while scrolled up in history, scrollbar stays visible in all modes)\n\
# scrollbar_visibility = on_scroll\n\
# Scrollbar style: neutral | muted_theme | theme\n\
# scrollbar_style = neutral\n\
\n\
# Advanced runtime settings (usually leave these as defaults)\n\
# Preferred shell executable path\n\
# shell = /bin/zsh\n\
# Fallback startup directory when working_dir is unset: home or process\n\
# working_dir_fallback = home\n\
# Advertise 24-bit color support to child apps\n\
# colorterm = truecolor\n\
# Scrollback history lines (lower = less memory, max 100000)\n\
# scrollback_history = 2000\n\
# Scrollback for inactive tabs (saves memory with many tabs)\n\
# inactive_tab_scrollback = 500\n\
# Keybindings (Ghostty-style trigger overrides)\n\
# keybind = cmd-p=toggle_command_palette\n\
# keybind = cmd-c=copy\n\
# keybind = cmd-c=unbind\n\
# keybind = clear\n\
# Show/hide shortcut badges in command palette\n\
# command_palette_show_keybinds = true\n";

pub type ThemeId = String;

const DEFAULT_THEME_ID: &str = "termy";

fn parse_theme_id(value: &str) -> Option<ThemeId> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(canonical) = termy_themes::canonical_builtin_theme_id(value) {
        return Some(canonical.to_string());
    }

    let normalized = termy_themes::normalize_theme_id(value);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn upsert_theme_assignment(contents: &str, theme_id: &str) -> String {
    let mut new_config = String::new();
    let mut replaced = false;
    let mut inserted_before_first_section = false;
    let mut in_root_section = true;

    for line in contents.lines() {
        let trimmed = line.trim();
        let is_section_header = trimmed.starts_with('[') && trimmed.ends_with(']');

        if is_section_header {
            if !replaced && !inserted_before_first_section {
                new_config.push_str(&format!("theme = {}\n", theme_id));
                inserted_before_first_section = true;
            }
            in_root_section = false;
            new_config.push_str(line);
            new_config.push('\n');
            continue;
        }

        if in_root_section {
            let mut parts = trimmed.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim();
            if key.eq_ignore_ascii_case("theme") {
                if !replaced {
                    new_config.push_str(&format!("theme = {}\n", theme_id));
                    replaced = true;
                }
                continue;
            }
        }

        new_config.push_str(line);
        new_config.push('\n');
    }

    if !replaced && !inserted_before_first_section {
        if !new_config.is_empty() && !new_config.ends_with('\n') {
            new_config.push('\n');
        }
        new_config.push_str(&format!("theme = {}\n", theme_id));
    }

    new_config
}

fn replace_or_insert_section(
    contents: &str,
    section_name: &str,
    section_lines: &[String],
) -> String {
    let mut new_config = String::new();
    let mut in_target_section = false;
    let mut target_section_found = false;
    let target_header = format!("[{}]", section_name);

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_target_section = false;
            if trimmed.eq_ignore_ascii_case(&target_header) {
                target_section_found = true;
                in_target_section = true;
                new_config.push_str(line);
                new_config.push('\n');
                for section_line in section_lines {
                    new_config.push_str(section_line);
                    new_config.push('\n');
                }
                continue;
            }
        }

        if in_target_section {
            continue;
        }

        new_config.push_str(line);
        new_config.push('\n');
    }

    if !target_section_found {
        if !new_config.is_empty() {
            new_config.push('\n');
        }
        new_config.push_str(&target_header);
        new_config.push('\n');
        for section_line in section_lines {
            new_config.push_str(section_line);
            new_config.push('\n');
        }
    }

    new_config
}

fn update_config_contents<R>(
    updater: impl FnOnce(&str) -> Result<(String, R), String>,
) -> Result<R, String> {
    let config_path =
        ensure_config_file().ok_or_else(|| "Could not locate config file".to_string())?;
    let existing =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;
    let (updated, result) = updater(&existing)?;
    fs::write(&config_path, updated).map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabTitleSource {
    Manual,
    Explicit,
    Shell,
    Fallback,
}

impl TabTitleSource {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "manual" => Some(Self::Manual),
            "explicit" => Some(Self::Explicit),
            "shell" | "app" | "terminal" => Some(Self::Shell),
            "fallback" | "default" => Some(Self::Fallback),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabTitleMode {
    Smart,
    Shell,
    Explicit,
    Static,
}

impl TabTitleMode {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "smart" => Some(Self::Smart),
            "shell" => Some(Self::Shell),
            "explicit" => Some(Self::Explicit),
            "static" => Some(Self::Static),
            _ => None,
        }
    }

    fn default_priority(self) -> Vec<TabTitleSource> {
        match self {
            Self::Smart => vec![
                TabTitleSource::Manual,
                TabTitleSource::Explicit,
                TabTitleSource::Shell,
                TabTitleSource::Fallback,
            ],
            Self::Shell => vec![
                TabTitleSource::Manual,
                TabTitleSource::Shell,
                TabTitleSource::Fallback,
            ],
            Self::Explicit => vec![
                TabTitleSource::Manual,
                TabTitleSource::Explicit,
                TabTitleSource::Fallback,
            ],
            Self::Static => vec![TabTitleSource::Manual, TabTitleSource::Fallback],
        }
    }
}

#[derive(Debug, Clone)]
pub struct TabTitleConfig {
    pub mode: TabTitleMode,
    pub priority: Vec<TabTitleSource>,
    pub fallback: String,
    pub explicit_prefix: String,
    pub shell_integration: bool,
    pub prompt_format: String,
    pub command_format: String,
}

impl Default for TabTitleConfig {
    fn default() -> Self {
        Self {
            mode: TabTitleMode::Smart,
            priority: TabTitleMode::Smart.default_priority(),
            fallback: DEFAULT_TAB_TITLE_FALLBACK.to_string(),
            explicit_prefix: DEFAULT_TAB_TITLE_EXPLICIT_PREFIX.to_string(),
            shell_integration: true,
            prompt_format: DEFAULT_TAB_TITLE_PROMPT_FORMAT.to_string(),
            command_format: DEFAULT_TAB_TITLE_COMMAND_FORMAT.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Line,
    Block,
}

impl CursorStyle {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "line" | "bar" | "beam" | "ibeam" => Some(Self::Line),
            "block" | "box" => Some(Self::Block),
            _ => None,
        }
    }
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self::Block
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalScrollbarVisibility {
    Off,
    Always,
    OnScroll,
}

impl TerminalScrollbarVisibility {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "always" => Some(Self::Always),
            "on_scroll" | "onscroll" => Some(Self::OnScroll),
            _ => None,
        }
    }
}

impl Default for TerminalScrollbarVisibility {
    fn default() -> Self {
        Self::OnScroll
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalScrollbarStyle {
    Neutral,
    MutedTheme,
    Theme,
}

impl TerminalScrollbarStyle {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "neutral" => Some(Self::Neutral),
            "muted_theme" | "mutedtheme" => Some(Self::MutedTheme),
            "theme" => Some(Self::Theme),
            _ => None,
        }
    }
}

impl Default for TerminalScrollbarStyle {
    fn default() -> Self {
        Self::Neutral
    }
}

#[derive(Debug, Clone, Default)]
pub struct CustomColors {
    pub foreground: Option<Rgba>,
    pub background: Option<Rgba>,
    pub cursor: Option<Rgba>,
    pub ansi: [Option<Rgba>; 16],
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub theme: ThemeId,
    pub working_dir: Option<String>,
    pub working_dir_fallback: WorkingDirFallback,
    pub use_tabs: bool,
    pub max_tabs: usize,
    pub tab_title: TabTitleConfig,
    pub shell: Option<String>,
    pub term: String,
    pub colorterm: Option<String>,
    pub window_width: f32,
    pub window_height: f32,
    pub font_family: String,
    pub font_size: f32,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub background_opacity: f32,
    pub background_blur: bool,
    pub padding_x: f32,
    pub padding_y: f32,
    pub mouse_scroll_multiplier: f32,
    pub terminal_scrollbar_visibility: TerminalScrollbarVisibility,
    pub terminal_scrollbar_style: TerminalScrollbarStyle,
    pub scrollback_history: usize,
    pub inactive_tab_scrollback: Option<usize>,
    pub command_palette_show_keybinds: bool,
    pub keybind_lines: Vec<KeybindConfigLine>,
    pub colors: CustomColors,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindConfigLine {
    pub line_number: usize,
    pub value: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME_ID.to_string(),
            working_dir: None,
            working_dir_fallback: WorkingDirFallback::default(),
            use_tabs: true,
            max_tabs: DEFAULT_MAX_TABS,
            tab_title: TabTitleConfig::default(),
            shell: None,
            term: DEFAULT_TERM.to_string(),
            colorterm: Some(DEFAULT_COLORTERM.to_string()),
            window_width: 1280.0,
            window_height: 820.0,
            font_family: "JetBrains Mono".to_string(),
            font_size: 14.0,
            cursor_style: CursorStyle::default(),
            cursor_blink: DEFAULT_CURSOR_BLINK,
            background_opacity: 1.0,
            background_blur: false,
            padding_x: 12.0,
            padding_y: 8.0,
            mouse_scroll_multiplier: DEFAULT_MOUSE_SCROLL_MULTIPLIER,
            terminal_scrollbar_visibility: TerminalScrollbarVisibility::default(),
            terminal_scrollbar_style: TerminalScrollbarStyle::default(),
            scrollback_history: DEFAULT_SCROLLBACK_HISTORY,
            inactive_tab_scrollback: DEFAULT_INACTIVE_TAB_SCROLLBACK,
            command_palette_show_keybinds: true,
            keybind_lines: Vec::new(),
            colors: CustomColors::default(),
        }
    }
}

impl AppConfig {
    pub fn load_or_create() -> Self {
        let mut config = Self::default();
        let Some(path) = ensure_config_file() else {
            return config;
        };

        if let Ok(contents) = fs::read_to_string(&path) {
            config = Self::from_contents(&contents);
        }

        config
    }

    fn from_contents(contents: &str) -> Self {
        let mut config = Self::default();
        let mut tab_title_priority_overridden = false;
        let mut in_colors_section = false;

        for (line_number, line) in contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let section = &line[1..line.len() - 1].trim().to_ascii_lowercase();
                in_colors_section = section == "colors";
                continue;
            }

            let mut parts = line.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim();
            let value = parts.next().unwrap_or("").trim();

            if in_colors_section {
                parse_color_entry(&mut config.colors, key, value);
                continue;
            }

            if key.eq_ignore_ascii_case("theme") {
                if let Some(theme) = parse_theme_id(value) {
                    config.theme = theme;
                }
            }

            if key.eq_ignore_ascii_case("working_dir") && !value.is_empty() {
                config.working_dir = Some(value.to_string());
            }

            if key.eq_ignore_ascii_case("working_dir_fallback")
                || key.eq_ignore_ascii_case("default_working_dir")
            {
                if let Some(fallback) = WorkingDirFallback::from_str(value) {
                    config.working_dir_fallback = fallback;
                }
            }

            if key.eq_ignore_ascii_case("use_tabs") {
                if let Some(use_tabs) = parse_bool(value) {
                    config.use_tabs = use_tabs;
                }
            }

            if key.eq_ignore_ascii_case("max_tabs") {
                if let Ok(max_tabs) = value.parse::<usize>() {
                    config.max_tabs = max_tabs.clamp(1, MAX_TABS_LIMIT);
                }
            }

            if key.eq_ignore_ascii_case("tab_title_priority") {
                if let Some(priority) = parse_tab_title_priority(value) {
                    config.tab_title.priority = priority;
                    tab_title_priority_overridden = true;
                }
            }

            if key.eq_ignore_ascii_case("tab_title_mode") {
                if let Some(mode) = TabTitleMode::from_str(value) {
                    config.tab_title.mode = mode;
                }
            }

            if key.eq_ignore_ascii_case("tab_title_fallback") {
                if let Some(fallback) = parse_string_value(value) {
                    config.tab_title.fallback = fallback;
                }
            }

            if key.eq_ignore_ascii_case("tab_title_explicit_prefix") {
                if let Some(prefix) = parse_string_value(value) {
                    config.tab_title.explicit_prefix = prefix;
                }
            }

            if key.eq_ignore_ascii_case("tab_title_shell_integration") {
                if let Some(enabled) = parse_bool(value) {
                    config.tab_title.shell_integration = enabled;
                }
            }

            if key.eq_ignore_ascii_case("tab_title_prompt_format") {
                if let Some(format) = parse_string_value(value) {
                    config.tab_title.prompt_format = format;
                }
            }

            if key.eq_ignore_ascii_case("tab_title_command_format") {
                if let Some(format) = parse_string_value(value) {
                    config.tab_title.command_format = format;
                }
            }

            if key.eq_ignore_ascii_case("shell") {
                config.shell = parse_optional_string_value(value);
            }

            if key.eq_ignore_ascii_case("term") {
                if let Some(term) = parse_string_value(value) {
                    config.term = term;
                }
            }

            if key.eq_ignore_ascii_case("colorterm") {
                config.colorterm = parse_optional_string_value(value);
            }

            if key.eq_ignore_ascii_case("window_width") {
                if let Ok(window_width) = value.parse::<f32>() {
                    if window_width > 0.0 {
                        config.window_width = window_width;
                    }
                }
            }

            if key.eq_ignore_ascii_case("window_height") {
                if let Ok(window_height) = value.parse::<f32>() {
                    if window_height > 0.0 {
                        config.window_height = window_height;
                    }
                }
            }

            if key.eq_ignore_ascii_case("font_family") {
                if let Some(font_family) = parse_string_value(value) {
                    config.font_family = font_family;
                }
            }

            if key.eq_ignore_ascii_case("font_size") {
                if let Ok(font_size) = value.parse::<f32>() {
                    if font_size > 0.0 {
                        config.font_size = font_size;
                    }
                }
            }

            if key.eq_ignore_ascii_case("cursor_style") {
                if let Some(cursor_style) = CursorStyle::from_str(value) {
                    config.cursor_style = cursor_style;
                }
            }

            if key.eq_ignore_ascii_case("cursor_blink") {
                if let Some(cursor_blink) = parse_bool(value) {
                    config.cursor_blink = cursor_blink;
                }
            }

            if key.eq_ignore_ascii_case("background_opacity") {
                if let Ok(opacity) = value.parse::<f32>() {
                    config.background_opacity = opacity.clamp(0.0, 1.0);
                }
            }

            if key.eq_ignore_ascii_case("background_blur") {
                if let Some(enabled) = parse_bool(value) {
                    config.background_blur = enabled;
                }
            }

            if key.eq_ignore_ascii_case("padding_x") {
                if let Ok(padding_x) = value.parse::<f32>() {
                    if padding_x >= 0.0 {
                        config.padding_x = padding_x;
                    }
                }
            }

            if key.eq_ignore_ascii_case("padding_y") {
                if let Ok(padding_y) = value.parse::<f32>() {
                    if padding_y >= 0.0 {
                        config.padding_y = padding_y;
                    }
                }
            }

            if key.eq_ignore_ascii_case("mouse_scroll_multiplier") {
                if let Ok(multiplier) = value.parse::<f32>()
                    && multiplier.is_finite()
                {
                    config.mouse_scroll_multiplier =
                        multiplier.clamp(MIN_MOUSE_SCROLL_MULTIPLIER, MAX_MOUSE_SCROLL_MULTIPLIER);
                }
            }

            if key.eq_ignore_ascii_case("scrollbar_visibility") {
                if let Some(visibility) = TerminalScrollbarVisibility::from_str(value) {
                    config.terminal_scrollbar_visibility = visibility;
                }
            }

            if key.eq_ignore_ascii_case("scrollbar_style") {
                if let Some(style) = TerminalScrollbarStyle::from_str(value) {
                    config.terminal_scrollbar_style = style;
                }
            }

            if key.eq_ignore_ascii_case("scrollback_history")
                || key.eq_ignore_ascii_case("scrollback")
            {
                if let Ok(history) = value.parse::<usize>() {
                    config.scrollback_history = history.min(MAX_SCROLLBACK_HISTORY);
                }
            }

            if key.eq_ignore_ascii_case("inactive_tab_scrollback") {
                if let Ok(history) = value.parse::<usize>() {
                    config.inactive_tab_scrollback = Some(history.min(MAX_SCROLLBACK_HISTORY));
                }
            }

            if key.eq_ignore_ascii_case("command_palette_show_keybinds") {
                if let Some(show) = parse_bool(value) {
                    config.command_palette_show_keybinds = show;
                }
            }

            if key.eq_ignore_ascii_case("keybind")
                && let Some(raw) = parse_string_value(value)
            {
                config.keybind_lines.push(KeybindConfigLine {
                    line_number: line_number + 1,
                    value: raw,
                });
            }
        }

        if !tab_title_priority_overridden {
            config.tab_title.priority = config.tab_title.mode.default_priority();
        }

        config
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_string_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let unquoted = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    let unquoted = unquoted.trim();
    if unquoted.is_empty() {
        return None;
    }

    Some(unquoted.to_string())
}

fn parse_optional_string_value(value: &str) -> Option<String> {
    let parsed = parse_string_value(value)?;
    let normalized = parsed.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "none" | "unset" | "default" | "auto") {
        return None;
    }
    Some(parsed)
}

fn parse_hex_color(value: &str) -> Option<Rgba> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    })
}

fn parse_color_entry(colors: &mut CustomColors, key: &str, value: &str) {
    let key_lower = key.to_ascii_lowercase();
    let color = match parse_hex_color(value) {
        Some(c) => c,
        None => return,
    };

    match key_lower.as_str() {
        "foreground" | "fg" => colors.foreground = Some(color),
        "background" | "bg" => colors.background = Some(color),
        "cursor" => colors.cursor = Some(color),
        "black" | "color0" => colors.ansi[0] = Some(color),
        "red" | "color1" => colors.ansi[1] = Some(color),
        "green" | "color2" => colors.ansi[2] = Some(color),
        "yellow" | "color3" => colors.ansi[3] = Some(color),
        "blue" | "color4" => colors.ansi[4] = Some(color),
        "magenta" | "color5" => colors.ansi[5] = Some(color),
        "cyan" | "color6" => colors.ansi[6] = Some(color),
        "white" | "color7" => colors.ansi[7] = Some(color),
        "bright_black" | "color8" => colors.ansi[8] = Some(color),
        "bright_red" | "color9" => colors.ansi[9] = Some(color),
        "bright_green" | "color10" => colors.ansi[10] = Some(color),
        "bright_yellow" | "color11" => colors.ansi[11] = Some(color),
        "bright_blue" | "color12" => colors.ansi[12] = Some(color),
        "bright_magenta" | "color13" => colors.ansi[13] = Some(color),
        "bright_cyan" | "color14" => colors.ansi[14] = Some(color),
        "bright_white" | "color15" => colors.ansi[15] = Some(color),
        _ => {}
    }
}

pub fn import_colors_from_json(json_path: &Path) -> Result<String, String> {
    let contents =
        fs::read_to_string(json_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let json: serde_json::Value =
        serde_json::from_str(&contents).map_err(|e| format!("Invalid JSON: {}", e))?;

    let colors = json
        .as_object()
        .ok_or_else(|| "JSON must be an object".to_string())?;

    let mut color_lines = Vec::new();

    for (key, value) in colors {
        if key.starts_with('$') {
            continue;
        }

        let hex = value
            .as_str()
            .ok_or_else(|| format!("Color '{}' must be a hex string", key))?;

        if parse_hex_color(hex).is_none() {
            return Err(format!("Invalid hex color for '{}': {}", key, hex));
        }

        let config_key = match key.to_ascii_lowercase().as_str() {
            "foreground" | "fg" => "foreground",
            "background" | "bg" => "background",
            "cursor" => "cursor",
            "black" | "color0" => "black",
            "red" | "color1" => "red",
            "green" | "color2" => "green",
            "yellow" | "color3" => "yellow",
            "blue" | "color4" => "blue",
            "magenta" | "color5" => "magenta",
            "cyan" | "color6" => "cyan",
            "white" | "color7" => "white",
            "bright_black" | "brightblack" | "color8" => "bright_black",
            "bright_red" | "brightred" | "color9" => "bright_red",
            "bright_green" | "brightgreen" | "color10" => "bright_green",
            "bright_yellow" | "brightyellow" | "color11" => "bright_yellow",
            "bright_blue" | "brightblue" | "color12" => "bright_blue",
            "bright_magenta" | "brightmagenta" | "color13" => "bright_magenta",
            "bright_cyan" | "brightcyan" | "color14" => "bright_cyan",
            "bright_white" | "brightwhite" | "color15" => "bright_white",
            _ => continue,
        };

        color_lines.push(format!("{} = {}", config_key, hex));
    }

    if color_lines.is_empty() {
        return Err("No valid colors found in JSON".to_string());
    }
    let color_count = color_lines.len();
    update_config_contents(|existing| {
        Ok((
            replace_or_insert_section(existing, "colors", &color_lines),
            (),
        ))
    })?;
    Ok(format!("Imported {} colors", color_count))
}

pub fn set_theme_in_config(theme_id: &str) -> Result<String, String> {
    let theme = parse_theme_id(theme_id).ok_or_else(|| "Invalid theme id".to_string())?;
    update_config_contents(|existing| {
        Ok((
            upsert_theme_assignment(existing, &theme),
            format!("Theme set to {}", theme),
        ))
    })
}

fn upsert_config_value(contents: &str, key: &str, value: &str) -> String {
    let mut new_config = String::new();
    let mut replaced = false;
    let mut in_root_section = true;

    for line in contents.lines() {
        let trimmed = line.trim();
        let is_section_header = trimmed.starts_with('[') && trimmed.ends_with(']');

        if is_section_header {
            if !replaced && in_root_section {
                new_config.push_str(&format!("{} = {}\n", key, value));
                replaced = true;
            }
            in_root_section = false;
        }

        if in_root_section && !trimmed.starts_with('#') {
            let mut parts = trimmed.splitn(2, '=');
            let line_key = parts.next().unwrap_or("").trim();
            if line_key.eq_ignore_ascii_case(key) {
                if !replaced {
                    new_config.push_str(&format!("{} = {}\n", key, value));
                    replaced = true;
                }
                continue;
            }
        }

        new_config.push_str(line);
        new_config.push('\n');
    }

    if !replaced {
        if !new_config.is_empty() && !new_config.ends_with('\n') {
            new_config.push('\n');
        }
        new_config.push_str(&format!("{} = {}\n", key, value));
    }

    new_config
}

pub fn set_config_value(key: &str, value: &str) -> Result<(), String> {
    update_config_contents(|existing| Ok((upsert_config_value(existing, key, value), ())))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingDirFallback {
    Home,
    Process,
}

impl WorkingDirFallback {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "home" | "user" => Some(Self::Home),
            "process" | "cwd" => Some(Self::Process),
            _ => None,
        }
    }
}

impl Default for WorkingDirFallback {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            Self::Home
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Self::Process
        }
    }
}

fn parse_tab_title_priority(value: &str) -> Option<Vec<TabTitleSource>> {
    let mut priority = Vec::new();
    for token in value.split(',') {
        let Some(source) = TabTitleSource::from_str(token) else {
            continue;
        };

        if !priority.contains(&source) {
            priority.push(source);
        }
    }

    if priority.is_empty() {
        return None;
    }

    Some(priority)
}

pub fn ensure_config_file() -> Option<PathBuf> {
    let path = config_path()?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, DEFAULT_CONFIG);
    }
    Some(path)
}

pub fn open_config_file() {
    let Some(path) = ensure_config_file() else {
        return;
    };

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(&path).status();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(&path).status();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", "", path.to_string_lossy().as_ref()])
            .status();
    }
}

fn config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(app_data) = env::var("APPDATA")
            && !app_data.trim().is_empty()
        {
            return Some(Path::new(&app_data).join("termy").join("config.txt"));
        }

        if let Ok(user_profile) = env::var("USERPROFILE")
            && !user_profile.trim().is_empty()
        {
            return Some(Path::new(&user_profile).join(".config/termy/config.txt"));
        }
    }

    if let Ok(xdg_config_home) = env::var("XDG_CONFIG_HOME")
        && !xdg_config_home.trim().is_empty()
    {
        return Some(Path::new(&xdg_config_home).join("termy/config.txt"));
    }

    if let Ok(home) = env::var("HOME")
        && !home.trim().is_empty()
    {
        return Some(Path::new(&home).join(".config/termy/config.txt"));
    }

    env::current_dir()
        .ok()
        .map(|dir| dir.join(".config/termy/config.txt"))
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, CursorStyle, TabTitleMode, TabTitleSource, TerminalScrollbarStyle,
        TerminalScrollbarVisibility, WorkingDirFallback, replace_or_insert_section,
        upsert_theme_assignment,
    };

    #[test]
    fn tab_title_mode_sets_default_priority() {
        let config = AppConfig::from_contents(
            "tab_title_mode = static\n\
             tab_title_fallback = Session\n",
        );

        assert_eq!(config.tab_title.mode, TabTitleMode::Static);
        assert_eq!(
            config.tab_title.priority,
            vec![TabTitleSource::Manual, TabTitleSource::Fallback]
        );
        assert_eq!(config.tab_title.fallback, "Session");
    }

    #[test]
    fn tab_title_priority_overrides_mode() {
        let config = AppConfig::from_contents(
            "tab_title_mode = static\n\
             tab_title_priority = shell, explicit, fallback\n\
             tab_title_fallback = Session\n\
             tab_title_explicit_prefix = termy:custom:\n\
             tab_title_shell_integration = false\n\
             tab_title_prompt_format = cwd:{cwd}\n\
             tab_title_command_format = run:{command}\n",
        );

        assert_eq!(config.tab_title.mode, TabTitleMode::Static);
        assert_eq!(
            config.tab_title.priority,
            vec![
                TabTitleSource::Shell,
                TabTitleSource::Explicit,
                TabTitleSource::Fallback
            ]
        );
        assert_eq!(config.tab_title.fallback, "Session");
        assert_eq!(config.tab_title.explicit_prefix, "termy:custom:");
        assert!(!config.tab_title.shell_integration);
        assert_eq!(config.tab_title.prompt_format, "cwd:{cwd}");
        assert_eq!(config.tab_title.command_format, "run:{command}");
    }

    #[test]
    fn runtime_env_options_parse() {
        let config = AppConfig::from_contents(
            "term = screen-256color\n\
             shell = /bin/zsh\n\
             working_dir_fallback = process\n\
             colorterm = none\n",
        );

        assert_eq!(config.term, "screen-256color");
        assert_eq!(config.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(config.working_dir_fallback, WorkingDirFallback::Process);
        assert!(config.colorterm.is_none());
    }

    #[test]
    fn keybind_lines_are_collected_in_order_with_line_numbers() {
        let config = AppConfig::from_contents(
            "# ignore comments\n\
             keybind = cmd-p=toggle_command_palette\n\
             keybind = cmd-c=copy\n\
             keybind = cmd-c=unbind\n\
             keybind = clear\n",
        );

        assert_eq!(config.keybind_lines.len(), 4);
        assert_eq!(config.keybind_lines[0].line_number, 2);
        assert_eq!(
            config.keybind_lines[0].value,
            "cmd-p=toggle_command_palette"
        );
        assert_eq!(config.keybind_lines[1].line_number, 3);
        assert_eq!(config.keybind_lines[1].value, "cmd-c=copy");
        assert_eq!(config.keybind_lines[2].line_number, 4);
        assert_eq!(config.keybind_lines[2].value, "cmd-c=unbind");
        assert_eq!(config.keybind_lines[3].line_number, 5);
        assert_eq!(config.keybind_lines[3].value, "clear");
    }

    #[test]
    fn command_palette_show_keybinds_parses_and_defaults() {
        let defaults = AppConfig::from_contents("");
        assert!(defaults.command_palette_show_keybinds);

        let disabled = AppConfig::from_contents("command_palette_show_keybinds = false\n");
        assert!(!disabled.command_palette_show_keybinds);
    }

    #[test]
    fn terminal_scrollbar_visibility_parses_and_defaults() {
        let defaults = AppConfig::from_contents("");
        assert_eq!(
            defaults.terminal_scrollbar_visibility,
            TerminalScrollbarVisibility::OnScroll
        );

        let off = AppConfig::from_contents("scrollbar_visibility = off\n");
        assert_eq!(
            off.terminal_scrollbar_visibility,
            TerminalScrollbarVisibility::Off
        );

        let always = AppConfig::from_contents("scrollbar_visibility = always\n");
        assert_eq!(
            always.terminal_scrollbar_visibility,
            TerminalScrollbarVisibility::Always
        );

        let strict = AppConfig::from_contents("terminal_scrollbar_visibility = always\n");
        assert_eq!(
            strict.terminal_scrollbar_visibility,
            TerminalScrollbarVisibility::OnScroll
        );

        let on_alias_removed = AppConfig::from_contents("scrollbar_visibility = on\n");
        assert_eq!(
            on_alias_removed.terminal_scrollbar_visibility,
            TerminalScrollbarVisibility::OnScroll
        );

        let invalid = AppConfig::from_contents("scrollbar_visibility = nope\n");
        assert_eq!(
            invalid.terminal_scrollbar_visibility,
            TerminalScrollbarVisibility::OnScroll
        );
    }

    #[test]
    fn terminal_scrollbar_style_parses_and_defaults() {
        let defaults = AppConfig::from_contents("");
        assert_eq!(
            defaults.terminal_scrollbar_style,
            TerminalScrollbarStyle::Neutral
        );

        let theme = AppConfig::from_contents("scrollbar_style = theme\n");
        assert_eq!(
            theme.terminal_scrollbar_style,
            TerminalScrollbarStyle::Theme
        );

        let muted_theme = AppConfig::from_contents("scrollbar_style = muted_theme\n");
        assert_eq!(
            muted_theme.terminal_scrollbar_style,
            TerminalScrollbarStyle::MutedTheme
        );

        let neutral = AppConfig::from_contents("scrollbar_style = neutral\n");
        assert_eq!(
            neutral.terminal_scrollbar_style,
            TerminalScrollbarStyle::Neutral
        );

        let strict = AppConfig::from_contents("terminal_scrollbar_style = theme\n");
        assert_eq!(
            strict.terminal_scrollbar_style,
            TerminalScrollbarStyle::Neutral
        );

        let invalid = AppConfig::from_contents("scrollbar_style = accent\n");
        assert_eq!(
            invalid.terminal_scrollbar_style,
            TerminalScrollbarStyle::Neutral
        );
    }

    #[test]
    fn mouse_scroll_multiplier_parses_and_clamps() {
        let defaults = AppConfig::from_contents("");
        assert_eq!(defaults.mouse_scroll_multiplier, 3.0);

        let custom = AppConfig::from_contents("mouse_scroll_multiplier = 2.5\n");
        assert_eq!(custom.mouse_scroll_multiplier, 2.5);

        let clamped_low = AppConfig::from_contents("mouse_scroll_multiplier = -1\n");
        assert_eq!(clamped_low.mouse_scroll_multiplier, 0.1);

        let clamped_high = AppConfig::from_contents("mouse_scroll_multiplier = 20000\n");
        assert_eq!(clamped_high.mouse_scroll_multiplier, 1_000.0);
    }

    #[test]
    fn background_opacity_and_blur_parse_and_default() {
        let defaults = AppConfig::from_contents("");
        assert_eq!(defaults.background_opacity, 1.0);
        assert!(!defaults.background_blur);

        let configured = AppConfig::from_contents(
            "background_opacity = 0.9\n\
             background_blur = true\n",
        );
        assert_eq!(configured.background_opacity, 0.9);
        assert!(configured.background_blur);

        let configured_numeric_true = AppConfig::from_contents("background_blur = 1\n");
        assert!(configured_numeric_true.background_blur);

        let configured_numeric_false = AppConfig::from_contents("background_blur = 0\n");
        assert!(!configured_numeric_false.background_blur);

        let clamped_low = AppConfig::from_contents("background_opacity = -0.5\n");
        assert_eq!(clamped_low.background_opacity, 0.0);

        let clamped_high = AppConfig::from_contents("background_opacity = 4.0\n");
        assert_eq!(clamped_high.background_opacity, 1.0);

        let old_key_ignored = AppConfig::from_contents("transparent_background_opacity = 0.2\n");
        assert_eq!(old_key_ignored.background_opacity, 1.0);
    }

    #[test]
    fn cursor_style_and_blink_parse_and_default() {
        let defaults = AppConfig::from_contents("");
        assert_eq!(defaults.cursor_style, CursorStyle::Block);
        assert!(defaults.cursor_blink);

        let line = AppConfig::from_contents("cursor_style = line\n");
        assert_eq!(line.cursor_style, CursorStyle::Line);

        let line_alias = AppConfig::from_contents("cursor_style = bar\n");
        assert_eq!(line_alias.cursor_style, CursorStyle::Line);

        let block = AppConfig::from_contents("cursor_style = block\n");
        assert_eq!(block.cursor_style, CursorStyle::Block);

        let blink_disabled = AppConfig::from_contents("cursor_blink = false\n");
        assert!(!blink_disabled.cursor_blink);
    }

    #[test]
    fn scrollback_history_parses_and_clamps() {
        let defaults = AppConfig::from_contents("");
        assert_eq!(defaults.scrollback_history, 2000);

        let custom = AppConfig::from_contents("scrollback_history = 5000\n");
        assert_eq!(custom.scrollback_history, 5000);

        let alias = AppConfig::from_contents("scrollback = 3000\n");
        assert_eq!(alias.scrollback_history, 3000);

        let clamped_high = AppConfig::from_contents("scrollback_history = 200000\n");
        assert_eq!(clamped_high.scrollback_history, 100_000);
    }

    #[test]
    fn max_tabs_parses_and_clamps() {
        let defaults = AppConfig::from_contents("");
        assert_eq!(defaults.max_tabs, 10);

        let custom = AppConfig::from_contents("max_tabs = 5\n");
        assert_eq!(custom.max_tabs, 5);

        let clamped_low = AppConfig::from_contents("max_tabs = 0\n");
        assert_eq!(clamped_low.max_tabs, 1);

        let clamped_high = AppConfig::from_contents("max_tabs = 500\n");
        assert_eq!(clamped_high.max_tabs, 100);
    }

    #[test]
    fn custom_colors_parse() {
        let config = AppConfig::from_contents(
            "theme = termy\n\
             \n\
             [colors]\n\
             foreground = #e7ebf5\n\
             background = #0b1020\n\
             cursor = #a7e9a3\n\
             black = #0b1020\n\
             red = #f1b8c5\n\
             color10 = #00ff00\n",
        );

        let fg = config.colors.foreground.unwrap();
        assert!((fg.r - 0.906).abs() < 0.01);
        assert!((fg.g - 0.922).abs() < 0.01);
        assert!((fg.b - 0.961).abs() < 0.01);

        let bg = config.colors.background.unwrap();
        assert!((bg.r - 0.043).abs() < 0.01);
        assert!((bg.g - 0.063).abs() < 0.01);
        assert!((bg.b - 0.125).abs() < 0.01);

        assert!(config.colors.cursor.is_some());
        assert!(config.colors.ansi[0].is_some());
        assert!(config.colors.ansi[1].is_some());
        assert!(config.colors.ansi[10].is_some());
        assert!(config.colors.ansi[2].is_none());
    }

    #[test]
    fn upsert_theme_assignment_replaces_existing_root_theme() {
        let input = "theme = termy\nfont_size = 14\n";
        let output = upsert_theme_assignment(input, "nord");
        assert_eq!(output, "theme = nord\nfont_size = 14\n");
    }

    #[test]
    fn upsert_theme_assignment_inserts_before_first_section_when_missing() {
        let input = "font_size = 14\n\n[colors]\nforeground = #ffffff\n";
        let output = upsert_theme_assignment(input, "tokyo-night");
        assert_eq!(
            output,
            "font_size = 14\n\ntheme = tokyo-night\n[colors]\nforeground = #ffffff\n"
        );
    }

    #[test]
    fn replace_or_insert_section_replaces_existing_section_body() {
        let input = "theme = termy\n[colors]\nforeground = #ffffff\nbackground = #000000\n";
        let output = replace_or_insert_section(
            input,
            "colors",
            &[
                "foreground = #111111".to_string(),
                "cursor = #222222".to_string(),
            ],
        );

        assert_eq!(
            output,
            "theme = termy\n[colors]\nforeground = #111111\ncursor = #222222\n"
        );
    }

    #[test]
    fn replace_or_insert_section_appends_missing_section() {
        let input = "theme = termy\nfont_size = 14\n";
        let output =
            replace_or_insert_section(input, "colors", &["foreground = #111111".to_string()]);

        assert_eq!(
            output,
            "theme = termy\nfont_size = 14\n\n[colors]\nforeground = #111111\n"
        );
    }
}
