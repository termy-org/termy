use super::state::{CommandPaletteItem, CommandPaletteItemKind, CommandPaletteState};

const SAVED_LAYOUT_NAME_REQUIRED_HINT: &str = "name required";
const SAVED_LAYOUT_NAME_UNCHANGED_HINT: &str = "unchanged";
const SAVED_LAYOUT_LIVE_HINT: &str = "live autosave";
const SAVED_LAYOUT_OPEN_SAVE_MODE_TITLE: &str = "Save Current Layout…";
const SAVED_LAYOUT_OPEN_RENAME_MODE_TITLE: &str = "Rename Saved Layout…";
const SAVED_LAYOUT_OPEN_DELETE_MODE_TITLE: &str = "Delete Saved Layout…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum SavedLayoutIntent {
    Browse,
    SaveInput,
    RenameSelect,
    RenameInput,
    Delete,
}

impl CommandPaletteItem {
    pub(super) fn saved_layout_open(layout_name: &str, is_live: bool) -> Self {
        Self {
            title: layout_name.to_string(),
            keywords: format!(
                "saved layout load open restore {}",
                layout_name.replace('-', " ")
            ),
            enabled: true,
            status_hint: is_live.then_some(SAVED_LAYOUT_LIVE_HINT),
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutOpen {
                layout_name: layout_name.to_string(),
            },
        }
    }

    pub(super) fn saved_layout_open_save_mode() -> Self {
        Self {
            title: SAVED_LAYOUT_OPEN_SAVE_MODE_TITLE.to_string(),
            keywords: "saved layout save current".to_string(),
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutOpenSaveMode,
        }
    }

    pub(super) fn saved_layout_save_as(layout_name: &str) -> Self {
        let trimmed = layout_name.trim();
        let exists_title = if trimmed.is_empty() {
            "Save Current Layout".to_string()
        } else {
            format!("Save Current Layout as \"{}\"", trimmed)
        };
        let enabled = !trimmed.is_empty();
        Self {
            title: exists_title,
            keywords: format!("saved layout save {}", trimmed.replace('-', " ")),
            enabled,
            status_hint: (!enabled).then_some(SAVED_LAYOUT_NAME_REQUIRED_HINT),
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutSaveAs {
                layout_name: trimmed.to_string(),
            },
        }
    }

    pub(super) fn saved_layout_open_rename_mode() -> Self {
        Self {
            title: SAVED_LAYOUT_OPEN_RENAME_MODE_TITLE.to_string(),
            keywords: "saved layout rename".to_string(),
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutOpenRenameMode,
        }
    }

    pub(super) fn saved_layout_rename_select(layout_name: &str) -> Self {
        Self {
            title: layout_name.to_string(),
            keywords: format!("saved layout rename {}", layout_name.replace('-', " ")),
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutRenameSelect {
                layout_name: layout_name.to_string(),
            },
        }
    }

    pub(super) fn saved_layout_rename_apply(
        current_layout_name: &str,
        next_layout_name: &str,
    ) -> Self {
        let next_layout_name = next_layout_name.trim().to_string();
        let mut enabled = true;
        let mut status_hint = None;
        if next_layout_name.is_empty() {
            enabled = false;
            status_hint = Some(SAVED_LAYOUT_NAME_REQUIRED_HINT);
        } else if current_layout_name.eq_ignore_ascii_case(&next_layout_name) {
            enabled = false;
            status_hint = Some(SAVED_LAYOUT_NAME_UNCHANGED_HINT);
        }

        let rendered_next = if next_layout_name.is_empty() {
            "<new name>"
        } else {
            next_layout_name.as_str()
        };
        Self {
            title: format!(
                "Rename \"{}\" -> \"{}\"",
                current_layout_name, rendered_next
            ),
            keywords: format!(
                "saved layout rename {} {}",
                current_layout_name.replace('-', " "),
                next_layout_name.replace('-', " ")
            ),
            enabled,
            status_hint,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutRenameApply {
                current_layout_name: current_layout_name.to_string(),
                next_layout_name,
            },
        }
    }

    pub(super) fn saved_layout_open_delete_mode() -> Self {
        Self {
            title: SAVED_LAYOUT_OPEN_DELETE_MODE_TITLE.to_string(),
            keywords: "saved layout delete remove".to_string(),
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutOpenDeleteMode,
        }
    }

    pub(super) fn saved_layout_delete(layout_name: &str) -> Self {
        Self {
            title: format!("Delete \"{}\"", layout_name),
            keywords: format!("saved layout delete {}", layout_name.replace('-', " ")),
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutDelete {
                layout_name: layout_name.to_string(),
            },
        }
    }
}

impl CommandPaletteState {
    pub(super) fn saved_layout_intent(&self) -> SavedLayoutIntent {
        self.saved_layout_intent
    }

    pub(super) fn set_saved_layout_intent(&mut self, intent: SavedLayoutIntent) {
        self.saved_layout_intent = intent;
        if intent != SavedLayoutIntent::RenameInput {
            self.saved_layout_rename_source = None;
        }
    }

    pub(super) fn begin_saved_layout_rename(&mut self, layout_name: &str) {
        self.saved_layout_intent = SavedLayoutIntent::RenameInput;
        self.saved_layout_rename_source = Some(layout_name.to_string());
        self.input_mut().clear();
    }

    pub(super) fn back_from_saved_layout_rename_input(&mut self) -> bool {
        if self.saved_layout_intent != SavedLayoutIntent::RenameInput {
            return false;
        }
        self.saved_layout_intent = SavedLayoutIntent::RenameSelect;
        self.saved_layout_rename_source = None;
        self.input_mut().clear();
        true
    }

    pub(super) fn saved_layout_rename_source(&self) -> Option<&str> {
        self.saved_layout_rename_source.as_deref()
    }

    pub(super) fn set_saved_layout_names(
        &mut self,
        mut names: Vec<String>,
        live_name: Option<String>,
        autosave_enabled: bool,
    ) {
        names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
        names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        self.saved_layout_names = names;
        self.saved_layout_live_name = live_name;
        self.saved_layout_autosave_enabled = autosave_enabled;
    }

    pub(super) fn saved_layout_items_for_query(&self, query: &str) -> Vec<CommandPaletteItem> {
        match self.saved_layout_intent {
            SavedLayoutIntent::Browse => {
                let mut items = self
                    .saved_layout_names
                    .iter()
                    .map(|name| {
                        let is_live = self.saved_layout_autosave_enabled
                            && self
                                .saved_layout_live_name
                                .as_deref()
                                .is_some_and(|current| current.eq_ignore_ascii_case(name));
                        CommandPaletteItem::saved_layout_open(name, is_live)
                    })
                    .collect::<Vec<_>>();
                let query = query.trim();
                if query.is_empty() {
                    let mut utility_items = vec![CommandPaletteItem::saved_layout_open_save_mode()];
                    if !self.saved_layout_names.is_empty() {
                        utility_items.push(CommandPaletteItem::saved_layout_open_rename_mode());
                        utility_items.push(CommandPaletteItem::saved_layout_open_delete_mode());
                    }
                    utility_items.extend(items);
                    return utility_items;
                }

                let exact_match = self
                    .saved_layout_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(query));
                if !exact_match {
                    items.push(CommandPaletteItem::saved_layout_save_as(query));
                }
                items
            }
            SavedLayoutIntent::SaveInput => {
                vec![CommandPaletteItem::saved_layout_save_as(query)]
            }
            SavedLayoutIntent::RenameSelect => self
                .saved_layout_names
                .iter()
                .map(|name| CommandPaletteItem::saved_layout_rename_select(name))
                .collect(),
            SavedLayoutIntent::RenameInput => {
                let Some(current_layout_name) = self.saved_layout_rename_source() else {
                    return Vec::new();
                };
                vec![CommandPaletteItem::saved_layout_rename_apply(
                    current_layout_name,
                    query,
                )]
            }
            SavedLayoutIntent::Delete => self
                .saved_layout_names
                .iter()
                .map(|name| CommandPaletteItem::saved_layout_delete(name))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_view::command_palette::state::CommandPaletteState;

    #[test]
    fn browse_items_mark_live_layout_when_autosave_is_enabled() {
        let mut state = CommandPaletteState::new(false);
        state.set_saved_layout_names(
            vec!["Main".to_string(), "Scratch".to_string()],
            Some("Main".to_string()),
            true,
        );

        let items = state.saved_layout_items_for_query("");
        let main = items
            .iter()
            .find(|item| matches!(
                item.kind,
                CommandPaletteItemKind::SavedLayoutOpen { ref layout_name } if layout_name == "Main"
            ))
            .expect("missing main layout item");

        assert_eq!(main.status_hint, Some("live autosave"));
    }
}
