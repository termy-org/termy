#[macro_export]
macro_rules! termy_command_catalog {
    ($visitor:ident) => {
        $visitor! {
            (NewTab, "new_tab"),
            (CloseTab, "close_tab"),
            (MoveTabLeft, "move_tab_left"),
            (MoveTabRight, "move_tab_right"),
            (SwitchTabLeft, "switch_tab_left"),
            (SwitchTabRight, "switch_tab_right"),
            (MinimizeWindow, "minimize_window"),
            (RenameTab, "rename_tab"),
            (AppInfo, "app_info"),
            (NativeSdkExample, "native_sdk_example"),
            (RestartApp, "restart_app"),
            (OpenConfig, "open_config"),
            (OpenSettings, "open_settings"),
            (ImportColors, "import_colors"),
            (SwitchTheme, "switch_theme"),
            (ZoomIn, "zoom_in"),
            (ZoomOut, "zoom_out"),
            (ZoomReset, "zoom_reset"),
            (OpenSearch, "open_search"),
            (CheckForUpdates, "check_for_updates"),
            (Quit, "quit"),
            (ToggleCommandPalette, "toggle_command_palette"),
            (Copy, "copy"),
            (Paste, "paste"),
            (CloseSearch, "close_search"),
            (SearchNext, "search_next"),
            (SearchPrevious, "search_previous"),
            (ToggleSearchCaseSensitive, "toggle_search_case_sensitive"),
            (ToggleSearchRegex, "toggle_search_regex"),
            (InstallCli, "install_cli"),
        }
    };
}

macro_rules! define_command_catalog {
    ($(($variant:ident, $config_name:literal),)+) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum CommandId {
            $($variant,)+
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct CommandSpec {
            pub id: CommandId,
            pub config_name: &'static str,
        }

        const COMMAND_IDS: &[CommandId] = &[
            $(CommandId::$variant,)+
        ];

        const COMMAND_CONFIG_NAMES: &[&str] = &[
            $($config_name,)+
        ];

        const COMMAND_SPECS: &[CommandSpec] = &[
            $(CommandSpec {
                id: CommandId::$variant,
                config_name: $config_name,
            },)+
        ];

        pub fn command_specs() -> &'static [CommandSpec] {
            COMMAND_SPECS
        }

        impl CommandId {
            pub fn from_config_name(name: &str) -> Option<Self> {
                let normalized = normalize_config_name(name);
                match normalized.as_str() {
                    $($config_name => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub fn config_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $config_name,)+
                }
            }

            pub fn all() -> impl std::iter::ExactSizeIterator<Item = Self> + Clone {
                COMMAND_IDS.iter().copied()
            }

            pub fn all_config_names() -> impl std::iter::ExactSizeIterator<Item = &'static str> {
                COMMAND_CONFIG_NAMES.iter().copied()
            }
        }
    };
}

termy_command_catalog!(define_command_catalog);

fn normalize_config_name(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::{CommandId, command_specs};

    #[test]
    fn command_catalog_has_unique_config_names() {
        let mut names = command_specs()
            .iter()
            .map(|spec| spec.config_name)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), command_specs().len());
    }

    #[test]
    fn from_config_name_accepts_dash_and_underscore() {
        assert_eq!(
            CommandId::from_config_name("toggle-command-palette"),
            Some(CommandId::ToggleCommandPalette)
        );
        assert_eq!(
            CommandId::from_config_name("toggle_command_palette"),
            Some(CommandId::ToggleCommandPalette)
        );
    }
}
