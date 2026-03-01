use crate::{
    AppConfig, ConfigDiagnosticKind, ConfigParseReport, CursorStyle, Rgb8, TabCloseVisibility,
    TabTitleMode, TabTitleSource, TabWidthMode, TerminalScrollbarStyle,
    TerminalScrollbarVisibility, WorkingDirFallback,
};

fn parse(input: &str) -> AppConfig {
    AppConfig::from_contents(input)
}

fn parse_report(input: &str) -> ConfigParseReport {
    AppConfig::from_contents_with_report(input)
}

#[test]
fn from_contents_with_report_captures_diagnostics() {
    let report = parse_report(
        "bad syntax line\n\
         typo_key = true\n\
         [colors]\n\
         unknown = #ffffff\n\
         foreground = #nothex\n\
         [tab_title]\n\
         font_size = nope\n\
         font_size = 12\n",
    );

    assert_eq!(report.config.font_size, AppConfig::default().font_size);
    assert_eq!(report.diagnostics.len(), 4);
    assert_eq!(
        report.diagnostics[0].kind,
        ConfigDiagnosticKind::InvalidSyntax
    );
    assert_eq!(
        report.diagnostics[1].kind,
        ConfigDiagnosticKind::UnknownRootKey
    );
    assert_eq!(
        report.diagnostics[2].kind,
        ConfigDiagnosticKind::UnknownColorKey
    );
    assert_eq!(
        report.diagnostics[3].kind,
        ConfigDiagnosticKind::InvalidValue
    );
}

#[test]
fn non_color_sections_do_not_mutate_root_keys() {
    let defaults = AppConfig::default();
    let report = parse_report(
        "[tab_title]\n\
         font_size = 18\n\
         cursor_blink = false\n\
         [unknown]\n\
         font_size = 20\n",
    );

    assert_eq!(report.config.font_size, defaults.font_size);
    assert_eq!(report.config.cursor_blink, defaults.cursor_blink);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].kind,
        ConfigDiagnosticKind::UnknownSection
    );
}

#[test]
fn duplicate_root_key_diagnostics_use_canonical_key_groups() {
    let report = parse_report(
        "working_dir_fallback = home\n\
         default_working_dir = process\n",
    );
    assert_eq!(
        report.config.working_dir_fallback,
        WorkingDirFallback::Process
    );
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].kind,
        ConfigDiagnosticKind::DuplicateRootKey
    );
}

#[test]
fn optional_fields_accept_none_sentinel() {
    let report = parse_report(
        "working_dir = none\n\
         inactive_tab_scrollback = none\n",
    );
    assert!(report.diagnostics.is_empty());
    assert_eq!(report.config.working_dir, None);
    assert_eq!(report.config.inactive_tab_scrollback, None);
}

#[test]
fn tab_title_mode_sets_default_priority() {
    let config = parse(
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
    let config = parse(
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
fn enum_keys_parse_table_driven() {
    let tab_close_cases = [
        ("hover", TabCloseVisibility::Hover),
        ("activehover", TabCloseVisibility::ActiveHover),
        ("always", TabCloseVisibility::Always),
    ];
    for (input, expected) in tab_close_cases {
        let config = parse(&format!("tab_close_visibility = {}\n", input));
        assert_eq!(config.tab_close_visibility, expected);
    }
    assert_eq!(
        parse("tab_close_visibility = invalid\n").tab_close_visibility,
        TabCloseVisibility::ActiveHover
    );

    let tab_width_cases = [
        ("stable", TabWidthMode::Stable),
        ("activegrow", TabWidthMode::ActiveGrow),
        ("active_grow_sticky", TabWidthMode::ActiveGrowSticky),
    ];
    for (input, expected) in tab_width_cases {
        let config = parse(&format!("tab_width_mode = {}\n", input));
        assert_eq!(config.tab_width_mode, expected);
    }
    assert_eq!(
        parse("tab_width_mode = invalid\n").tab_width_mode,
        TabWidthMode::ActiveGrowSticky
    );

    let cursor_style_cases = [("line", CursorStyle::Line), ("bar", CursorStyle::Line)];
    for (input, expected) in cursor_style_cases {
        let config = parse(&format!("cursor_style = {}\n", input));
        assert_eq!(config.cursor_style, expected);
    }
    assert_eq!(
        parse("cursor_style = block\n").cursor_style,
        CursorStyle::Block
    );

    let scrollbar_visibility_cases = [
        ("off", TerminalScrollbarVisibility::Off),
        ("always", TerminalScrollbarVisibility::Always),
        ("on_scroll", TerminalScrollbarVisibility::OnScroll),
    ];
    for (input, expected) in scrollbar_visibility_cases {
        let config = parse(&format!("scrollbar_visibility = {}\n", input));
        assert_eq!(config.terminal_scrollbar_visibility, expected);
    }
    assert_eq!(
        parse("scrollbar_visibility = nope\n").terminal_scrollbar_visibility,
        TerminalScrollbarVisibility::OnScroll
    );
    // `scrollbar_visibility` is the supported key; the legacy
    // `terminal_scrollbar_visibility` key should be ignored.
    assert_eq!(
        parse("terminal_scrollbar_visibility = always\n").terminal_scrollbar_visibility,
        TerminalScrollbarVisibility::OnScroll
    );

    let scrollbar_style_cases = [
        ("neutral", TerminalScrollbarStyle::Neutral),
        ("muted_theme", TerminalScrollbarStyle::MutedTheme),
        ("theme", TerminalScrollbarStyle::Theme),
    ];
    for (input, expected) in scrollbar_style_cases {
        let config = parse(&format!("scrollbar_style = {}\n", input));
        assert_eq!(config.terminal_scrollbar_style, expected);
    }
    assert_eq!(
        parse("terminal_scrollbar_style = theme\n").terminal_scrollbar_style,
        TerminalScrollbarStyle::Neutral
    );

    let fallback_cases = [
        ("home", WorkingDirFallback::Home),
        ("process", WorkingDirFallback::Process),
    ];
    for (input, expected) in fallback_cases {
        let config = parse(&format!("working_dir_fallback = {}\n", input));
        assert_eq!(config.working_dir_fallback, expected);
    }
    assert_eq!(
        parse("working_dir_fallback = invalid\n").working_dir_fallback,
        WorkingDirFallback::Home
    );
}

#[derive(Clone, Copy)]
enum BoolField {
    ShowTermyInTitlebar,
    CursorBlink,
    BackgroundBlur,
    WarnOnQuit,
    CommandPaletteShowKeybinds,
    TabTitleShellIntegration,
}

impl BoolField {
    fn key(self) -> &'static str {
        match self {
            Self::ShowTermyInTitlebar => "show_termy_in_titlebar",
            Self::CursorBlink => "cursor_blink",
            Self::BackgroundBlur => "background_blur",
            Self::WarnOnQuit => "warn_on_quit_with_running_process",
            Self::CommandPaletteShowKeybinds => "command_palette_show_keybinds",
            Self::TabTitleShellIntegration => "tab_title_shell_integration",
        }
    }

    fn read(self, config: &AppConfig) -> bool {
        match self {
            Self::ShowTermyInTitlebar => config.show_termy_in_titlebar,
            Self::CursorBlink => config.cursor_blink,
            Self::BackgroundBlur => config.background_blur,
            Self::WarnOnQuit => config.warn_on_quit_with_running_process,
            Self::CommandPaletteShowKeybinds => config.command_palette_show_keybinds,
            Self::TabTitleShellIntegration => config.tab_title.shell_integration,
        }
    }
}

#[test]
fn bool_keys_parse_table_driven() {
    let defaults = AppConfig::default();
    let fields = [
        BoolField::ShowTermyInTitlebar,
        BoolField::CursorBlink,
        BoolField::BackgroundBlur,
        BoolField::WarnOnQuit,
        BoolField::CommandPaletteShowKeybinds,
        BoolField::TabTitleShellIntegration,
    ];

    for field in fields {
        let key = field.key();

        let enabled = parse(&format!("{} = true\n", key));
        assert!(field.read(&enabled));

        let enabled_numeric = parse(&format!("{} = 1\n", key));
        assert!(field.read(&enabled_numeric));

        let disabled = parse(&format!("{} = false\n", key));
        assert!(!field.read(&disabled));

        let disabled_numeric = parse(&format!("{} = 0\n", key));
        assert!(!field.read(&disabled_numeric));

        let invalid = parse(&format!("{} = maybe\n", key));
        assert_eq!(field.read(&invalid), field.read(&defaults));
    }
}

#[test]
fn numeric_keys_parse_table_driven() {
    let defaults = parse("");

    let positive_float_cases = [
        ("window_width", 1100.0, defaults.window_width),
        ("window_height", 700.0, defaults.window_height),
        ("font_size", 16.0, defaults.font_size),
    ];
    for (key, expected, fallback) in positive_float_cases {
        let valid = parse(&format!("{} = {}\n", key, expected));
        let parsed = match key {
            "window_width" => valid.window_width,
            "window_height" => valid.window_height,
            "font_size" => valid.font_size,
            _ => unreachable!(),
        };
        assert_eq!(parsed, expected);

        let invalid = parse(&format!("{} = -1\n", key));
        let parsed = match key {
            "window_width" => invalid.window_width,
            "window_height" => invalid.window_height,
            "font_size" => invalid.font_size,
            _ => unreachable!(),
        };
        assert_eq!(parsed, fallback);
    }

    let non_negative_float_cases = [("padding_x", 2.0), ("padding_y", 4.0)];
    for (key, expected) in non_negative_float_cases {
        let valid = parse(&format!("{} = {}\n", key, expected));
        let parsed = match key {
            "padding_x" => valid.padding_x,
            "padding_y" => valid.padding_y,
            _ => unreachable!(),
        };
        assert_eq!(parsed, expected);

        let invalid = parse(&format!("{} = -1\n", key));
        let parsed = match key {
            "padding_x" => invalid.padding_x,
            "padding_y" => invalid.padding_y,
            _ => unreachable!(),
        };
        let default_value = match key {
            "padding_x" => defaults.padding_x,
            "padding_y" => defaults.padding_y,
            _ => unreachable!(),
        };
        assert_eq!(parsed, default_value);
    }

    assert_eq!(parse("background_opacity = -0.5\n").background_opacity, 0.0);
    assert_eq!(parse("background_opacity = 4\n").background_opacity, 1.0);
    let nan_opacity = parse_report("background_opacity = NaN\n");
    assert_eq!(
        nan_opacity.config.background_opacity,
        defaults.background_opacity
    );
    assert!(
        nan_opacity
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == ConfigDiagnosticKind::InvalidValue)
    );
    // The removed/unsupported alias should be ignored, so the default remains.
    assert_eq!(
        parse("transparent_background_opacity = 0.2\n").background_opacity,
        defaults.background_opacity
    );

    assert_eq!(
        parse("mouse_scroll_multiplier = -1\n").mouse_scroll_multiplier,
        0.1
    );
    assert_eq!(
        parse("mouse_scroll_multiplier = 20000\n").mouse_scroll_multiplier,
        1_000.0
    );

    assert_eq!(
        parse("scrollback_history = 5000\n").scrollback_history,
        5000
    );
    assert_eq!(parse("scrollback = 3000\n").scrollback_history, 3000);
    assert_eq!(
        parse("scrollback_history = 200000\n").scrollback_history,
        100_000
    );

    assert_eq!(parse("font_size = inf\n").font_size, defaults.font_size);
    assert_eq!(parse("padding_x = NaN\n").padding_x, defaults.padding_x);
}

#[test]
fn runtime_env_options_parse() {
    let config = parse(
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
fn tmux_runtime_options_parse() {
    let config = parse(
        "tmux_enabled = true\n\
         tmux_persistence = true\n\
         tmux_binary = /opt/homebrew/bin/tmux\n\
         working_dir_fallback = process\n",
    );

    assert!(config.tmux_enabled);
    assert!(config.tmux_persistence);
    assert_eq!(config.tmux_binary, "/opt/homebrew/bin/tmux");
    assert_eq!(config.working_dir_fallback, WorkingDirFallback::Process);
}

#[test]
fn removed_tmux_session_name_key_produces_unknown_root_key_diagnostic() {
    let report = parse_report("tmux_session_name = work\n");
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].kind, ConfigDiagnosticKind::UnknownRootKey);
}

#[test]
fn keybind_lines_are_collected_in_order_with_line_numbers() {
    let config = parse(
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
fn removed_hide_titlebar_buttons_key_is_ignored_as_unknown() {
    let configured = parse(
        "hide_titlebar_buttons = true\n\
         warn_on_quit_with_running_process = false\n",
    );
    assert!(!configured.warn_on_quit_with_running_process);
}

#[test]
fn custom_colors_parse() {
    let config = parse(
        "theme = termy\n\
         \n\
         [colors]\n\
         foreground = #e7ebf5\n\
         background = #0b1020\n\
         cursor = #a7e9a3\n\
         black = #0b1020\n\
         red = #f1b8c5\n\
         color10 = #00ff00\n\
         brightblack = #101010\n",
    );

    let fg = config.colors.foreground.expect("foreground color");
    assert_eq!(
        fg,
        Rgb8 {
            r: 0xe7,
            g: 0xeb,
            b: 0xf5
        }
    );

    let bg = config.colors.background.expect("background color");
    assert_eq!(
        bg,
        Rgb8 {
            r: 0x0b,
            g: 0x10,
            b: 0x20
        }
    );

    assert!(config.colors.cursor.is_some());
    assert!(config.colors.ansi[0].is_some());
    assert!(config.colors.ansi[1].is_some());
    assert!(config.colors.ansi[10].is_some());
    assert!(config.colors.ansi[8].is_some());
    assert!(config.colors.ansi[2].is_none());
}

#[test]
fn shell_decide_theme_aliases_canonicalize() {
    let config = parse("theme = shell\n");
    assert_eq!(config.theme, "shell-decide");

    let config = parse("theme = let shell decide\n");
    assert_eq!(config.theme, "shell-decide");
}

#[test]
fn builtin_theme_aliases_canonicalize() {
    let config = parse("theme = gruvbox\n");
    assert_eq!(config.theme, "gruvbox-dark");

    let config = parse("theme = tokyonight\n");
    assert_eq!(config.theme, "tokyo-night");
}
