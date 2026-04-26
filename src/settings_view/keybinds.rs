use super::*;
use std::collections::HashMap;

impl SettingsWindow {
    pub(super) fn bindable_actions() -> Vec<CommandId> {
        termy_command_core::command_specs()
            .iter()
            .map(|spec| spec.id)
            .collect()
    }

    pub(super) fn action_title_from_config_name(config_name: &str) -> String {
        config_name
            .split('_')
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                let mut chars = segment.chars();
                match chars.next() {
                    Some(first) => {
                        let mut title = String::with_capacity(segment.len());
                        title.push(first.to_ascii_uppercase());
                        title.extend(chars);
                        title
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn collapse_resolved_keybinds_to_single_binding(
        resolved: &[termy_command_core::ResolvedKeybind],
    ) -> HashMap<CommandId, String> {
        let mut bindings = HashMap::with_capacity(resolved.len());
        for binding in resolved {
            bindings.insert(binding.action, binding.trigger.clone());
        }
        bindings
    }

    pub(super) fn effective_action_bindings_from_lines(
        lines: &[termy_config_core::KeybindConfigLine],
    ) -> HashMap<CommandId, String> {
        let (directives, _warnings) =
            termy_command_core::parse_keybind_directives_from_iter(lines.iter().map(|line| {
                termy_command_core::KeybindLineRef {
                    line_number: line.line_number,
                    value: line.value.as_str(),
                }
            }));
        let resolved = termy_command_core::resolve_keybinds(
            termy_command_core::default_resolved_keybinds(),
            &directives,
        );
        Self::collapse_resolved_keybinds_to_single_binding(&resolved)
    }

    fn effective_action_bindings(&self) -> HashMap<CommandId, String> {
        Self::effective_action_bindings_from_lines(&self.config.keybind_lines)
    }

    fn serialize_structured_keybind_lines(bindings: &HashMap<CommandId, String>) -> Vec<String> {
        let mut entries = bindings
            .iter()
            .map(|(action, trigger)| (action.config_name(), trigger.clone()))
            .collect::<Vec<_>>();
        entries
            .sort_unstable_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(&right.1)));

        let mut lines = Vec::with_capacity(entries.len() + 1);
        lines.push("clear".to_string());
        for (action_name, trigger) in entries {
            lines.push(format!("{}={}", trigger, action_name));
        }
        lines
    }

    fn persist_action_bindings(
        &mut self,
        bindings: &HashMap<CommandId, String>,
    ) -> Result<(), String> {
        let lines = Self::serialize_structured_keybind_lines(bindings);
        config::set_keybind_lines(&lines)?;
        self.config.keybind_lines = lines
            .into_iter()
            .enumerate()
            .map(|(index, value)| termy_config_core::KeybindConfigLine {
                line_number: index + 1,
                value,
            })
            .collect();
        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn reset_keybinds_to_defaults(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = config::set_keybind_lines(&[]) {
            termy_toast::error(error);
            return;
        }

        self.config.keybind_lines.clear();
        self.capturing_action = None;
        termy_toast::success("Saved");
        cx.notify();
    }

    pub(super) fn clear_action_binding(&mut self, action: CommandId, cx: &mut Context<Self>) {
        let mut bindings = self.effective_action_bindings();
        bindings.remove(&action);
        if let Err(error) = self.persist_action_bindings(&bindings) {
            termy_toast::error(error);
            return;
        }
        self.capturing_action = None;
        termy_toast::success("Saved");
        cx.notify();
    }

    fn assign_action_binding(&mut self, action: CommandId, trigger: &str, cx: &mut Context<Self>) {
        let mut bindings = self.effective_action_bindings();
        Self::apply_assignment_with_conflict_resolution(&mut bindings, action, trigger);

        if let Err(error) = self.persist_action_bindings(&bindings) {
            termy_toast::error(error);
            return;
        }

        self.capturing_action = None;
        termy_toast::success("Saved");
        cx.notify();
    }

    fn apply_assignment_with_conflict_resolution(
        bindings: &mut HashMap<CommandId, String>,
        action: CommandId,
        trigger: &str,
    ) {
        bindings.retain(|existing_action, existing_trigger| {
            *existing_action == action || existing_trigger != trigger
        });
        bindings.insert(action, trigger.to_string());
    }

    fn begin_action_binding_capture(
        &mut self,
        action: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.capturing_action = Some(action);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn has_no_modifiers(modifiers: crate::gpui::Modifiers) -> bool {
        !modifiers.control
            && !modifiers.alt
            && !modifiers.shift
            && !modifiers.platform
            && !modifiers.function
    }

    fn is_modifier_only_key(key: &str) -> bool {
        matches!(
            key,
            "shift"
                | "control"
                | "ctrl"
                | "alt"
                | "option"
                | "command"
                | "cmd"
                | "super"
                | "meta"
                | "fn"
                | "function"
                | "secondary"
        )
    }

    fn canonicalize_captured_trigger(
        key: &str,
        modifiers: crate::gpui::Modifiers,
    ) -> Result<Option<String>, String> {
        let normalized_key = key.trim().to_ascii_lowercase();
        if normalized_key.is_empty() || Self::is_modifier_only_key(&normalized_key) {
            return Ok(None);
        }

        let mut parts = Vec::new();
        let secondary = modifiers.secondary();
        if secondary {
            parts.push("secondary");
        }
        if modifiers.control && !secondary {
            parts.push("ctrl");
        }
        if modifiers.alt {
            parts.push("alt");
        }
        if modifiers.shift {
            parts.push("shift");
        }
        if modifiers.platform && !secondary {
            parts.push("cmd");
        }
        if modifiers.function {
            parts.push("fn");
        }

        let raw = if parts.is_empty() {
            normalized_key
        } else {
            format!("{}-{}", parts.join("-"), normalized_key)
        };

        termy_command_core::canonicalize_keybind_trigger(&raw).map(Some)
    }

    fn secondary_display_label() -> &'static str {
        if cfg!(target_os = "macos") {
            "CMD"
        } else {
            "CTRL"
        }
    }

    fn modifier_display_label(modifier: &str) -> String {
        match modifier {
            "secondary" => Self::secondary_display_label().to_string(),
            "ctrl" => "CTRL".to_string(),
            "alt" => {
                if cfg!(target_os = "macos") {
                    "OPT".to_string()
                } else {
                    "ALT".to_string()
                }
            }
            "shift" => "SHIFT".to_string(),
            "cmd" => "CMD".to_string(),
            "fn" => "FN".to_string(),
            other => other.to_ascii_uppercase(),
        }
    }

    fn key_display_label(key: &str) -> String {
        match key {
            "space" => "SPACE".to_string(),
            "enter" => "ENTER".to_string(),
            "escape" => "ESC".to_string(),
            "tab" => "TAB".to_string(),
            "backspace" => "BACKSPACE".to_string(),
            "delete" => "DELETE".to_string(),
            "home" => "HOME".to_string(),
            "end" => "END".to_string(),
            "pageup" => "PAGE UP".to_string(),
            "pagedown" => "PAGE DOWN".to_string(),
            "left" => "LEFT".to_string(),
            "right" => "RIGHT".to_string(),
            "up" => "UP".to_string(),
            "down" => "DOWN".to_string(),
            other if other.len() == 1 => other.to_ascii_uppercase(),
            other => other.to_ascii_uppercase(),
        }
    }

    pub(super) fn display_trigger_for_os(trigger: &str) -> String {
        let displayed = trigger
            .split_whitespace()
            .filter(|component| !component.is_empty())
            .filter_map(|component| {
                let parts = component
                    .split('-')
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>();
                let (key, modifiers) = parts.split_last()?;
                let key = Self::key_display_label(key);
                if modifiers.is_empty() {
                    return Some(key);
                }
                let modifiers = modifiers
                    .iter()
                    .map(|modifier| Self::modifier_display_label(modifier))
                    .collect::<Vec<_>>()
                    .join(" + ");
                Some(format!("{} + {}", modifiers, key))
            })
            .collect::<Vec<_>>();

        if displayed.is_empty() {
            trigger.to_string()
        } else {
            displayed.join(" then ")
        }
    }

    pub(super) fn handle_keybind_capture(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(action) = self.capturing_action else {
            return;
        };

        let key = event.keystroke.key.as_str();
        let no_modifiers = Self::has_no_modifiers(event.keystroke.modifiers);
        if no_modifiers && key.eq_ignore_ascii_case("escape") {
            self.capturing_action = None;
            cx.notify();
            return;
        }

        if no_modifiers
            && (key.eq_ignore_ascii_case("backspace") || key.eq_ignore_ascii_case("delete"))
        {
            self.clear_action_binding(action, cx);
            return;
        }

        match Self::canonicalize_captured_trigger(key, event.keystroke.modifiers) {
            Ok(Some(trigger)) => self.assign_action_binding(action, &trigger, cx),
            Ok(None) => {}
            Err(error) => termy_toast::error(format!("Invalid key combo: {}", error)),
        }
    }

}

#[cfg(test)]
mod tests {
    use super::SettingsWindow;
    use std::collections::HashMap;
    use termy_command_core::{CommandId, ResolvedKeybind, command_specs};

    #[test]
    fn bindable_actions_match_command_catalog() {
        let actions = SettingsWindow::bindable_actions();
        let expected = command_specs()
            .iter()
            .map(|spec| spec.id)
            .collect::<Vec<_>>();
        assert_eq!(actions, expected);
    }

    #[test]
    fn collapse_resolved_keybinds_keeps_last_binding_per_action() {
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

        let collapsed = SettingsWindow::collapse_resolved_keybinds_to_single_binding(&resolved);
        assert_eq!(
            collapsed.get(&CommandId::Copy),
            Some(&"ctrl-shift-c".to_string())
        );
        assert_eq!(
            collapsed.get(&CommandId::Paste),
            Some(&"secondary-v".to_string())
        );
        assert_eq!(collapsed.len(), 2);
    }

    #[test]
    fn serialize_structured_keybind_lines_is_deterministic_and_includes_clear() {
        let mut bindings = HashMap::new();
        bindings.insert(CommandId::Paste, "secondary-v".to_string());
        bindings.insert(CommandId::Copy, "secondary-c".to_string());

        let lines = SettingsWindow::serialize_structured_keybind_lines(&bindings);
        assert_eq!(lines[0], "clear");
        assert_eq!(lines[1], "secondary-c=copy");
        assert_eq!(lines[2], "secondary-v=paste");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn assignment_conflict_moves_trigger_to_new_action() {
        let mut bindings = HashMap::new();
        bindings.insert(CommandId::Copy, "secondary-c".to_string());
        bindings.insert(CommandId::Paste, "secondary-v".to_string());

        SettingsWindow::apply_assignment_with_conflict_resolution(
            &mut bindings,
            CommandId::Paste,
            "secondary-c",
        );

        assert_eq!(
            bindings.get(&CommandId::Paste),
            Some(&"secondary-c".to_string())
        );
        assert!(!bindings.contains_key(&CommandId::Copy));
    }

    #[test]
    fn canonicalize_captured_trigger_supports_modifier_combos() {
        let modifiers = crate::gpui::Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        };
        let trigger = SettingsWindow::canonicalize_captured_trigger("C", modifiers)
            .expect("should canonicalize")
            .expect("should produce trigger");
        assert_eq!(trigger, "alt-shift-c");
    }

    #[test]
    fn canonicalize_captured_trigger_ignores_modifier_only_keys() {
        let modifiers = crate::gpui::Modifiers {
            shift: true,
            ..Default::default()
        };
        let trigger = SettingsWindow::canonicalize_captured_trigger("shift", modifiers)
            .expect("modifier-only should not fail");
        assert!(trigger.is_none());
    }

    #[test]
    fn display_trigger_for_os_maps_secondary_to_native_modifier_label() {
        let rendered = SettingsWindow::display_trigger_for_os("secondary-n");
        #[cfg(target_os = "macos")]
        assert_eq!(rendered, "CMD + N");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(rendered, "CTRL + N");
    }
}
