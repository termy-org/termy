use crate::types::{
    AiProvider, AppConfig, CursorStyle, PaneFocusEffect, TabCloseVisibility, TabTitleMode,
    TabWidthMode, TerminalScrollbarStyle, TerminalScrollbarVisibility, WorkingDirFallback,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    Appearance,
    Terminal,
    Tabs,
    Advanced,
    Colors,
    Keybindings,
}

impl SettingsSection {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Terminal => "Terminal",
            Self::Tabs => "Tabs",
            Self::Advanced => "Advanced",
            Self::Colors => "Colors",
            Self::Keybindings => "Keybindings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootSettingSpec {
    pub id: RootSettingId,
    pub key: &'static str,
    pub aliases: &'static [&'static str],
    pub section: SettingsSection,
    pub group: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub keywords: &'static [&'static str],
    pub value_kind: RootSettingValueKind,
    pub repeatable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSettingSpec {
    pub id: ColorSettingId,
    pub key: &'static str,
    pub aliases: &'static [&'static str],
    pub title: &'static str,
    pub description: &'static str,
    pub keywords: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSettingValueKind {
    Text,
    Numeric,
    Boolean,
    Enum,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumChoice {
    pub value: &'static str,
    pub label: &'static str,
}

fn normalize_key(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('-', "_")
}

macro_rules! define_root_settings {
    ($((
        $id:ident,
        $key:literal,
        [$($alias:literal),* $(,)?],
        $section:ident,
        $group:literal,
        $title:literal,
        $description:literal,
        [$($keyword:literal),* $(,)?],
        $value_kind:expr,
        $repeatable:expr
    )),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum RootSettingId {
            $($id,)+
        }

        pub const ROOT_SETTING_SPECS: &[RootSettingSpec] = &[
            $(RootSettingSpec {
                id: RootSettingId::$id,
                key: $key,
                aliases: &[$($alias),*],
                section: SettingsSection::$section,
                group: $group,
                title: $title,
                description: $description,
                keywords: &[$($keyword),*],
                value_kind: $value_kind,
                repeatable: $repeatable,
            },)+
        ];

        pub const ROOT_SETTING_KEYS: &[&str] = &[
            $($key,)+
        ];

        pub const ROOT_SETTING_ALL_KEYS: &[&str] = &[
            $($key, $($alias,)* )+
        ];

        pub fn root_setting_specs() -> &'static [RootSettingSpec] {
            ROOT_SETTING_SPECS
        }

        pub fn root_setting_spec(id: RootSettingId) -> &'static RootSettingSpec {
            &ROOT_SETTING_SPECS[id as usize]
        }

        pub fn root_setting_from_key(raw: &str) -> Option<RootSettingId> {
            let normalized = normalize_key(raw);
            match normalized.as_str() {
                $($key $(| $alias)* => Some(RootSettingId::$id),)+
                _ => None,
            }
        }
    };
}

macro_rules! define_color_settings {
    ($((
        $id:ident,
        $key:literal,
        [$($alias:literal),* $(,)?],
        $title:literal,
        $description:literal,
        [$($keyword:literal),* $(,)?]
    )),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ColorSettingId {
            $($id,)+
        }

        pub const COLOR_SETTING_SPECS: &[ColorSettingSpec] = &[
            $(ColorSettingSpec {
                id: ColorSettingId::$id,
                key: $key,
                aliases: &[$($alias),*],
                title: $title,
                description: $description,
                keywords: &[$($keyword),*],
            },)+
        ];

        pub const COLOR_SETTING_KEYS: &[&str] = &[
            $($key,)+
        ];

        pub fn color_setting_specs() -> &'static [ColorSettingSpec] {
            COLOR_SETTING_SPECS
        }

        pub fn color_setting_spec(id: ColorSettingId) -> &'static ColorSettingSpec {
            &COLOR_SETTING_SPECS[id as usize]
        }

        pub fn color_setting_from_key(raw: &str) -> Option<ColorSettingId> {
            let normalized = normalize_key(raw);
            match normalized.as_str() {
                $($key $(| $alias)* => Some(ColorSettingId::$id),)+
                _ => None,
            }
        }
    };
}

pub const CURSOR_STYLE_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "block",
        label: "Block",
    },
    EnumChoice {
        value: "line",
        label: "Line",
    },
];

pub const TAB_TITLE_MODE_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "smart",
        label: "Smart",
    },
    EnumChoice {
        value: "shell",
        label: "Shell",
    },
    EnumChoice {
        value: "explicit",
        label: "Explicit",
    },
    EnumChoice {
        value: "static",
        label: "Static",
    },
];

pub const SCROLLBAR_VISIBILITY_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "off",
        label: "Off",
    },
    EnumChoice {
        value: "always",
        label: "Always",
    },
    EnumChoice {
        value: "on_scroll",
        label: "On Scroll",
    },
];

pub const SCROLLBAR_STYLE_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "neutral",
        label: "Neutral",
    },
    EnumChoice {
        value: "muted_theme",
        label: "Muted Theme",
    },
    EnumChoice {
        value: "theme",
        label: "Theme",
    },
];

pub const TAB_CLOSE_VISIBILITY_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "active_hover",
        label: "Active + Hover",
    },
    EnumChoice {
        value: "hover",
        label: "Hover",
    },
    EnumChoice {
        value: "always",
        label: "Always",
    },
];

pub const TAB_WIDTH_MODE_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "stable",
        label: "Stable",
    },
    EnumChoice {
        value: "active_grow",
        label: "Active Grow",
    },
    EnumChoice {
        value: "active_grow_sticky",
        label: "Active Grow Sticky",
    },
];

pub const WORKING_DIR_FALLBACK_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "home",
        label: "Home",
    },
    EnumChoice {
        value: "process",
        label: "Process",
    },
];

pub const PANE_FOCUS_EFFECT_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "off",
        label: "Off",
    },
    EnumChoice {
        value: "soft_spotlight",
        label: "Soft Spotlight",
    },
    EnumChoice {
        value: "cinematic",
        label: "Cinematic",
    },
    EnumChoice {
        value: "minimal",
        label: "Minimal",
    },
];

pub const AI_PROVIDER_ENUM_CHOICES: &[EnumChoice] = &[
    EnumChoice {
        value: "openai",
        label: "OpenAI",
    },
    EnumChoice {
        value: "gemini",
        label: "Gemini",
    },
];

define_root_settings! {
    (Theme, "theme", [], Appearance, "THEME", "Theme", "Current color scheme name", ["color", "scheme", "appearance"], RootSettingValueKind::Special, false),
    (AutoUpdate, "auto_update", [], Advanced, "UPDATES", "Auto Update", "Enable automatic update checks and notifications", ["update", "check", "upgrade", "version"], RootSettingValueKind::Boolean, false),
    (TmuxEnabled, "tmux_enabled", [], Terminal, "TMUX", "Tmux Enabled", "Enable tmux runtime integration", ["tmux", "runtime", "integration", "enabled"], RootSettingValueKind::Boolean, false),
    (TmuxPersistence, "tmux_persistence", [], Terminal, "TMUX", "Tmux Persistence", "Reuse tmux tabs and panes across app restarts", ["tmux", "session", "persistence", "restart"], RootSettingValueKind::Boolean, false),
    (NativeTabPersistence, "native_tab_persistence", [], Advanced, "STARTUP", "Native Tab Persistence", "Restore native tabs and pane splits across app restarts", ["native", "tabs", "panes", "split", "restore", "startup"], RootSettingValueKind::Boolean, false),
    (NativeLayoutAutosave, "native_layout_autosave", [], Advanced, "STARTUP", "Native Layout Autosave", "Auto-save changes back into the currently loaded named layout", ["native", "layout", "autosave", "saved", "snapshot"], RootSettingValueKind::Boolean, false),
    (NativeBufferPersistence, "native_buffer_persistence", [], Advanced, "STARTUP", "Native Buffer Persistence", "Replay saved buffer text when restoring native layouts", ["native", "buffer", "scrollback", "history", "restore"], RootSettingValueKind::Boolean, false),
    (AgentSidebarEnabled, "agent_sidebar_enabled", [], Advanced, "UI", "Enable Agent Sidebar", "Allow the experimental agent sidebar to be toggled with the keyboard shortcut", ["agent", "sidebar", "experimental", "assistant", "panel"], RootSettingValueKind::Boolean, false),
    (AgentSidebarWidth, "agent_sidebar_width", [], Advanced, "UI", "Agent Sidebar Width", "Saved width for the experimental agent sidebar in pixels", ["agent", "sidebar", "width", "panel"], RootSettingValueKind::Numeric, false),
    (ShowPluginsTab, "show_plugins_tab", [], Advanced, "UI", "Show Plugins Tab", "Show the Plugins section in Settings", ["plugins", "settings", "sidebar", "tab"], RootSettingValueKind::Boolean, false),
    (ShowDebugOverlay, "show_debug_overlay", [], Advanced, "UI", "Show Debug Overlay", "Show FPS, CPU, and memory in the terminal corner", ["debug", "overlay", "fps", "cpu", "memory"], RootSettingValueKind::Boolean, false),
    (TmuxBinary, "tmux_binary", [], Terminal, "TMUX", "Tmux Binary", "tmux executable path or binary name", ["tmux", "binary", "path"], RootSettingValueKind::Text, false),
    (TmuxShowActivePaneBorder, "tmux_show_active_pane_border", [], Terminal, "TMUX", "Show Active Pane Border", "Show active tmux pane border highlight in managed sessions", ["tmux", "pane", "border", "highlight"], RootSettingValueKind::Boolean, false),
    (WorkingDir, "working_dir", [], Advanced, "STARTUP", "Working Directory", "Initial directory for new sessions", ["working directory", "cwd", "startup", "path"], RootSettingValueKind::Text, false),
    (WorkingDirFallback, "working_dir_fallback", ["default_working_dir"], Advanced, "STARTUP", "Working Directory Fallback", "Directory used when working_dir is unset", ["working directory", "fallback", "cwd", "startup"], RootSettingValueKind::Enum, false),
    (WarnOnQuitWithRunningProcess, "warn_on_quit_with_running_process", [], Advanced, "SAFETY", "Warn On Quit", "Warn before quitting when a tab has an active process", ["quit", "warning", "safety", "process"], RootSettingValueKind::Boolean, false),
    (TabTitlePriority, "tab_title_priority", [], Tabs, "TAB TITLES", "Title Priority", "Exact source priority for tab titles", ["tab", "title", "priority", "source"], RootSettingValueKind::Special, false),
    (TabTitleMode, "tab_title_mode", [], Tabs, "TAB TITLES", "Title Mode", "How tab titles are determined", ["tab", "title", "mode", "smart", "shell", "explicit", "static"], RootSettingValueKind::Enum, false),
    (TabTitleFallback, "tab_title_fallback", [], Tabs, "TAB TITLES", "Fallback Title", "Default tab title when no source is available", ["tab", "title", "fallback"], RootSettingValueKind::Text, false),
    (TabTitleExplicitPrefix, "tab_title_explicit_prefix", [], Tabs, "TAB TITLES", "Explicit Prefix", "Prefix used for explicit OSC title payloads", ["tab", "title", "prefix", "osc"], RootSettingValueKind::Text, false),
    (TabTitleShellIntegration, "tab_title_shell_integration", [], Tabs, "TAB TITLES", "Shell Integration", "Export TERMY_* environment values for shell hooks", ["shell", "integration", "env", "hooks"], RootSettingValueKind::Boolean, false),
    (TabTitlePromptFormat, "tab_title_prompt_format", [], Tabs, "TAB TITLES", "Prompt Format", "Template for prompt-derived tab titles", ["tab", "title", "prompt", "format"], RootSettingValueKind::Text, false),
    (TabTitleCommandFormat, "tab_title_command_format", [], Tabs, "TAB TITLES", "Command Format", "Template for command-derived tab titles", ["tab", "title", "command", "format"], RootSettingValueKind::Text, false),
    (TabCloseVisibility, "tab_close_visibility", [], Tabs, "TAB STRIP", "Close Button Visibility", "When tab close buttons are visible", ["tab", "close", "visibility", "hover"], RootSettingValueKind::Enum, false),
    (TabWidthMode, "tab_width_mode", [], Tabs, "TAB STRIP", "Tab Width Mode", "How tab widths react to active state", ["tab", "width", "layout", "active"], RootSettingValueKind::Enum, false),
    (TabSwitchModifierHints, "tab_switch_modifier_hints", [], Tabs, "TAB STRIP", "Show Tab Switch Hints", "Show secondary+1..9 number badges on the first nine tabs while the secondary modifier is held", ["tab", "switch", "hints", "modifier", "secondary", "shortcuts"], RootSettingValueKind::Boolean, false),
    (ShowTermyInTitlebar, "show_termy_in_titlebar", [], Tabs, "TITLE BAR", "Show Termy In Titlebar", "Show or hide the termy branding in the titlebar", ["titlebar", "branding", "tabs"], RootSettingValueKind::Boolean, false),
    (Shell, "shell", [], Terminal, "SHELL", "Shell", "Executable used for new sessions", ["shell", "bash", "zsh", "fish"], RootSettingValueKind::Text, false),
    (Term, "term", [], Terminal, "SHELL", "TERM", "TERM value exposed to child applications", ["term", "terminal", "env"], RootSettingValueKind::Text, false),
    (Colorterm, "colorterm", [], Terminal, "SHELL", "COLORTERM", "COLORTERM value exposed to child applications", ["colorterm", "color", "env"], RootSettingValueKind::Text, false),
    (WindowWidth, "window_width", [], Advanced, "WINDOW", "Window Width", "Default startup window width in pixels", ["window", "width", "startup", "size"], RootSettingValueKind::Numeric, false),
    (WindowHeight, "window_height", [], Advanced, "WINDOW", "Window Height", "Default startup window height in pixels", ["window", "height", "startup", "size"], RootSettingValueKind::Numeric, false),
    (FontFamily, "font_family", [], Appearance, "FONT", "Font Family", "Font family used in terminal UI", ["font", "typeface", "text"], RootSettingValueKind::Special, false),
    (FontSize, "font_size", [], Appearance, "FONT", "Font Size", "Terminal font size in pixels", ["font", "size", "text"], RootSettingValueKind::Numeric, false),
    (CursorStyle, "cursor_style", [], Terminal, "CURSOR", "Cursor Style", "Shape of the terminal cursor", ["cursor", "shape", "block", "line"], RootSettingValueKind::Enum, false),
    (CursorBlink, "cursor_blink", [], Terminal, "CURSOR", "Cursor Blink", "Enable blinking cursor animation", ["cursor", "blink", "animation"], RootSettingValueKind::Boolean, false),
    (BackgroundOpacity, "background_opacity", [], Appearance, "WINDOW", "Background Opacity", "Window background opacity (0.0 to 1.0)", ["background", "opacity", "transparency"], RootSettingValueKind::Numeric, false),
    (BackgroundBlur, "background_blur", [], Appearance, "WINDOW", "Background Blur", "Enable blur effect for transparent backgrounds", ["background", "blur", "window"], RootSettingValueKind::Boolean, false),
    (PaddingX, "padding_x", [], Appearance, "PADDING", "Horizontal Padding", "Left and right terminal padding", ["padding", "spacing", "horizontal"], RootSettingValueKind::Numeric, false),
    (PaddingY, "padding_y", [], Appearance, "PADDING", "Vertical Padding", "Top and bottom terminal padding", ["padding", "spacing", "vertical"], RootSettingValueKind::Numeric, false),
    (MouseScrollMultiplier, "mouse_scroll_multiplier", [], Terminal, "SCROLLING", "Scroll Multiplier", "Mouse wheel scroll speed multiplier", ["scroll", "mouse", "speed"], RootSettingValueKind::Numeric, false),
    (ScrollbarVisibility, "scrollbar_visibility", [], Terminal, "SCROLLING", "Scrollbar Visibility", "Terminal scrollbar visibility behavior", ["scrollbar", "visibility", "scroll"], RootSettingValueKind::Enum, false),
    (ScrollbarStyle, "scrollbar_style", [], Terminal, "SCROLLING", "Scrollbar Style", "Terminal scrollbar color style", ["scrollbar", "style", "theme"], RootSettingValueKind::Enum, false),
    (ScrollbackHistory, "scrollback_history", ["scrollback"], Terminal, "SCROLLING", "Scrollback History", "Lines retained in terminal scrollback", ["scrollback", "history", "buffer", "lines"], RootSettingValueKind::Numeric, false),
    (InactiveTabScrollback, "inactive_tab_scrollback", [], Terminal, "SCROLLING", "Inactive Tab Scrollback", "Scrollback limit for inactive tabs", ["scrollback", "inactive", "tabs"], RootSettingValueKind::Numeric, false),
    (PaneFocusEffect, "pane_focus_effect", [], Terminal, "UI", "Pane Focus Effect", "How inactive panes are visually dimmed when a pane is active", ["pane", "focus", "dimming", "effect"], RootSettingValueKind::Enum, false),
    (PaneFocusStrength, "pane_focus_strength", [], Terminal, "UI", "Pane Focus Strength", "Strength of active pane emphasis (0.0 to 2.0)", ["pane", "focus", "strength", "dimming"], RootSettingValueKind::Numeric, false),
    (CommandPaletteShowKeybinds, "command_palette_show_keybinds", [], Terminal, "UI", "Show Keybindings In Palette", "Show shortcut badges in command palette rows", ["palette", "keybinds", "shortcuts"], RootSettingValueKind::Boolean, false),
    (AiProvider, "ai_provider", [], Advanced, "AI", "AI Provider", "Provider used for AI input and model listing", ["ai", "provider", "openai", "gemini"], RootSettingValueKind::Enum, false),
    (OpenaiApiKey, "openai_api_key", ["openai_key"], Advanced, "AI", "OpenAI API Key", "API key for OpenAI integration", ["openai", "api", "key", "ai", "gpt"], RootSettingValueKind::Text, false),
    (GeminiApiKey, "gemini_api_key", ["google_ai_api_key"], Advanced, "AI", "Gemini API Key", "API key for Google Gemini integration", ["gemini", "google", "api", "key", "ai"], RootSettingValueKind::Text, false),
    (OpenaiModel, "openai_model", ["ai_model"], Advanced, "AI", "AI Model", "Model used for AI input requests", ["openai", "gemini", "model", "ai", "gpt"], RootSettingValueKind::Text, false),
    (Keybind, "keybind", [], Keybindings, "KEYBINDS", "Keybind Directive", "Keybinding override directive", ["keybind", "shortcut", "command"], RootSettingValueKind::Special, true),
}

define_color_settings! {
    (Foreground, "foreground", ["fg"], "Foreground", "Default text color", ["text", "foreground"]),
    (Background, "background", ["bg"], "Background", "Terminal background color", ["background", "surface"]),
    (Cursor, "cursor", [], "Cursor", "Cursor color", ["cursor"]),
    (Black, "black", ["color0"], "Black", "ANSI black", ["ansi", "black", "color0"]),
    (Red, "red", ["color1"], "Red", "ANSI red", ["ansi", "red", "color1"]),
    (Green, "green", ["color2"], "Green", "ANSI green", ["ansi", "green", "color2"]),
    (Yellow, "yellow", ["color3"], "Yellow", "ANSI yellow", ["ansi", "yellow", "color3"]),
    (Blue, "blue", ["color4"], "Blue", "ANSI blue", ["ansi", "blue", "color4"]),
    (Magenta, "magenta", ["color5"], "Magenta", "ANSI magenta", ["ansi", "magenta", "color5"]),
    (Cyan, "cyan", ["color6"], "Cyan", "ANSI cyan", ["ansi", "cyan", "color6"]),
    (White, "white", ["color7"], "White", "ANSI white", ["ansi", "white", "color7"]),
    (BrightBlack, "bright_black", ["brightblack", "color8"], "Bright Black", "ANSI bright black", ["ansi", "bright", "black", "color8"]),
    (BrightRed, "bright_red", ["brightred", "color9"], "Bright Red", "ANSI bright red", ["ansi", "bright", "red", "color9"]),
    (BrightGreen, "bright_green", ["brightgreen", "color10"], "Bright Green", "ANSI bright green", ["ansi", "bright", "green", "color10"]),
    (BrightYellow, "bright_yellow", ["brightyellow", "color11"], "Bright Yellow", "ANSI bright yellow", ["ansi", "bright", "yellow", "color11"]),
    (BrightBlue, "bright_blue", ["brightblue", "color12"], "Bright Blue", "ANSI bright blue", ["ansi", "bright", "blue", "color12"]),
    (BrightMagenta, "bright_magenta", ["brightmagenta", "color13"], "Bright Magenta", "ANSI bright magenta", ["ansi", "bright", "magenta", "color13"]),
    (BrightCyan, "bright_cyan", ["brightcyan", "color14"], "Bright Cyan", "ANSI bright cyan", ["ansi", "bright", "cyan", "color14"]),
    (BrightWhite, "bright_white", ["brightwhite", "color15"], "Bright White", "ANSI bright white", ["ansi", "bright", "white", "color15"]),
}

pub fn canonical_root_key(raw: &str) -> Option<&'static str> {
    root_setting_from_key(raw).map(|id| root_setting_spec(id).key)
}

pub fn canonical_color_key(raw: &str) -> Option<&'static str> {
    color_setting_from_key(raw).map(|id| color_setting_spec(id).key)
}

pub fn root_setting_value_kind(id: RootSettingId) -> RootSettingValueKind {
    root_setting_spec(id).value_kind
}

pub fn root_setting_enum_choices(id: RootSettingId) -> Option<&'static [EnumChoice]> {
    match id {
        RootSettingId::WorkingDirFallback => Some(WORKING_DIR_FALLBACK_ENUM_CHOICES),
        RootSettingId::TabTitleMode => Some(TAB_TITLE_MODE_ENUM_CHOICES),
        RootSettingId::TabCloseVisibility => Some(TAB_CLOSE_VISIBILITY_ENUM_CHOICES),
        RootSettingId::TabWidthMode => Some(TAB_WIDTH_MODE_ENUM_CHOICES),
        RootSettingId::CursorStyle => Some(CURSOR_STYLE_ENUM_CHOICES),
        RootSettingId::ScrollbarVisibility => Some(SCROLLBAR_VISIBILITY_ENUM_CHOICES),
        RootSettingId::ScrollbarStyle => Some(SCROLLBAR_STYLE_ENUM_CHOICES),
        RootSettingId::PaneFocusEffect => Some(PANE_FOCUS_EFFECT_ENUM_CHOICES),
        RootSettingId::AiProvider => Some(AI_PROVIDER_ENUM_CHOICES),
        _ => None,
    }
}

pub fn root_setting_default_value(config: &AppConfig, id: RootSettingId) -> Option<String> {
    match id {
        RootSettingId::Theme => Some(config.theme.clone()),
        RootSettingId::AutoUpdate => Some(config.auto_update.to_string()),
        RootSettingId::TmuxEnabled => Some(config.tmux_enabled.to_string()),
        RootSettingId::TmuxPersistence => Some(config.tmux_persistence.to_string()),
        RootSettingId::NativeTabPersistence => Some(config.native_tab_persistence.to_string()),
        RootSettingId::NativeLayoutAutosave => Some(config.native_layout_autosave.to_string()),
        RootSettingId::NativeBufferPersistence => {
            Some(config.native_buffer_persistence.to_string())
        }
        RootSettingId::AgentSidebarEnabled => Some(config.agent_sidebar_enabled.to_string()),
        RootSettingId::AgentSidebarWidth => Some(config.agent_sidebar_width.to_string()),
        RootSettingId::ShowPluginsTab => Some(config.show_plugins_tab.to_string()),
        RootSettingId::ShowDebugOverlay => Some(config.show_debug_overlay.to_string()),
        RootSettingId::TmuxBinary => Some(config.tmux_binary.clone()),
        RootSettingId::TmuxShowActivePaneBorder => {
            Some(config.tmux_show_active_pane_border.to_string())
        }
        RootSettingId::WorkingDir => config.working_dir.clone(),
        RootSettingId::WorkingDirFallback => Some(match config.working_dir_fallback {
            WorkingDirFallback::Home => "home".to_string(),
            WorkingDirFallback::Process => "process".to_string(),
        }),
        RootSettingId::WarnOnQuitWithRunningProcess => {
            Some(config.warn_on_quit_with_running_process.to_string())
        }
        RootSettingId::TabTitlePriority => Some(
            config
                .tab_title
                .priority
                .iter()
                .map(|source| match source {
                    crate::types::TabTitleSource::Manual => "manual",
                    crate::types::TabTitleSource::Explicit => "explicit",
                    crate::types::TabTitleSource::Shell => "shell",
                    crate::types::TabTitleSource::Fallback => "fallback",
                })
                .collect::<Vec<_>>()
                .join(", "),
        ),
        RootSettingId::TabTitleMode => Some(match config.tab_title.mode {
            TabTitleMode::Smart => "smart".to_string(),
            TabTitleMode::Shell => "shell".to_string(),
            TabTitleMode::Explicit => "explicit".to_string(),
            TabTitleMode::Static => "static".to_string(),
        }),
        RootSettingId::TabTitleFallback => Some(config.tab_title.fallback.clone()),
        RootSettingId::TabTitleExplicitPrefix => Some(config.tab_title.explicit_prefix.clone()),
        RootSettingId::TabTitleShellIntegration => {
            Some(config.tab_title.shell_integration.to_string())
        }
        RootSettingId::TabTitlePromptFormat => Some(config.tab_title.prompt_format.clone()),
        RootSettingId::TabTitleCommandFormat => Some(config.tab_title.command_format.clone()),
        RootSettingId::TabCloseVisibility => Some(match config.tab_close_visibility {
            TabCloseVisibility::ActiveHover => "active_hover".to_string(),
            TabCloseVisibility::Hover => "hover".to_string(),
            TabCloseVisibility::Always => "always".to_string(),
        }),
        RootSettingId::TabWidthMode => Some(match config.tab_width_mode {
            TabWidthMode::Stable => "stable".to_string(),
            TabWidthMode::ActiveGrow => "active_grow".to_string(),
            TabWidthMode::ActiveGrowSticky => "active_grow_sticky".to_string(),
        }),
        RootSettingId::TabSwitchModifierHints => Some(config.tab_switch_modifier_hints.to_string()),
        RootSettingId::ShowTermyInTitlebar => Some(config.show_termy_in_titlebar.to_string()),
        RootSettingId::Shell => config.shell.clone(),
        RootSettingId::Term => Some(config.term.clone()),
        RootSettingId::Colorterm => config.colorterm.clone(),
        RootSettingId::WindowWidth => Some(config.window_width.to_string()),
        RootSettingId::WindowHeight => Some(config.window_height.to_string()),
        RootSettingId::FontFamily => Some(config.font_family.clone()),
        RootSettingId::FontSize => Some(config.font_size.to_string()),
        RootSettingId::CursorStyle => Some(match config.cursor_style {
            CursorStyle::Line => "line".to_string(),
            CursorStyle::Block => "block".to_string(),
        }),
        RootSettingId::CursorBlink => Some(config.cursor_blink.to_string()),
        RootSettingId::BackgroundOpacity => Some(config.background_opacity.to_string()),
        RootSettingId::BackgroundBlur => Some(config.background_blur.to_string()),
        RootSettingId::PaddingX => Some(config.padding_x.to_string()),
        RootSettingId::PaddingY => Some(config.padding_y.to_string()),
        RootSettingId::MouseScrollMultiplier => Some(config.mouse_scroll_multiplier.to_string()),
        RootSettingId::ScrollbarVisibility => Some(match config.terminal_scrollbar_visibility {
            TerminalScrollbarVisibility::Off => "off".to_string(),
            TerminalScrollbarVisibility::Always => "always".to_string(),
            TerminalScrollbarVisibility::OnScroll => "on_scroll".to_string(),
        }),
        RootSettingId::ScrollbarStyle => Some(match config.terminal_scrollbar_style {
            TerminalScrollbarStyle::Neutral => "neutral".to_string(),
            TerminalScrollbarStyle::MutedTheme => "muted_theme".to_string(),
            TerminalScrollbarStyle::Theme => "theme".to_string(),
        }),
        RootSettingId::ScrollbackHistory => Some(config.scrollback_history.to_string()),
        RootSettingId::InactiveTabScrollback => {
            config.inactive_tab_scrollback.map(|v| v.to_string())
        }
        RootSettingId::PaneFocusEffect => Some(match config.pane_focus_effect {
            PaneFocusEffect::Off => "off".to_string(),
            PaneFocusEffect::SoftSpotlight => "soft_spotlight".to_string(),
            PaneFocusEffect::Cinematic => "cinematic".to_string(),
            PaneFocusEffect::Minimal => "minimal".to_string(),
        }),
        RootSettingId::PaneFocusStrength => Some(config.pane_focus_strength.to_string()),
        RootSettingId::CommandPaletteShowKeybinds => {
            Some(config.command_palette_show_keybinds.to_string())
        }
        RootSettingId::AiProvider => Some(match config.ai_provider {
            AiProvider::OpenAi => "openai".to_string(),
            AiProvider::Gemini => "gemini".to_string(),
        }),
        RootSettingId::OpenaiApiKey => config.openai_api_key.clone(),
        RootSettingId::GeminiApiKey => config.gemini_api_key.clone(),
        RootSettingId::OpenaiModel => config.openai_model.clone(),
        RootSettingId::Keybind => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enum_choice_values(id: RootSettingId) -> Vec<&'static str> {
        root_setting_enum_choices(id)
            .expect("missing enum choices")
            .iter()
            .map(|choice| choice.value)
            .collect()
    }

    #[test]
    fn enum_target_fields_have_expected_choices() {
        assert_eq!(
            root_setting_value_kind(RootSettingId::CursorStyle),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::CursorStyle),
            vec!["block", "line"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::TabTitleMode),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::TabTitleMode),
            vec!["smart", "shell", "explicit", "static"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::ScrollbarVisibility),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::ScrollbarVisibility),
            vec!["off", "always", "on_scroll"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::ScrollbarStyle),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::ScrollbarStyle),
            vec!["neutral", "muted_theme", "theme"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::TabCloseVisibility),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::TabCloseVisibility),
            vec!["active_hover", "hover", "always"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::TabWidthMode),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::TabWidthMode),
            vec!["stable", "active_grow", "active_grow_sticky"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::WorkingDirFallback),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::WorkingDirFallback),
            vec!["home", "process"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::PaneFocusEffect),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::PaneFocusEffect),
            vec!["off", "soft_spotlight", "cinematic", "minimal"]
        );

        assert_eq!(
            root_setting_value_kind(RootSettingId::AiProvider),
            RootSettingValueKind::Enum
        );
        assert_eq!(
            enum_choice_values(RootSettingId::AiProvider),
            vec!["openai", "gemini"]
        );
    }

    #[test]
    fn non_target_fields_are_not_enum() {
        assert_eq!(
            root_setting_value_kind(RootSettingId::Theme),
            RootSettingValueKind::Special
        );
        assert!(root_setting_enum_choices(RootSettingId::Theme).is_none());

        assert_eq!(
            root_setting_value_kind(RootSettingId::TabTitlePriority),
            RootSettingValueKind::Special
        );
        assert!(root_setting_enum_choices(RootSettingId::TabTitlePriority).is_none());

        assert_eq!(
            root_setting_value_kind(RootSettingId::CursorBlink),
            RootSettingValueKind::Boolean
        );
        assert!(root_setting_enum_choices(RootSettingId::CursorBlink).is_none());

        assert_eq!(
            root_setting_value_kind(RootSettingId::PaneFocusStrength),
            RootSettingValueKind::Numeric
        );
        assert!(root_setting_enum_choices(RootSettingId::PaneFocusStrength).is_none());
    }
}
