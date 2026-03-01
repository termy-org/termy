use crate::commands::{CommandAction, CommandMenuEntry, MenuRoot};
use gpui::{Menu, MenuItem};
#[cfg(target_os = "macos")]
use gpui::SystemMenuType;

const INSTALL_CLI_TITLE: &str = "Install CLI";
const INSTALL_CLI_INSTALLED_TITLE: &str = "Install CLI (Installed)";

pub(crate) fn app_menus(install_cli_available: bool, tmux_enabled: bool) -> Vec<Menu> {
    CommandAction::menu_roots()
        .iter()
        .copied()
        .map(|root| build_menu(root, install_cli_available, tmux_enabled))
        .collect()
}

fn build_menu(root: MenuRoot, install_cli_available: bool, tmux_enabled: bool) -> Menu {
    let entries = CommandAction::menu_entries_for_root(root);
    let mut items = Vec::new();

    #[cfg(target_os = "macos")]
    if root == MenuRoot::App {
        items.push(MenuItem::os_submenu("Services", SystemMenuType::Services));
        if !entries.is_empty() {
            items.push(MenuItem::separator());
        }
    }

    append_menu_entries(&mut items, &entries, install_cli_available, tmux_enabled);

    Menu {
        name: root.title().into(),
        items,
    }
}

fn append_menu_entries(
    items: &mut Vec<MenuItem>,
    entries: &[CommandMenuEntry],
    install_cli_available: bool,
    tmux_enabled: bool,
) {
    let mut previous_section = None;

    for entry in entries {
        if entry.action.requires_tmux() && !tmux_enabled {
            continue;
        }

        if let Some(section) = previous_section {
            if section != entry.section {
                items.push(MenuItem::separator());
            }
        }

        let title = if entry.action == CommandAction::InstallCli {
            if install_cli_available {
                INSTALL_CLI_TITLE
            } else {
                INSTALL_CLI_INSTALLED_TITLE
            }
        } else {
            entry.title
        };

        items.push(entry.action.to_menu_item(title, entry.role));
        previous_section = Some(entry.section);
    }
}

#[cfg(test)]
mod tests {
    use super::{INSTALL_CLI_INSTALLED_TITLE, INSTALL_CLI_TITLE, app_menus};
    use gpui::{MenuItem, OsAction};

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
    }

    #[test]
    fn tmux_only_actions_are_hidden_when_tmux_is_disabled() {
        let window_menu = app_menus(true, false)
            .into_iter()
            .find(|menu| menu.name.as_ref() == "Window")
            .expect("missing Window menu");
        assert!(
            !window_menu
                .items
                .iter()
                .filter_map(|item| match item {
                    MenuItem::Action { name, .. } => Some(name.as_ref()),
                    _ => None,
                })
                .any(|title| matches!(
                    title,
                    "New Tab"
                        | "Close Tab"
                        | "Move Tab Left"
                        | "Move Tab Right"
                        | "Switch Tab Left"
                        | "Switch Tab Right"
                        | "Split Pane Vertical"
                        | "Split Pane Horizontal"
                        | "Close Pane"
                        | "Rename Tab"
                ))
        );
    }
}
