use gpui::{FocusHandle, KeyBinding, MenuItem, OsAction, Window, actions};
use termy_command_core::CommandId;

const GLOBAL_CONTEXT: Option<&str> = None;
const TERMINAL_CONTEXT: Option<&str> = Some("Terminal");
const INLINE_INPUT_CONTEXT: Option<&str> = Some("InlineInput");

pub type MenuSection = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPaletteVisibility {
    Always,
    MacOsOnly,
}

impl CommandPaletteVisibility {
    pub fn is_visible(self) -> bool {
        self.is_visible_on_macos(cfg!(target_os = "macos"))
    }

    pub const fn is_visible_on_macos(self, is_macos: bool) -> bool {
        match self {
            Self::Always => true,
            Self::MacOsOnly => is_macos,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuRoot {
    App,
    File,
    Edit,
    View,
    Window,
    Help,
}

impl MenuRoot {
    pub const fn title(self) -> &'static str {
        match self {
            Self::App => "Termy",
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Window => "Window",
            Self::Help => "Help",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuVisibility {
    Always,
    MacOsOnly,
}

impl MenuVisibility {
    pub fn is_visible(self) -> bool {
        self.is_visible_on_macos(cfg!(target_os = "macos"))
    }

    pub const fn is_visible_on_macos(self, is_macos: bool) -> bool {
        match self {
            Self::Always => true,
            Self::MacOsOnly => is_macos,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuActionRole {
    Normal,
    Copy,
    Paste,
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
pub struct CommandMenuSpec {
    pub root: MenuRoot,
    pub section: MenuSection,
    pub title: &'static str,
    pub visibility: MenuVisibility,
    pub role: MenuActionRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMenuEntry {
    pub action: CommandAction,
    pub root: MenuRoot,
    pub section: MenuSection,
    pub title: &'static str,
    pub role: MenuActionRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub action: CommandAction,
    pub context: Option<&'static str>,
    pub palette: Option<CommandPaletteSpec>,
    pub menu: Option<CommandMenuSpec>,
}

const MENU_ROOTS: [MenuRoot; 6] = [
    MenuRoot::App,
    MenuRoot::File,
    MenuRoot::Edit,
    MenuRoot::View,
    MenuRoot::Window,
    MenuRoot::Help,
];

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

const fn menu(
    root: MenuRoot,
    section: MenuSection,
    title: &'static str,
    visibility: MenuVisibility,
    role: MenuActionRole,
) -> CommandMenuSpec {
    CommandMenuSpec {
        root,
        section,
        title,
        visibility,
        role,
    }
}

const fn command(
    action: CommandAction,
    context: Option<&'static str>,
    palette: Option<CommandPaletteSpec>,
    menu: Option<CommandMenuSpec>,
) -> CommandSpec {
    CommandSpec {
        action,
        context,
        palette,
        menu,
    }
}

macro_rules! define_commands {
    ($(($variant:ident, $context:expr, $palette:expr, $menu:expr)),+ $(,)?) => {
        actions!(
            termy,
            [$( $variant, )+]
        );

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum CommandAction {
            $( $variant, )+
        }

        const COMMAND_SPECS: &[CommandSpec] = &[
            $(command(CommandAction::$variant, $context, $palette, $menu),)+
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

            pub fn menu_roots() -> &'static [MenuRoot] {
                &MENU_ROOTS
            }

            pub fn menu_entries_for_root(root: MenuRoot) -> Vec<CommandMenuEntry> {
                let mut entries = COMMAND_SPECS
                    .iter()
                    .filter_map(|spec| {
                        let menu = spec.menu?;
                        if menu.root != root || !menu.visibility.is_visible() {
                            return None;
                        }

                        Some(CommandMenuEntry {
                            action: spec.action,
                            root: menu.root,
                            section: menu.section,
                            title: menu.title,
                            role: menu.role,
                        })
                    })
                    .collect::<Vec<_>>();

                // App info is intentionally surfaced from both the app menu and Help.
                if root == MenuRoot::Help {
                    entries.push(CommandMenuEntry {
                        action: CommandAction::AppInfo,
                        root: MenuRoot::Help,
                        section: 0,
                        title: "App Info",
                        role: MenuActionRole::Normal,
                    });
                }

                entries
            }

            pub fn to_menu_item(self, title: &'static str, role: MenuActionRole) -> MenuItem {
                let os_action = match role {
                    MenuActionRole::Normal => None,
                    MenuActionRole::Copy => Some(OsAction::Copy),
                    MenuActionRole::Paste => Some(OsAction::Paste),
                };

                match self {
                    $(
                        Self::$variant => match os_action {
                            Some(os_action) => MenuItem::os_action(title, $variant, os_action),
                            None => MenuItem::action(title, $variant),
                        },
                    )+
                }
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
                    $(Self::$variant => window.bindings_for_action_in(&$variant, focus_handle).into_iter().next(),)+
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
        )),
        Some(menu(
            MenuRoot::File,
            0,
            "New Tab",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        CloseTab,
        TERMINAL_CONTEXT,
        Some(palette(
            "Close Tab",
            "remove tab",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::File,
            0,
            "Close Tab",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        MoveTabLeft,
        TERMINAL_CONTEXT,
        Some(palette(
            "Move Tab Left",
            "reorder tab left",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::Window,
            1,
            "Move Tab Left",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        MoveTabRight,
        TERMINAL_CONTEXT,
        Some(palette(
            "Move Tab Right",
            "reorder tab right",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::Window,
            1,
            "Move Tab Right",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        SwitchTabLeft,
        TERMINAL_CONTEXT,
        Some(palette(
            "Switch Tab Left",
            "change active tab left",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::Window,
            1,
            "Switch Tab Left",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        SwitchTabRight,
        TERMINAL_CONTEXT,
        Some(palette(
            "Switch Tab Right",
            "change active tab right",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::Window,
            1,
            "Switch Tab Right",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        MinimizeWindow,
        TERMINAL_CONTEXT,
        None,
        Some(menu(
            MenuRoot::Window,
            0,
            "Minimize",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        RenameTab,
        TERMINAL_CONTEXT,
        Some(palette(
            "Rename Tab",
            "title name",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::File,
            0,
            "Rename Tab",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        AppInfo,
        TERMINAL_CONTEXT,
        Some(palette(
            "App Info",
            "information version about build",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::App,
            0,
            "App Info",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        NativeSdkExample,
        TERMINAL_CONTEXT,
        Some(palette(
            "Native SDK Example",
            "native sdk modal popup confirm dialog example",
            CommandPaletteVisibility::Always
        )),
        None
    ),
    (
        RestartApp,
        TERMINAL_CONTEXT,
        Some(palette(
            "Restart App",
            "relaunch reopen restart",
            CommandPaletteVisibility::Always
        )),
        None
    ),
    (
        OpenConfig,
        GLOBAL_CONTEXT,
        Some(palette(
            "Open Settings File",
            "settings file config edit",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::App,
            1,
            "Open Config File...",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        OpenSettings,
        GLOBAL_CONTEXT,
        Some(palette(
            "Settings",
            "settings preferences options",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::App,
            1,
            "Settings...",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        ImportColors,
        TERMINAL_CONTEXT,
        Some(palette(
            "Import Colors",
            "theme palette json",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::View,
            2,
            "Import Colors",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        SwitchTheme,
        TERMINAL_CONTEXT,
        Some(palette(
            "Switch Theme",
            "theme palette colors appearance",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::View,
            2,
            "Switch Theme",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        ZoomIn,
        TERMINAL_CONTEXT,
        Some(palette(
            "Zoom In",
            "font increase",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::View,
            1,
            "Zoom In",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        ZoomOut,
        TERMINAL_CONTEXT,
        Some(palette(
            "Zoom Out",
            "font decrease",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::View,
            1,
            "Zoom Out",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        ZoomReset,
        TERMINAL_CONTEXT,
        Some(palette(
            "Reset Zoom",
            "font default",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::View,
            1,
            "Reset Zoom",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        OpenSearch,
        TERMINAL_CONTEXT,
        Some(palette(
            "Find",
            "search lookup text",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::Edit,
            1,
            "Find",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        CheckForUpdates,
        TERMINAL_CONTEXT,
        Some(palette(
            "Check for Updates",
            "release version updater",
            CommandPaletteVisibility::MacOsOnly
        )),
        Some(menu(
            MenuRoot::App,
            0,
            "Check for Updates",
            MenuVisibility::MacOsOnly,
            MenuActionRole::Normal
        ))
    ),
    (
        Quit,
        GLOBAL_CONTEXT,
        Some(palette(
            "Quit Termy",
            "quit exit close",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::App,
            2,
            "Quit Termy",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        ToggleCommandPalette,
        TERMINAL_CONTEXT,
        None,
        Some(menu(
            MenuRoot::View,
            0,
            "Command Palette",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        Copy,
        TERMINAL_CONTEXT,
        None,
        Some(menu(
            MenuRoot::Edit,
            0,
            "Copy",
            MenuVisibility::Always,
            MenuActionRole::Copy
        ))
    ),
    (
        Paste,
        TERMINAL_CONTEXT,
        None,
        Some(menu(
            MenuRoot::Edit,
            0,
            "Paste",
            MenuVisibility::Always,
            MenuActionRole::Paste
        ))
    ),
    (CloseSearch, TERMINAL_CONTEXT, None, None),
    (
        SearchNext,
        TERMINAL_CONTEXT,
        None,
        Some(menu(
            MenuRoot::Edit,
            1,
            "Find Next",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (
        SearchPrevious,
        TERMINAL_CONTEXT,
        None,
        Some(menu(
            MenuRoot::Edit,
            1,
            "Find Previous",
            MenuVisibility::Always,
            MenuActionRole::Normal
        ))
    ),
    (ToggleSearchCaseSensitive, TERMINAL_CONTEXT, None, None),
    (ToggleSearchRegex, TERMINAL_CONTEXT, None, None),
    (
        InstallCli,
        TERMINAL_CONTEXT,
        Some(palette(
            "Install CLI",
            "install command line interface terminal shell path",
            CommandPaletteVisibility::Always
        )),
        Some(menu(
            MenuRoot::Help,
            0,
            "Install CLI",
            MenuVisibility::Always,
            MenuActionRole::Normal
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
    use super::{CommandAction, MenuActionRole, MenuRoot, MenuVisibility};
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
    fn menu_entries_have_non_empty_titles_and_valid_sections() {
        for root in CommandAction::menu_roots() {
            for entry in CommandAction::menu_entries_for_root(*root) {
                assert!(!entry.title.trim().is_empty());
                assert_eq!(entry.root, *root);
            }
        }
    }

    #[test]
    fn menu_entries_do_not_collide_on_root_section_and_title() {
        let mut seen = HashSet::new();
        for root in CommandAction::menu_roots() {
            for entry in CommandAction::menu_entries_for_root(*root) {
                assert!(
                    seen.insert((entry.root, entry.section, entry.title)),
                    "duplicate menu entry for ({:?}, {}, {:?})",
                    entry.root,
                    entry.section,
                    entry.title
                );
            }
        }
    }

    #[test]
    fn menu_visibility_filters_by_platform() {
        assert!(MenuVisibility::Always.is_visible_on_macos(true));
        assert!(MenuVisibility::Always.is_visible_on_macos(false));
        assert!(MenuVisibility::MacOsOnly.is_visible_on_macos(true));
        assert!(!MenuVisibility::MacOsOnly.is_visible_on_macos(false));
    }

    #[test]
    fn only_copy_and_paste_use_os_edit_roles() {
        for root in CommandAction::menu_roots() {
            for entry in CommandAction::menu_entries_for_root(*root) {
                match entry.role {
                    MenuActionRole::Copy => assert_eq!(entry.action, CommandAction::Copy),
                    MenuActionRole::Paste => assert_eq!(entry.action, CommandAction::Paste),
                    MenuActionRole::Normal => {}
                }
            }
        }
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

    #[test]
    fn menu_roots_are_stable_and_ordered() {
        assert_eq!(
            CommandAction::menu_roots(),
            &[
                MenuRoot::App,
                MenuRoot::File,
                MenuRoot::Edit,
                MenuRoot::View,
                MenuRoot::Window,
                MenuRoot::Help,
            ]
        );
    }
}
