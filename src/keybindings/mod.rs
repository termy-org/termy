use crate::commands::CommandAction;
use crate::config::AppConfig;
use gpui::App;
#[cfg(debug_assertions)]
use gpui::Keystroke;
use log::warn;
use termy_command_core::{
    KeybindLineRef, KeybindWarning, ResolvedKeybind, default_resolved_keybinds,
    parse_keybind_directives_from_iter, resolve_keybinds,
};

pub fn install_keybindings(cx: &mut App, config: &AppConfig) {
    let (resolved, warnings) = resolve_keybinds_for_config(config);
    report_warnings(&warnings);
    let resolved = reorder_resolved_keybinds_for_menu_display(resolved);

    for binding in &resolved {
        debug_assert_trigger_is_valid_for_gpui(&binding.trigger);
    }

    cx.clear_key_bindings();
    // Keep menu shortcut glyphs in sync with custom keybinds by rebuilding menus
    // only after the canonical keymap bindings are reinstalled.
    cx.bind_keys(resolved.iter().map(|binding| {
        CommandAction::from_command_id(binding.action).to_key_binding(&binding.trigger)
    }));
    cx.bind_keys(crate::commands::inline_input_keybindings());
    cx.set_menus(crate::menus::app_menus());
}

fn reorder_resolved_keybinds_for_menu_display(
    resolved: Vec<ResolvedKeybind>,
) -> Vec<ResolvedKeybind> {
    use std::collections::HashMap;

    if resolved.len() < 2 {
        return resolved;
    }

    let mut last_index_by_action = HashMap::new();
    for (index, binding) in resolved.iter().enumerate() {
        last_index_by_action.insert(binding.action, index);
    }

    let mut primary = Vec::new();
    let mut secondary = Vec::new();
    for (index, binding) in resolved.into_iter().enumerate() {
        if last_index_by_action.get(&binding.action) == Some(&index) {
            primary.push(binding);
        } else {
            secondary.push(binding);
        }
    }

    primary.extend(secondary);
    primary
}

pub(crate) fn resolve_keybinds_for_config(
    config: &AppConfig,
) -> (
    Vec<termy_command_core::ResolvedKeybind>,
    Vec<KeybindWarning>,
) {
    let (directives, warnings) =
        parse_keybind_directives_from_iter(config.keybind_lines.iter().map(|line| {
            KeybindLineRef {
                line_number: line.line_number,
                value: line.value.as_str(),
            }
        }));

    let resolved = resolve_keybinds(default_resolved_keybinds(), &directives);
    (resolved, warnings)
}

fn report_warnings(warnings: &[KeybindWarning]) {
    if warnings.is_empty() {
        return;
    }

    for warning in warnings {
        warn!(
            "Ignoring invalid keybind at config line {}: {}",
            warning.line_number, warning.message
        );
    }

    let noun = if warnings.len() == 1 { "line" } else { "lines" };
    termy_toast::warning(format!(
        "Ignored {} invalid keybind {}",
        warnings.len(),
        noun
    ));
}

#[cfg(debug_assertions)]
fn debug_assert_trigger_is_valid_for_gpui(trigger: &str) {
    for component in trigger.split_whitespace() {
        debug_assert!(
            Keystroke::parse(component).is_ok(),
            "command_core emitted unsupported GPUI keybind trigger component `{}` from `{}`",
            component,
            trigger
        );
    }
}

#[cfg(not(debug_assertions))]
fn debug_assert_trigger_is_valid_for_gpui(_trigger: &str) {}

#[cfg(test)]
mod tests {
    use super::{reorder_resolved_keybinds_for_menu_display, resolve_keybinds_for_config};
    use crate::config::AppConfig;
    use termy_command_core::{
        CommandId, KeybindLineRef, ResolvedKeybind, default_resolved_keybinds,
        parse_keybind_directives_from_iter, resolve_keybinds,
    };
    use termy_config_core::KeybindConfigLine;

    fn fixture_keybind_lines() -> Vec<KeybindConfigLine> {
        vec![
            KeybindConfigLine {
                line_number: 1,
                value: "Secondary-P=toggle_command_palette".to_string(),
            },
            KeybindConfigLine {
                line_number: 2,
                value: "Control-Shift-C=copy".to_string(),
            },
            KeybindConfigLine {
                line_number: 3,
                value: "cmd-=zoom_in".to_string(),
            },
            KeybindConfigLine {
                line_number: 4,
                value: "secondary-p=unbind".to_string(),
            },
        ]
    }

    #[test]
    fn resolved_keybinds_match_command_core_for_same_fixture() {
        let mut config = AppConfig::default();
        config.keybind_lines = fixture_keybind_lines();

        let (resolved_from_gui, warnings) = resolve_keybinds_for_config(&config);
        assert!(warnings.is_empty());

        let (directives, core_warnings) =
            parse_keybind_directives_from_iter(config.keybind_lines.iter().map(|line| {
                KeybindLineRef {
                    line_number: line.line_number,
                    value: line.value.as_str(),
                }
            }));
        assert!(core_warnings.is_empty());

        let resolved_from_core = resolve_keybinds(default_resolved_keybinds(), &directives);
        assert_eq!(resolved_from_gui, resolved_from_core);
    }

    #[test]
    fn resolved_keybinds_use_canonicalized_triggers() {
        let mut config = AppConfig::default();
        config.keybind_lines = fixture_keybind_lines();

        let (resolved, warnings) = resolve_keybinds_for_config(&config);
        assert!(warnings.is_empty());
        assert!(
            resolved
                .iter()
                .any(|binding| binding.trigger == "ctrl-shift-c"
                    && binding.action.config_name() == "copy")
        );
        assert!(
            resolved
                .iter()
                .any(|binding| binding.trigger == "cmd-="
                    && binding.action.config_name() == "zoom_in")
        );
        assert!(
            resolved
                .iter()
                .all(|binding| !(binding.trigger == "secondary-p"
                    && binding.action.config_name() == "toggle_command_palette"))
        );
    }

    #[test]
    fn reorder_keybinds_promotes_latest_binding_per_action() {
        let resolved = vec![
            ResolvedKeybind {
                trigger: "secondary-c".to_string(),
                action: CommandId::Copy,
            },
            ResolvedKeybind {
                trigger: "secondary-v".to_string(),
                action: CommandId::Paste,
            },
            ResolvedKeybind {
                trigger: "ctrl-shift-c".to_string(),
                action: CommandId::Copy,
            },
        ];

        let reordered = reorder_resolved_keybinds_for_menu_display(resolved);
        let first_copy = reordered
            .iter()
            .find(|binding| binding.action == CommandId::Copy)
            .expect("missing copy binding");
        let first_paste = reordered
            .iter()
            .find(|binding| binding.action == CommandId::Paste)
            .expect("missing paste binding");

        assert_eq!(first_copy.trigger, "ctrl-shift-c");
        assert_eq!(first_paste.trigger, "secondary-v");
        assert!(reordered.iter().any(|binding| binding.trigger == "secondary-c"));
    }

    #[test]
    fn reorder_keybinds_keeps_single_action_bindings_unchanged() {
        let resolved = vec![ResolvedKeybind {
            trigger: "secondary-t".to_string(),
            action: CommandId::NewTab,
        }];

        let reordered = reorder_resolved_keybinds_for_menu_display(resolved.clone());
        assert_eq!(reordered, resolved);
    }
}
