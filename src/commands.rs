use gpui::{FocusHandle, KeyBinding, Window, actions};

const GLOBAL_CONTEXT: Option<&str> = None;
const TERMINAL_CONTEXT: Option<&str> = Some("Terminal");
const INLINE_INPUT_CONTEXT: Option<&str> = Some("InlineInput");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPaletteVisibility {
    Always,
    MacOsOnly,
}

impl CommandPaletteVisibility {
    fn is_visible(self) -> bool {
        match self {
            Self::Always => true,
            Self::MacOsOnly => cfg!(target_os = "macos"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteSpec {
    pub title: &'static str,
    pub keywords: &'static str,
    pub visibility: CommandPaletteVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteEntry {
    pub action: CommandAction,
    pub title: &'static str,
    pub keywords: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub action: CommandAction,
    pub config_name: &'static str,
    pub context: Option<&'static str>,
    pub palette: Option<CommandPaletteSpec>,
}

const fn palette(
    title: &'static str,
    keywords: &'static str,
    visibility: CommandPaletteVisibility,
) -> CommandPaletteSpec {
    CommandPaletteSpec {
        title,
        keywords,
        visibility,
    }
}

const fn command(
    action: CommandAction,
    config_name: &'static str,
    context: Option<&'static str>,
    palette: Option<CommandPaletteSpec>,
) -> CommandSpec {
    CommandSpec {
        action,
        config_name,
        context,
        palette,
    }
}

macro_rules! define_commands {
    ($(($variant:ident, $config_name:literal, $context:expr, $palette:expr)),+ $(,)?) => {
        actions!(
            termy,
            [$( $variant, )+]
        );

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum CommandAction {
            $( $variant, )+
        }

        const COMMAND_SPECS: &[CommandSpec] = &[
            $(command(CommandAction::$variant, $config_name, $context, $palette),)+
        ];

        impl CommandAction {
            #[cfg(test)]
            pub fn specs() -> &'static [CommandSpec] {
                COMMAND_SPECS
            }

            #[cfg(test)]
            pub fn all() -> impl std::iter::ExactSizeIterator<Item = Self> + Clone {
                COMMAND_SPECS.iter().map(|spec| spec.action)
            }

            pub fn from_config_name(name: &str) -> Option<Self> {
                let normalized = name.trim().to_ascii_lowercase().replace('-', "_");
                COMMAND_SPECS
                    .iter()
                    .find_map(|spec| (spec.config_name == normalized).then_some(spec.action))
            }

            pub fn all_config_names() -> impl std::iter::ExactSizeIterator<Item = &'static str> {
                COMMAND_SPECS.iter().map(|spec| spec.config_name)
            }

            pub fn palette_entries() -> Vec<CommandPaletteEntry> {
                COMMAND_SPECS
                    .iter()
                    .filter_map(|spec| {
                        let palette = spec.palette?;
                        if !palette.visibility.is_visible() {
                            return None;
                        }

                        Some(CommandPaletteEntry {
                            action: spec.action,
                            title: palette.title,
                            keywords: palette.keywords,
                        })
                    })
                    .collect()
            }

            pub fn to_key_binding(self, trigger: &str) -> KeyBinding {
                match self {
                    $(Self::$variant => KeyBinding::new(trigger, $variant, $context),)+
                }
            }

            pub fn keybinding_label(
                self,
                window: &Window,
                focus_handle: &FocusHandle,
            ) -> Option<String> {
                let binding = match self {
                    $(Self::$variant => window.highest_precedence_binding_for_action_in(&$variant, focus_handle),)+
                };

                binding.map(|binding| {
                    binding
                        .keystrokes()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            }
        }
    };
}

define_commands!(
    (
        NewTab,
        "new_tab",
        TERMINAL_CONTEXT,
        Some(palette(
            "New Tab",
            "create tab",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        CloseTab,
        "close_tab",
        TERMINAL_CONTEXT,
        Some(palette(
            "Close Tab",
            "remove tab",
            CommandPaletteVisibility::Always
        ))
    ),
    (MinimizeWindow, "minimize_window", TERMINAL_CONTEXT, None),
    (
        RenameTab,
        "rename_tab",
        TERMINAL_CONTEXT,
        Some(palette(
            "Rename Tab",
            "title name",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        AppInfo,
        "app_info",
        TERMINAL_CONTEXT,
        Some(palette(
            "App Info",
            "information version about build",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        NativeSdkExample,
        "native_sdk_example",
        TERMINAL_CONTEXT,
        Some(palette(
            "Native SDK Example",
            "native sdk modal popup confirm dialog example",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        RestartApp,
        "restart_app",
        TERMINAL_CONTEXT,
        Some(palette(
            "Restart App",
            "relaunch reopen restart",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        OpenConfig,
        "open_config",
        GLOBAL_CONTEXT,
        Some(palette(
            "Open Settings File",
            "settings file config edit",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        OpenSettings,
        "open_settings",
        GLOBAL_CONTEXT,
        Some(palette(
            "Settings",
            "settings preferences options",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ImportColors,
        "import_colors",
        TERMINAL_CONTEXT,
        Some(palette(
            "Import Colors",
            "theme palette json",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        SwitchTheme,
        "switch_theme",
        TERMINAL_CONTEXT,
        Some(palette(
            "Switch Theme",
            "theme palette colors appearance",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ZoomIn,
        "zoom_in",
        TERMINAL_CONTEXT,
        Some(palette(
            "Zoom In",
            "font increase",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ZoomOut,
        "zoom_out",
        TERMINAL_CONTEXT,
        Some(palette(
            "Zoom Out",
            "font decrease",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ZoomReset,
        "zoom_reset",
        TERMINAL_CONTEXT,
        Some(palette(
            "Reset Zoom",
            "font default",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        OpenSearch,
        "open_search",
        TERMINAL_CONTEXT,
        Some(palette(
            "Find",
            "search lookup text",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        CheckForUpdates,
        "check_for_updates",
        TERMINAL_CONTEXT,
        Some(palette(
            "Check for Updates",
            "release version updater",
            CommandPaletteVisibility::MacOsOnly
        ))
    ),
    (
        Quit,
        "quit",
        GLOBAL_CONTEXT,
        Some(palette(
            "Quit Termy",
            "quit exit close",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ToggleCommandPalette,
        "toggle_command_palette",
        TERMINAL_CONTEXT,
        None
    ),
    (Copy, "copy", TERMINAL_CONTEXT, None),
    (Paste, "paste", TERMINAL_CONTEXT, None),
    (CloseSearch, "close_search", TERMINAL_CONTEXT, None),
    (SearchNext, "search_next", TERMINAL_CONTEXT, None),
    (SearchPrevious, "search_previous", TERMINAL_CONTEXT, None),
    (
        ToggleSearchCaseSensitive,
        "toggle_search_case_sensitive",
        TERMINAL_CONTEXT,
        None
    ),
    (
        ToggleSearchRegex,
        "toggle_search_regex",
        TERMINAL_CONTEXT,
        None
    ),
    (
        InstallCli,
        "install_cli",
        TERMINAL_CONTEXT,
        Some(palette(
            "Install CLI",
            "install command line interface terminal shell path",
            CommandPaletteVisibility::Always
        ))
    ),
);

actions!(
    termy_inline_input,
    [
        InlineBackspace,
        InlineDelete,
        InlineMoveLeft,
        InlineMoveRight,
        InlineSelectLeft,
        InlineSelectRight,
        InlineSelectAll,
        InlineMoveToStart,
        InlineMoveToEnd,
        InlineDeleteWordBackward,
        InlineDeleteWordForward,
        InlineDeleteToStart,
        InlineDeleteToEnd,
    ]
);

pub fn inline_input_keybindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", InlineBackspace, INLINE_INPUT_CONTEXT),
        KeyBinding::new("delete", InlineDelete, INLINE_INPUT_CONTEXT),
        KeyBinding::new("left", InlineMoveLeft, INLINE_INPUT_CONTEXT),
        KeyBinding::new("right", InlineMoveRight, INLINE_INPUT_CONTEXT),
        KeyBinding::new("shift-left", InlineSelectLeft, INLINE_INPUT_CONTEXT),
        KeyBinding::new("shift-right", InlineSelectRight, INLINE_INPUT_CONTEXT),
        KeyBinding::new("secondary-a", InlineSelectAll, INLINE_INPUT_CONTEXT),
        KeyBinding::new("home", InlineMoveToStart, INLINE_INPUT_CONTEXT),
        KeyBinding::new("end", InlineMoveToEnd, INLINE_INPUT_CONTEXT),
        KeyBinding::new("secondary-left", InlineMoveToStart, INLINE_INPUT_CONTEXT),
        KeyBinding::new("secondary-right", InlineMoveToEnd, INLINE_INPUT_CONTEXT),
        KeyBinding::new(
            "alt-backspace",
            InlineDeleteWordBackward,
            INLINE_INPUT_CONTEXT,
        ),
        KeyBinding::new("alt-delete", InlineDeleteWordForward, INLINE_INPUT_CONTEXT),
        KeyBinding::new(
            "secondary-backspace",
            InlineDeleteToStart,
            INLINE_INPUT_CONTEXT,
        ),
        KeyBinding::new("secondary-delete", InlineDeleteToEnd, INLINE_INPUT_CONTEXT),
        KeyBinding::new("ctrl-backspace", InlineDeleteToStart, INLINE_INPUT_CONTEXT),
    ]
}

#[cfg(test)]
mod tests {
    use super::CommandAction;
    use std::collections::HashSet;

    #[test]
    fn command_catalog_contains_unique_actions() {
        let mut seen = HashSet::new();
        for spec in CommandAction::specs() {
            assert!(seen.insert(spec.action), "duplicate action in catalog");
        }

        assert_eq!(seen.len(), CommandAction::all().count());
    }

    #[test]
    fn switch_theme_is_configurable_and_palette_visible() {
        assert_eq!(
            CommandAction::from_config_name("switch_theme"),
            Some(CommandAction::SwitchTheme)
        );
        assert!(
            CommandAction::palette_entries()
                .iter()
                .any(|entry| entry.action == CommandAction::SwitchTheme)
        );
    }

    #[test]
    fn tab_actions_are_always_palette_visible() {
        let entries = CommandAction::palette_entries();
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == CommandAction::NewTab)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == CommandAction::CloseTab)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == CommandAction::RenameTab)
        );
    }
}
