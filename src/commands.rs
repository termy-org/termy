use gpui::{FocusHandle, KeyBinding, Window, actions};
use termy_command_core::CommandId;

const GLOBAL_CONTEXT: Option<&str> = None;
const TERMINAL_CONTEXT: Option<&str> = Some("Terminal");
const INLINE_INPUT_CONTEXT: Option<&str> = Some("InlineInput");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPaletteVisibility {
    Always,
    MacOsOnly,
}

impl CommandPaletteVisibility {
    pub fn is_visible(self) -> bool {
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
    context: Option<&'static str>,
    palette: Option<CommandPaletteSpec>,
) -> CommandSpec {
    CommandSpec {
        action,
        context,
        palette,
    }
}

macro_rules! define_commands {
    ($(($variant:ident, $context:expr, $palette:expr)),+ $(,)?) => {
        actions!(
            termy,
            [$( $variant, )+]
        );

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum CommandAction {
            $( $variant, )+
        }

        const COMMAND_SPECS: &[CommandSpec] = &[
            $(command(CommandAction::$variant, $context, $palette),)+
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

            #[allow(dead_code)]
            pub fn from_config_name(name: &str) -> Option<Self> {
                CommandId::from_config_name(name).map(Self::from_command_id)
            }

            #[allow(dead_code)]
            pub fn all_config_names() -> impl std::iter::ExactSizeIterator<Item = &'static str> {
                CommandId::all_config_names()
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

macro_rules! impl_command_action_id_mapping {
    ($(($variant:ident, $_config_name:literal),)+) => {
        impl CommandAction {
            pub fn from_command_id(id: CommandId) -> Self {
                match id {
                    $(CommandId::$variant => Self::$variant,)+
                }
            }

            #[allow(dead_code)]
            pub fn to_command_id(self) -> CommandId {
                match self {
                    $(Self::$variant => CommandId::$variant,)+
                }
            }
        }
    };
}

define_commands!(
    (
        NewTab,
        TERMINAL_CONTEXT,
        Some(palette(
            "New Tab",
            "create tab",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        CloseTab,
        TERMINAL_CONTEXT,
        Some(palette(
            "Close Tab",
            "remove tab",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        MoveTabLeft,
        TERMINAL_CONTEXT,
        Some(palette(
            "Move Tab Left",
            "reorder tab left",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        MoveTabRight,
        TERMINAL_CONTEXT,
        Some(palette(
            "Move Tab Right",
            "reorder tab right",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        SwitchTabLeft,
        TERMINAL_CONTEXT,
        Some(palette(
            "Switch Tab Left",
            "change active tab left",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        SwitchTabRight,
        TERMINAL_CONTEXT,
        Some(palette(
            "Switch Tab Right",
            "change active tab right",
            CommandPaletteVisibility::Always
        ))
    ),
    (MinimizeWindow, TERMINAL_CONTEXT, None),
    (
        RenameTab,
        TERMINAL_CONTEXT,
        Some(palette(
            "Rename Tab",
            "title name",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        AppInfo,
        TERMINAL_CONTEXT,
        Some(palette(
            "App Info",
            "information version about build",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        NativeSdkExample,
        TERMINAL_CONTEXT,
        Some(palette(
            "Native SDK Example",
            "native sdk modal popup confirm dialog example",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        RestartApp,
        TERMINAL_CONTEXT,
        Some(palette(
            "Restart App",
            "relaunch reopen restart",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        OpenConfig,
        GLOBAL_CONTEXT,
        Some(palette(
            "Open Settings File",
            "settings file config edit",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        OpenSettings,
        GLOBAL_CONTEXT,
        Some(palette(
            "Settings",
            "settings preferences options",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ImportColors,
        TERMINAL_CONTEXT,
        Some(palette(
            "Import Colors",
            "theme palette json",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        SwitchTheme,
        TERMINAL_CONTEXT,
        Some(palette(
            "Switch Theme",
            "theme palette colors appearance",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ZoomIn,
        TERMINAL_CONTEXT,
        Some(palette(
            "Zoom In",
            "font increase",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ZoomOut,
        TERMINAL_CONTEXT,
        Some(palette(
            "Zoom Out",
            "font decrease",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        ZoomReset,
        TERMINAL_CONTEXT,
        Some(palette(
            "Reset Zoom",
            "font default",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        OpenSearch,
        TERMINAL_CONTEXT,
        Some(palette(
            "Find",
            "search lookup text",
            CommandPaletteVisibility::Always
        ))
    ),
    (
        CheckForUpdates,
        TERMINAL_CONTEXT,
        Some(palette(
            "Check for Updates",
            "release version updater",
            CommandPaletteVisibility::MacOsOnly
        ))
    ),
    (
        Quit,
        GLOBAL_CONTEXT,
        Some(palette(
            "Quit Termy",
            "quit exit close",
            CommandPaletteVisibility::Always
        ))
    ),
    (ToggleCommandPalette, TERMINAL_CONTEXT, None),
    (Copy, TERMINAL_CONTEXT, None),
    (Paste, TERMINAL_CONTEXT, None),
    (CloseSearch, TERMINAL_CONTEXT, None),
    (SearchNext, TERMINAL_CONTEXT, None),
    (SearchPrevious, TERMINAL_CONTEXT, None),
    (ToggleSearchCaseSensitive, TERMINAL_CONTEXT, None),
    (ToggleSearchRegex, TERMINAL_CONTEXT, None),
    (
        InstallCli,
        TERMINAL_CONTEXT,
        Some(palette(
            "Install CLI",
            "install command line interface terminal shell path",
            CommandPaletteVisibility::Always
        ))
    ),
);

termy_command_core::termy_command_catalog!(impl_command_action_id_mapping);

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
    use termy_command_core::CommandId;

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
                .any(|entry| entry.action == CommandAction::MoveTabLeft)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == CommandAction::MoveTabRight)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == CommandAction::SwitchTabLeft)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == CommandAction::SwitchTabRight)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == CommandAction::RenameTab)
        );
    }

    #[test]
    fn command_action_roundtrips_all_core_command_ids() {
        for command_id in CommandId::all() {
            let action = CommandAction::from_command_id(command_id);
            assert_eq!(action.to_command_id(), command_id);
        }
    }

    #[test]
    fn command_action_count_matches_core_catalog() {
        assert_eq!(CommandAction::all().count(), CommandId::all().count());
    }
}
