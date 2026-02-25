use std::collections::HashMap;

use crate::color_keys::{ColorEntryError, apply_color_entry};
use crate::constants::{
    MAX_MOUSE_SCROLL_MULTIPLIER, MAX_SCROLLBACK_HISTORY, MIN_MOUSE_SCROLL_MULTIPLIER,
    SHELL_DECIDE_THEME_ID, VALID_SECTIONS,
};
use crate::diagnostics::{ConfigDiagnostic, ConfigDiagnosticKind, ConfigParseReport};
use crate::types::{
    AppConfig, CursorStyle, KeybindConfigLine, TabCloseVisibility, TabTitleMode, TabTitleSource,
    TabWidthMode, TerminalScrollbarStyle, TerminalScrollbarVisibility, ThemeId, WorkingDirFallback,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RootKey {
    Theme,
    WorkingDir,
    WorkingDirFallback,
    WarnOnQuitWithRunningProcess,
    TabTitlePriority,
    TabTitleMode,
    TabTitleFallback,
    TabTitleExplicitPrefix,
    TabTitleShellIntegration,
    TabTitlePromptFormat,
    TabTitleCommandFormat,
    TabCloseVisibility,
    TabWidthMode,
    ShowTermyInTitlebar,
    Shell,
    Term,
    Colorterm,
    WindowWidth,
    WindowHeight,
    FontFamily,
    FontSize,
    CursorStyle,
    CursorBlink,
    BackgroundOpacity,
    BackgroundBlur,
    PaddingX,
    PaddingY,
    MouseScrollMultiplier,
    ScrollbarVisibility,
    ScrollbarStyle,
    ScrollbackHistory,
    InactiveTabScrollback,
    CommandPaletteShowKeybinds,
    Keybind,
}

impl RootKey {
    fn is_repeatable(self) -> bool {
        matches!(self, Self::Keybind)
    }
}

impl AppConfig {
    pub fn from_contents(contents: &str) -> Self {
        Self::from_contents_with_report(contents).config
    }

    pub fn from_contents_with_report(contents: &str) -> ConfigParseReport {
        let mut config = Self::default();
        let mut diagnostics = Vec::new();
        let mut tab_title_priority_overridden = false;
        let mut in_colors_section = false;
        let mut seen_root_keys: HashMap<RootKey, usize> = HashMap::new();

        for (line_number, line) in contents.lines().enumerate() {
            let line_number = line_number + 1;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                in_colors_section = section == "colors";
                if !VALID_SECTIONS.iter().any(|valid| *valid == section) {
                    diagnostics.push(ConfigDiagnostic {
                        line_number,
                        kind: ConfigDiagnosticKind::UnknownSection,
                        message: format!("Unknown section '[{}]'", section),
                    });
                }
                continue;
            }

            let Some((raw_key, raw_value)) = line.split_once('=') else {
                diagnostics.push(ConfigDiagnostic {
                    line_number,
                    kind: ConfigDiagnosticKind::InvalidSyntax,
                    message: "Invalid syntax: expected 'key = value'".to_string(),
                });
                continue;
            };
            let key = raw_key.trim();
            let value = raw_value.trim();

            if key.is_empty() {
                diagnostics.push(ConfigDiagnostic {
                    line_number,
                    kind: ConfigDiagnosticKind::InvalidSyntax,
                    message: "Invalid syntax: missing key before '='".to_string(),
                });
                continue;
            }

            if in_colors_section {
                match apply_color_entry(&mut config.colors, key, value) {
                    Ok(()) => {}
                    Err(ColorEntryError::UnknownKey) => diagnostics.push(ConfigDiagnostic {
                        line_number,
                        kind: ConfigDiagnosticKind::UnknownColorKey,
                        message: format!("Unknown color key '{}'", key),
                    }),
                    Err(ColorEntryError::InvalidValue) => diagnostics.push(ConfigDiagnostic {
                        line_number,
                        kind: ConfigDiagnosticKind::InvalidValue,
                        message: format!(
                            "Invalid color value '{}' for '{}': expected #RRGGBB",
                            value, key
                        ),
                    }),
                }
                continue;
            }

            let Some(root_key) = parse_root_key(key) else {
                diagnostics.push(ConfigDiagnostic {
                    line_number,
                    kind: ConfigDiagnosticKind::UnknownRootKey,
                    message: format!("Unknown root key '{}'", key),
                });
                continue;
            };

            if !root_key.is_repeatable() {
                if let Some(first_line) = seen_root_keys.get(&root_key).copied() {
                    diagnostics.push(ConfigDiagnostic {
                        line_number,
                        kind: ConfigDiagnosticKind::DuplicateRootKey,
                        message: format!(
                            "Duplicate root key '{}' (first defined on line {})",
                            key, first_line
                        ),
                    });
                } else {
                    seen_root_keys.insert(root_key, line_number);
                }
            }

            match root_key {
                RootKey::Theme => {
                    if let Some(theme) = parse_theme_id(value) {
                        config.theme = theme;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "a non-empty theme id",
                        );
                    }
                }
                RootKey::WorkingDir => {
                    if value.is_empty() {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "a non-empty path value",
                        );
                    } else {
                        config.working_dir = Some(value.to_string());
                    }
                }
                RootKey::WorkingDirFallback => {
                    if let Some(fallback) = WorkingDirFallback::from_str(value) {
                        config.working_dir_fallback = fallback;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "one of: home, process",
                        );
                    }
                }
                RootKey::WarnOnQuitWithRunningProcess => {
                    if let Some(parsed) =
                        parse_bool_field(&mut diagnostics, line_number, key, value)
                    {
                        config.warn_on_quit_with_running_process = parsed;
                    }
                }
                RootKey::TabTitlePriority => {
                    if let Some(priority) = parse_tab_title_priority(value) {
                        config.tab_title.priority = priority;
                        tab_title_priority_overridden = true;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "a comma-separated list of tab title sources",
                        );
                    }
                }
                RootKey::TabTitleMode => {
                    if let Some(mode) = TabTitleMode::from_str(value) {
                        config.tab_title.mode = mode;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "one of: smart, shell, explicit, static",
                        );
                    }
                }
                RootKey::TabTitleFallback => {
                    if let Some(parsed) = parse_string_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a non-empty string",
                    ) {
                        config.tab_title.fallback = parsed;
                    }
                }
                RootKey::TabTitleExplicitPrefix => {
                    if let Some(parsed) = parse_string_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a non-empty string",
                    ) {
                        config.tab_title.explicit_prefix = parsed;
                    }
                }
                RootKey::TabTitleShellIntegration => {
                    if let Some(parsed) =
                        parse_bool_field(&mut diagnostics, line_number, key, value)
                    {
                        config.tab_title.shell_integration = parsed;
                    }
                }
                RootKey::TabTitlePromptFormat => {
                    if let Some(parsed) = parse_string_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a non-empty string",
                    ) {
                        config.tab_title.prompt_format = parsed;
                    }
                }
                RootKey::TabTitleCommandFormat => {
                    if let Some(parsed) = parse_string_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a non-empty string",
                    ) {
                        config.tab_title.command_format = parsed;
                    }
                }
                RootKey::TabCloseVisibility => {
                    if let Some(parsed) = TabCloseVisibility::from_str(value) {
                        config.tab_close_visibility = parsed;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "one of: active_hover, hover, always",
                        );
                    }
                }
                RootKey::TabWidthMode => {
                    if let Some(parsed) = TabWidthMode::from_str(value) {
                        config.tab_width_mode = parsed;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "one of: stable, active_grow, active_grow_sticky",
                        );
                    }
                }
                RootKey::ShowTermyInTitlebar => {
                    if let Some(parsed) =
                        parse_bool_field(&mut diagnostics, line_number, key, value)
                    {
                        config.show_termy_in_titlebar = parsed;
                    }
                }
                RootKey::Shell => {
                    config.shell = parse_optional_string_value(value);
                }
                RootKey::Term => {
                    if let Some(parsed) = parse_string_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a non-empty string",
                    ) {
                        config.term = parsed;
                    }
                }
                RootKey::Colorterm => {
                    config.colorterm = parse_optional_string_value(value);
                }
                RootKey::WindowWidth => {
                    if let Some(parsed) =
                        parse_positive_f32_field(&mut diagnostics, line_number, key, value)
                    {
                        config.window_width = parsed;
                    }
                }
                RootKey::WindowHeight => {
                    if let Some(parsed) =
                        parse_positive_f32_field(&mut diagnostics, line_number, key, value)
                    {
                        config.window_height = parsed;
                    }
                }
                RootKey::FontFamily => {
                    if let Some(parsed) = parse_string_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a non-empty string",
                    ) {
                        config.font_family = parsed;
                    }
                }
                RootKey::FontSize => {
                    if let Some(parsed) =
                        parse_positive_f32_field(&mut diagnostics, line_number, key, value)
                    {
                        config.font_size = parsed;
                    }
                }
                RootKey::CursorStyle => {
                    if let Some(parsed) = CursorStyle::from_str(value) {
                        config.cursor_style = parsed;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "one of: line, block",
                        );
                    }
                }
                RootKey::CursorBlink => {
                    if let Some(parsed) =
                        parse_bool_field(&mut diagnostics, line_number, key, value)
                    {
                        config.cursor_blink = parsed;
                    }
                }
                RootKey::BackgroundOpacity => {
                    if let Some(parsed) = parse_f32_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a number between 0.0 and 1.0",
                    ) {
                        config.background_opacity = parsed.clamp(0.0, 1.0);
                    }
                }
                RootKey::BackgroundBlur => {
                    if let Some(parsed) =
                        parse_bool_field(&mut diagnostics, line_number, key, value)
                    {
                        config.background_blur = parsed;
                    }
                }
                RootKey::PaddingX => {
                    if let Some(parsed) =
                        parse_non_negative_f32_field(&mut diagnostics, line_number, key, value)
                    {
                        config.padding_x = parsed;
                    }
                }
                RootKey::PaddingY => {
                    if let Some(parsed) =
                        parse_non_negative_f32_field(&mut diagnostics, line_number, key, value)
                    {
                        config.padding_y = parsed;
                    }
                }
                RootKey::MouseScrollMultiplier => {
                    if let Some(parsed) = parse_finite_f32_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a finite number",
                    ) {
                        config.mouse_scroll_multiplier =
                            parsed.clamp(MIN_MOUSE_SCROLL_MULTIPLIER, MAX_MOUSE_SCROLL_MULTIPLIER);
                    }
                }
                RootKey::ScrollbarVisibility => {
                    if let Some(parsed) = TerminalScrollbarVisibility::from_str(value) {
                        config.terminal_scrollbar_visibility = parsed;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "one of: off, always, on_scroll",
                        );
                    }
                }
                RootKey::ScrollbarStyle => {
                    if let Some(parsed) = TerminalScrollbarStyle::from_str(value) {
                        config.terminal_scrollbar_style = parsed;
                    } else {
                        push_invalid_value(
                            &mut diagnostics,
                            line_number,
                            key,
                            value,
                            "one of: neutral, muted_theme, theme",
                        );
                    }
                }
                RootKey::ScrollbackHistory => {
                    if let Some(parsed) =
                        parse_usize_field(&mut diagnostics, line_number, key, value)
                    {
                        config.scrollback_history = parsed.min(MAX_SCROLLBACK_HISTORY);
                    }
                }
                RootKey::InactiveTabScrollback => {
                    if let Some(parsed) =
                        parse_usize_field(&mut diagnostics, line_number, key, value)
                    {
                        config.inactive_tab_scrollback = Some(parsed.min(MAX_SCROLLBACK_HISTORY));
                    }
                }
                RootKey::CommandPaletteShowKeybinds => {
                    if let Some(parsed) =
                        parse_bool_field(&mut diagnostics, line_number, key, value)
                    {
                        config.command_palette_show_keybinds = parsed;
                    }
                }
                RootKey::Keybind => {
                    if let Some(parsed) = parse_string_field(
                        &mut diagnostics,
                        line_number,
                        key,
                        value,
                        "a keybind directive",
                    ) {
                        config.keybind_lines.push(KeybindConfigLine {
                            line_number,
                            value: parsed,
                        });
                    }
                }
            }
        }

        if !tab_title_priority_overridden {
            config.tab_title.priority = config.tab_title.mode.default_priority();
        }

        ConfigParseReport::new(config, diagnostics)
    }
}

pub fn parse_theme_id(value: &str) -> Option<ThemeId> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(canonical) = termy_themes::canonical_builtin_theme_id(value) {
        return Some(canonical.to_string());
    }

    let normalized = termy_themes::normalize_theme_id(value);
    if matches!(
        normalized.as_str(),
        "shell" | "shell-decide" | "let-shell-decide"
    ) {
        return Some(SHELL_DECIDE_THEME_ID.to_string());
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn parse_root_key(raw: &str) -> Option<RootKey> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "theme" => Some(RootKey::Theme),
        "working_dir" => Some(RootKey::WorkingDir),
        "working_dir_fallback" | "default_working_dir" => Some(RootKey::WorkingDirFallback),
        "warn_on_quit_with_running_process" => Some(RootKey::WarnOnQuitWithRunningProcess),
        "tab_title_priority" => Some(RootKey::TabTitlePriority),
        "tab_title_mode" => Some(RootKey::TabTitleMode),
        "tab_title_fallback" => Some(RootKey::TabTitleFallback),
        "tab_title_explicit_prefix" => Some(RootKey::TabTitleExplicitPrefix),
        "tab_title_shell_integration" => Some(RootKey::TabTitleShellIntegration),
        "tab_title_prompt_format" => Some(RootKey::TabTitlePromptFormat),
        "tab_title_command_format" => Some(RootKey::TabTitleCommandFormat),
        "tab_close_visibility" => Some(RootKey::TabCloseVisibility),
        "tab_width_mode" => Some(RootKey::TabWidthMode),
        "show_termy_in_titlebar" => Some(RootKey::ShowTermyInTitlebar),
        "shell" => Some(RootKey::Shell),
        "term" => Some(RootKey::Term),
        "colorterm" => Some(RootKey::Colorterm),
        "window_width" => Some(RootKey::WindowWidth),
        "window_height" => Some(RootKey::WindowHeight),
        "font_family" => Some(RootKey::FontFamily),
        "font_size" => Some(RootKey::FontSize),
        "cursor_style" => Some(RootKey::CursorStyle),
        "cursor_blink" => Some(RootKey::CursorBlink),
        "background_opacity" => Some(RootKey::BackgroundOpacity),
        "background_blur" => Some(RootKey::BackgroundBlur),
        "padding_x" => Some(RootKey::PaddingX),
        "padding_y" => Some(RootKey::PaddingY),
        "mouse_scroll_multiplier" => Some(RootKey::MouseScrollMultiplier),
        "scrollbar_visibility" => Some(RootKey::ScrollbarVisibility),
        "scrollbar_style" => Some(RootKey::ScrollbarStyle),
        "scrollback_history" | "scrollback" => Some(RootKey::ScrollbackHistory),
        "inactive_tab_scrollback" => Some(RootKey::InactiveTabScrollback),
        "command_palette_show_keybinds" => Some(RootKey::CommandPaletteShowKeybinds),
        "keybind" => Some(RootKey::Keybind),
        _ => None,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_bool_field(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
) -> Option<bool> {
    parse_bool(value).or_else(|| {
        push_invalid_value(diagnostics, line_number, key, value, "a boolean value");
        None
    })
}

fn parse_f32_field(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
    expected: &str,
) -> Option<f32> {
    value.parse::<f32>().ok().or_else(|| {
        push_invalid_value(diagnostics, line_number, key, value, expected);
        None
    })
}

fn parse_positive_f32_field(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
) -> Option<f32> {
    let parsed = parse_f32_field(diagnostics, line_number, key, value, "a positive number")?;
    if parsed > 0.0 {
        Some(parsed)
    } else {
        push_invalid_value(diagnostics, line_number, key, value, "a positive number");
        None
    }
}

fn parse_non_negative_f32_field(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
) -> Option<f32> {
    let parsed = parse_f32_field(diagnostics, line_number, key, value, "a number >= 0")?;
    if parsed >= 0.0 {
        Some(parsed)
    } else {
        push_invalid_value(diagnostics, line_number, key, value, "a number >= 0");
        None
    }
}

fn parse_finite_f32_field(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
    expected: &str,
) -> Option<f32> {
    let parsed = parse_f32_field(diagnostics, line_number, key, value, expected)?;
    if parsed.is_finite() {
        Some(parsed)
    } else {
        push_invalid_value(diagnostics, line_number, key, value, expected);
        None
    }
}

fn parse_usize_field(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
) -> Option<usize> {
    value.parse::<usize>().ok().or_else(|| {
        push_invalid_value(diagnostics, line_number, key, value, "a positive integer");
        None
    })
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

fn parse_string_field(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
    expected: &str,
) -> Option<String> {
    parse_string_value(value).or_else(|| {
        push_invalid_value(diagnostics, line_number, key, value, expected);
        None
    })
}

fn parse_optional_string_value(value: &str) -> Option<String> {
    let parsed = parse_string_value(value)?;
    let normalized = parsed.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "none" | "unset" | "default" | "auto") {
        return None;
    }
    Some(parsed)
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

fn push_invalid_value(
    diagnostics: &mut Vec<ConfigDiagnostic>,
    line_number: usize,
    key: &str,
    value: &str,
    expected: &str,
) {
    diagnostics.push(ConfigDiagnostic {
        line_number,
        kind: ConfigDiagnosticKind::InvalidValue,
        message: format!(
            "Invalid value '{}' for '{}': expected {}",
            value, key, expected
        ),
    });
}
