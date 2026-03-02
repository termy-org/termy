use crate::commands::{CommandAction, CommandMenuEntry, MenuRoot};
use gpui::{Menu, MenuItem};
#[cfg(target_os = "macos")]
use gpui::SystemMenuType;
use termy_command_core::{CommandAvailability, CommandCapabilities, CommandUnavailableReason};

const INSTALL_CLI_TITLE: &str = "Install CLI";
const INSTALL_CLI_INSTALLED_TITLE: &str = "Install CLI (Installed)";
const SPLIT_PANE_VERTICAL_TMUX_REQUIRED_TITLE: &str = "Split Pane Vertical (tmux required)";
const SPLIT_PANE_HORIZONTAL_TMUX_REQUIRED_TITLE: &str = "Split Pane Horizontal (tmux required)";
const CLOSE_PANE_TMUX_REQUIRED_TITLE: &str = "Close Pane (tmux required)";
const FOCUS_NEXT_PANE_TMUX_REQUIRED_TITLE: &str = "Focus Next Pane (tmux required)";

pub(crate) fn app_menus(install_cli_available: bool, tmux_enabled: bool) -> Vec<Menu> {
    let capabilities = CommandCapabilities {
        tmux_runtime_active: tmux_enabled,
        install_cli_available,
    };

    CommandAction::menu_roots()
        .iter()
        .copied()
        .map(|root| build_menu(root, capabilities))
        .collect()
}

fn build_menu(root: MenuRoot, capabilities: CommandCapabilities) -> Menu {
    let entries = CommandAction::menu_entries_for_root(root);
    let mut items = Vec::new();

    #[cfg(target_os = "macos")]
    if root == MenuRoot::App {
        items.push(MenuItem::os_submenu("Services", SystemMenuType::Services));
        if !entries.is_empty() {
            items.push(MenuItem::separator());
        }
    }

    append_menu_entries(&mut items, &entries, capabilities);

    Menu {
        name: root.title().into(),
        items,
    }
}

fn append_menu_entries(
    items: &mut Vec<MenuItem>,
    entries: &[CommandMenuEntry],
    capabilities: CommandCapabilities,
) {
    let mut previous_section = None;

    for entry in entries {
        let availability = entry.action.availability(capabilities);
        let Some(title) = menu_item_title(entry, availability) else {
            continue;
        };

        if let Some(section) = previous_section {
            if section != entry.section {
                items.push(MenuItem::separator());
            }
        }

        items.push(entry.action.to_menu_item(title, entry.role));
        previous_section = Some(entry.section);
    }
}

fn menu_item_title(
    entry: &CommandMenuEntry,
    availability: CommandAvailability,
) -> Option<&'static str> {
    if availability.enabled {
        if entry.action == CommandAction::InstallCli {
            return Some(INSTALL_CLI_TITLE);
        }
        return Some(entry.title);
    }

    match availability.reason {
        // Keep only the requested pane actions visible in native mode and label
        // them explicitly so users understand why those rows are unavailable.
        Some(CommandUnavailableReason::RequiresTmuxRuntime) => tmux_required_menu_title(entry.action),
        Some(CommandUnavailableReason::InstallCliAlreadyInstalled) => {
            Some(INSTALL_CLI_INSTALLED_TITLE)
        }
        None => unreachable!("disabled command must include an unavailable reason"),
    }
}

fn tmux_required_menu_title(action: CommandAction) -> Option<&'static str> {
    match action {
        CommandAction::SplitPaneVertical => Some(SPLIT_PANE_VERTICAL_TMUX_REQUIRED_TITLE),
        CommandAction::SplitPaneHorizontal => Some(SPLIT_PANE_HORIZONTAL_TMUX_REQUIRED_TITLE),
        CommandAction::ClosePane => Some(CLOSE_PANE_TMUX_REQUIRED_TITLE),
        CommandAction::FocusPaneNext => Some(FOCUS_NEXT_PANE_TMUX_REQUIRED_TITLE),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CLOSE_PANE_TMUX_REQUIRED_TITLE, FOCUS_NEXT_PANE_TMUX_REQUIRED_TITLE,
        INSTALL_CLI_INSTALLED_TITLE, INSTALL_CLI_TITLE, SPLIT_PANE_HORIZONTAL_TMUX_REQUIRED_TITLE,
        SPLIT_PANE_VERTICAL_TMUX_REQUIRED_TITLE, app_menus,
    };
    use crate::commands::CommandAction;
    use gpui::{MenuItem, OsAction};
    use termy_command_core::{CommandCapabilities, CommandUnavailableReason};

    #[test]
    fn top_level_menu_order_is_stable() {
        let names = app_menus(true, true)
            .into_iter()
            .map(|menu| menu.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(names, ["Termy", "File", "Edit", "View", "Window", "Help"]);
    }

    #[test]
    fn app_menu_includes_services_only_on_macos() {
        let app_menu = app_menus(true, true)
            .into_iter()
            .find(|menu| menu.name.as_ref() == "Termy")
            .expect("missing Termy menu");

        let has_services = app_menu
            .items
            .iter()
            .any(|item| matches!(item, MenuItem::SystemMenu(_)));

        #[cfg(target_os = "macos")]
        assert!(has_services);

        #[cfg(not(target_os = "macos"))]
        assert!(!has_services);
    }

    #[test]
    fn edit_menu_copy_and_paste_use_os_actions() {
        let edit_menu = app_menus(true, true)
            .into_iter()
            .find(|menu| menu.name.as_ref() == "Edit")
            .expect("missing Edit menu");

        let mut copy_os_action = None;
        let mut paste_os_action = None;

        for item in &edit_menu.items {
            if let MenuItem::Action {
                name, os_action, ..
            } = item
            {
                match name.as_ref() {
                    "Copy" => copy_os_action = *os_action,
                    "Paste" => paste_os_action = *os_action,
                    _ => {}
                }
            }
        }

        assert!(matches!(copy_os_action, Some(OsAction::Copy)));
        assert!(matches!(paste_os_action, Some(OsAction::Paste)));
    }

    #[test]
    fn separators_are_inserted_only_between_sections() {
        for menu in app_menus(true, true) {
            if menu.items.is_empty() {
                continue;
            }

            assert!(!matches!(menu.items.first(), Some(MenuItem::Separator)));
            assert!(!matches!(menu.items.last(), Some(MenuItem::Separator)));

            let mut previous_was_separator = false;
            for item in &menu.items {
                if matches!(item, MenuItem::Separator) {
                    assert!(!previous_was_separator);
                    previous_was_separator = true;
                } else {
                    previous_was_separator = false;
                }
            }
        }
    }

    #[test]
    fn install_cli_menu_title_reflects_install_state() {
        let help_menu_available = app_menus(true, true)
            .into_iter()
            .find(|menu| menu.name.as_ref() == "Help")
            .expect("missing Help menu");
        let help_menu_installed = app_menus(false, true)
            .into_iter()
            .find(|menu| menu.name.as_ref() == "Help")
            .expect("missing Help menu");

        let install_cli_titles = |menu: &gpui::Menu| {
            menu.items
                .iter()
                .filter_map(|item| {
                    let MenuItem::Action { name, .. } = item else {
                        return None;
                    };
                    let title = name.as_ref();
                    if title == INSTALL_CLI_TITLE || title == INSTALL_CLI_INSTALLED_TITLE {
                        Some(title.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(install_cli_titles(&help_menu_available), [INSTALL_CLI_TITLE]);
        assert_eq!(
            install_cli_titles(&help_menu_installed),
            [INSTALL_CLI_INSTALLED_TITLE]
        );

        let availability = CommandAction::InstallCli.availability(CommandCapabilities {
            tmux_runtime_active: true,
            install_cli_available: false,
        });
        assert_eq!(
            availability.reason,
            Some(CommandUnavailableReason::InstallCliAlreadyInstalled)
        );
    }

    #[test]
    fn file_menu_shows_tmux_required_pane_actions_when_tmux_is_disabled() {
        let caps = CommandCapabilities {
            tmux_runtime_active: false,
            install_cli_available: true,
        };
        for action in [
            CommandAction::SplitPaneVertical,
            CommandAction::SplitPaneHorizontal,
            CommandAction::ClosePane,
            CommandAction::FocusPaneNext,
        ] {
            let availability = action.availability(caps);
            assert_eq!(
                availability.reason,
                Some(CommandUnavailableReason::RequiresTmuxRuntime)
            );
        }

        let file_menu = app_menus(true, false)
            .into_iter()
            .find(|menu| menu.name.as_ref() == "File")
            .expect("missing File menu");

        let labels = file_menu
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Action { name, .. } => name.as_ref().to_string(),
                MenuItem::Separator => "<separator>".to_string(),
                MenuItem::Submenu(_) | MenuItem::SystemMenu(_) => "<non-action>".to_string(),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "New Tab",
                "Close Tab",
                "Rename Tab",
                "<separator>",
                "tmux Sessions…",
                SPLIT_PANE_VERTICAL_TMUX_REQUIRED_TITLE,
                SPLIT_PANE_HORIZONTAL_TMUX_REQUIRED_TITLE,
                CLOSE_PANE_TMUX_REQUIRED_TITLE,
                FOCUS_NEXT_PANE_TMUX_REQUIRED_TITLE,
            ]
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn tmux_only_actions_outside_requested_file_set_remain_hidden_when_tmux_is_disabled() {
        let all_menu_titles = app_menus(true, false)
            .into_iter()
            .flat_map(|menu| menu.items)
            .filter_map(|item| {
                let MenuItem::Action { name, .. } = item else {
                    return None;
                };
                Some(name.as_ref().to_string())
            })
            .collect::<Vec<_>>();

        assert!(
            !all_menu_titles.iter().any(|title| matches!(
                title.as_str(),
                "Split Pane Vertical"
                    | "Split Pane Horizontal"
                    | "Close Pane"
                    | "Focus Next Pane"
                    | "Focus Previous Pane"
                    | "Toggle Pane Zoom"
            ))
        );
    }
}
