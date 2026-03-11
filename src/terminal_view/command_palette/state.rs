use super::super::*;
use super::state_layouts::SavedLayoutIntent;
use super::state_tmux::{TmuxSessionIntent, TmuxSessionRow, TmuxSessionStatusHint};
use crate::config::SHELL_DECIDE_THEME_ID;
use gpui::UniformListScrollHandle;
use std::collections::HashMap;
use termy_terminal_ui::TmuxSocketTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum CommandPaletteMode {
    Commands,
    Themes,
    TmuxSessions,
    Layouts,
    Tasks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum TaskIntent {
    Browse,
    CreateGlobalInput,
    CreateLayoutInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum CommandPaletteCommandIntent {
    Browse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CommandPaletteItemKind {
    Command(CommandAction),
    PluginCommand {
        plugin_id: String,
        command_id: String,
    },
    Theme(String),
    TmuxSessionAttachOrSwitch {
        session_name: String,
        socket_target: TmuxSocketTarget,
    },
    TmuxSessionCreateAndAttach {
        session_name: String,
        socket_target: TmuxSocketTarget,
    },
    TmuxSessionDetachCurrent,
    TmuxSessionOpenRenameMode,
    TmuxSessionOpenKillMode,
    TmuxSessionRenameSelect {
        session_name: String,
        socket_target: TmuxSocketTarget,
    },
    TmuxSessionRenameApply {
        current_session_name: String,
        next_session_name: String,
        socket_target: TmuxSocketTarget,
    },
    TmuxSessionKill {
        session_name: String,
        socket_target: TmuxSocketTarget,
    },
    SavedLayoutOpen {
        layout_name: String,
    },
    SavedLayoutOpenTasksMode {
        layout_name: String,
    },
    SavedLayoutOpenSaveMode,
    SavedLayoutSaveAs {
        layout_name: String,
    },
    SavedLayoutOpenRenameMode,
    SavedLayoutRenameSelect {
        layout_name: String,
    },
    SavedLayoutRenameApply {
        current_layout_name: String,
        next_layout_name: String,
    },
    SavedLayoutOpenDeleteMode,
    SavedLayoutDelete {
        layout_name: String,
    },
    TaskOpenCreateGlobalMode,
    TaskOpenCreateLayoutMode {
        layout_name: String,
    },
    TaskOpenSaveCurrentCommandGlobalMode,
    TaskOpenSaveCurrentCommandLayoutMode {
        layout_name: String,
    },
    TaskCreate {
        task_name: String,
        command: String,
        layout_name: Option<String>,
    },
    Task {
        task_name: String,
        command: String,
        working_dir: Option<String>,
        layout_name: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CommandPaletteItem {
    pub(super) title: String,
    pub(super) keywords: String,
    pub(super) enabled: bool,
    pub(super) status_hint: Option<&'static str>,
    pub(super) tmux_status_hint: Option<TmuxSessionStatusHint>,
    pub(super) kind: CommandPaletteItemKind,
}

impl CommandPaletteItem {
    pub(super) fn command_with_state(
        title: &str,
        keywords: &str,
        action: CommandAction,
        enabled: bool,
        status_hint: Option<&'static str>,
    ) -> Self {
        Self {
            title: title.to_string(),
            keywords: keywords.to_string(),
            enabled,
            status_hint,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::Command(action),
        }
    }

    pub(super) fn theme(theme_id: String, is_active: bool) -> Self {
        let title = if is_active {
            format!("\u{2713} {}", theme_id)
        } else {
            theme_id.clone()
        };
        let keywords = format!("theme palette colors {}", theme_id.replace('-', " "));

        Self {
            title,
            keywords,
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::Theme(theme_id),
        }
    }

    pub(super) fn plugin_command(
        title: String,
        keywords: String,
        plugin_id: String,
        command_id: String,
        enabled: bool,
        status_hint: Option<&'static str>,
    ) -> Self {
        Self {
            title,
            keywords,
            enabled,
            status_hint,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::PluginCommand {
                plugin_id,
                command_id,
            },
        }
    }

    pub(super) fn task(
        task_name: &str,
        command: &str,
        working_dir: Option<&str>,
        layout_name: Option<&str>,
    ) -> Self {
        let mut keywords = format!(
            "task run command {} {}",
            task_name.replace(['-', '_'], " "),
            command
        );
        if let Some(layout_name) = layout_name {
            keywords.push(' ');
            keywords.push_str(&layout_name.replace(['-', '_'], " "));
        }
        if let Some(working_dir) = working_dir {
            keywords.push(' ');
            keywords.push_str(working_dir);
        }

        let title = match layout_name {
            Some(layout_name) => format!("{task_name} [{layout_name}]"),
            None => task_name.to_string(),
        };

        Self {
            title,
            keywords,
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::Task {
                task_name: task_name.to_string(),
                command: command.to_string(),
                working_dir: working_dir.map(ToOwned::to_owned),
                layout_name: layout_name.map(ToOwned::to_owned),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(in super::super) struct CommandPaletteState {
    open: bool,
    mode: CommandPaletteMode,
    pub(super) command_intent: CommandPaletteCommandIntent,
    pub(super) tmux_session_intent: TmuxSessionIntent,
    pub(super) saved_layout_intent: SavedLayoutIntent,
    pub(super) task_intent: TaskIntent,
    pub(super) tmux_rename_source_session: Option<String>,
    pub(super) tmux_rename_source_socket: Option<TmuxSocketTarget>,
    pub(super) saved_layout_rename_source: Option<String>,
    input: InlineInputState,
    items: Vec<CommandPaletteItem>,
    filtered_indices: Vec<usize>,
    selected_filtered_index: usize,
    scroll_handle: UniformListScrollHandle,
    scroll_target_y: Option<f32>,
    scroll_max_y: f32,
    scroll_animating: bool,
    scroll_last_tick: Option<Instant>,
    show_keybinds: bool,
    shortcut_cache: HashMap<CommandAction, Option<String>>,
    pub(super) tmux_session_rows: Vec<TmuxSessionRow>,
    pub(super) tmux_create_socket_target: TmuxSocketTarget,
    pub(super) saved_layout_names: Vec<String>,
    pub(super) saved_layout_live_name: Option<String>,
    pub(super) saved_layout_autosave_enabled: bool,
}

impl CommandPaletteState {
    pub(in super::super) fn new(show_keybinds: bool) -> Self {
        Self {
            open: false,
            mode: CommandPaletteMode::Commands,
            command_intent: CommandPaletteCommandIntent::Browse,
            tmux_session_intent: TmuxSessionIntent::AttachOrSwitch,
            saved_layout_intent: SavedLayoutIntent::Browse,
            task_intent: TaskIntent::Browse,
            tmux_rename_source_session: None,
            tmux_rename_source_socket: None,
            saved_layout_rename_source: None,
            input: InlineInputState::new(String::new()),
            items: Vec::new(),
            filtered_indices: Vec::new(),
            selected_filtered_index: 0,
            scroll_handle: UniformListScrollHandle::new(),
            scroll_target_y: None,
            scroll_max_y: 0.0,
            scroll_animating: false,
            scroll_last_tick: None,
            show_keybinds,
            shortcut_cache: HashMap::new(),
            tmux_session_rows: Vec::new(),
            tmux_create_socket_target: TmuxSocketTarget::Default,
            saved_layout_names: Vec::new(),
            saved_layout_live_name: None,
            saved_layout_autosave_enabled: false,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn mode(&self) -> CommandPaletteMode {
        self.mode
    }

    pub(super) fn open(&mut self, mode: CommandPaletteMode) {
        self.open = true;
        self.set_mode(mode);
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.mode = CommandPaletteMode::Commands;
        self.reset_for_mode();
    }

    pub(super) fn set_mode(&mut self, mode: CommandPaletteMode) {
        self.mode = mode;
        self.reset_for_mode();
    }

    pub(super) fn set_show_keybinds(&mut self, show_keybinds: bool) {
        self.show_keybinds = show_keybinds;
    }

    pub(super) fn show_keybinds(&self) -> bool {
        self.show_keybinds
    }

    pub(super) fn input(&self) -> &InlineInputState {
        &self.input
    }

    pub(super) fn input_mut(&mut self) -> &mut InlineInputState {
        &mut self.input
    }

    pub(super) fn set_items(&mut self, items: Vec<CommandPaletteItem>) {
        self.items = items;
        self.refilter_current_query();
    }

    pub(super) fn command_intent(&self) -> CommandPaletteCommandIntent {
        self.command_intent
    }

    pub(super) fn task_intent(&self) -> TaskIntent {
        self.task_intent
    }

    pub(super) fn set_task_intent(&mut self, intent: TaskIntent) {
        self.task_intent = intent;
    }

    pub(super) fn cached_shortcut(&self, action: CommandAction) -> Option<Option<String>> {
        self.shortcut_cache.get(&action).cloned()
    }

    pub(super) fn cache_shortcut(&mut self, action: CommandAction, shortcut: Option<String>) {
        self.shortcut_cache.insert(action, shortcut);
    }

    pub(super) fn clear_shortcut_cache(&mut self) {
        self.shortcut_cache.clear();
    }

    pub(super) fn refilter_current_query(&mut self) {
        let filtered_indices =
            filter_command_palette_item_indices_by_query(&self.items, self.input.text());
        self.filtered_indices = filtered_indices;
        self.clamp_selection();
    }

    pub(super) fn filtered_len(&self) -> usize {
        self.filtered_indices.len()
    }

    pub(super) fn filtered_item(&self, filtered_index: usize) -> Option<&CommandPaletteItem> {
        let item_index = *self.filtered_indices.get(filtered_index)?;
        self.items.get(item_index)
    }

    pub(super) fn selected_filtered_index(&self) -> Option<usize> {
        let len = self.filtered_len();
        if len == 0 {
            None
        } else {
            Some(self.selected_filtered_index.min(len - 1))
        }
    }

    pub(super) fn set_selected_filtered_index(&mut self, index: usize) -> bool {
        let len = self.filtered_len();
        if len == 0 {
            self.selected_filtered_index = 0;
            return false;
        }

        let clamped = index.min(len - 1);
        let changed = self.selected_filtered_index != clamped;
        self.selected_filtered_index = clamped;
        changed
    }

    pub(super) fn move_selection_up(&mut self) -> bool {
        let Some(selected) = self.selected_filtered_index() else {
            return false;
        };
        if selected == 0 {
            return false;
        }
        self.set_selected_filtered_index(selected - 1)
    }

    pub(super) fn move_selection_down(&mut self) -> bool {
        let Some(selected) = self.selected_filtered_index() else {
            return false;
        };
        let len = self.filtered_len();
        if selected + 1 >= len {
            return false;
        }
        self.set_selected_filtered_index(selected + 1)
    }

    pub(super) fn base_scroll_handle(&self) -> gpui::ScrollHandle {
        self.scroll_handle.0.borrow().base_handle.clone()
    }

    pub(super) fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    pub(super) fn scroll_target_y(&self) -> Option<f32> {
        self.scroll_target_y
    }

    pub(super) fn set_scroll_target_y(&mut self, target: f32) {
        self.scroll_target_y = Some(target);
    }

    pub(super) fn clear_scroll_target_y(&mut self) {
        self.scroll_target_y = None;
    }

    pub(super) fn scroll_max_y(&self) -> f32 {
        self.scroll_max_y
    }

    pub(super) fn set_scroll_max_y_for_count(&mut self, item_count: usize) {
        self.scroll_max_y = command_palette_max_scroll_for_count(item_count);
    }

    pub(super) fn is_scroll_animating(&self) -> bool {
        self.scroll_animating
    }

    pub(super) fn start_scroll_animation(&mut self, now: Instant) {
        self.scroll_animating = true;
        self.scroll_last_tick = Some(now);
    }

    pub(super) fn stop_scroll_animation(&mut self) {
        self.scroll_animating = false;
        self.scroll_last_tick = None;
    }

    pub(super) fn scroll_dt_seconds(&mut self, now: Instant) -> f32 {
        let dt = self
            .scroll_last_tick
            .map(|last| (now - last).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        self.scroll_last_tick = Some(now);
        dt
    }

    pub(super) fn reset_scroll_animation_state(&mut self) {
        self.clear_scroll_target_y();
        self.scroll_max_y = 0.0;
        self.stop_scroll_animation();
    }

    pub(super) fn clamp_selection(&mut self) {
        let len = self.filtered_len();
        if len == 0 {
            self.selected_filtered_index = 0;
        } else if self.selected_filtered_index >= len {
            self.selected_filtered_index = len - 1;
        }
    }

    fn reset_for_mode(&mut self) {
        self.input.clear();
        self.items.clear();
        self.filtered_indices.clear();
        self.selected_filtered_index = 0;
        self.scroll_handle = UniformListScrollHandle::new();
        self.shortcut_cache.clear();
        self.reset_scroll_animation_state();
        if self.mode != CommandPaletteMode::TmuxSessions {
            self.tmux_session_intent = TmuxSessionIntent::AttachOrSwitch;
            self.tmux_rename_source_session = None;
            self.tmux_rename_source_socket = None;
        }
        if self.mode != CommandPaletteMode::Commands {
            self.command_intent = CommandPaletteCommandIntent::Browse;
        }
        if self.mode != CommandPaletteMode::Layouts {
            self.saved_layout_intent = SavedLayoutIntent::Browse;
            self.saved_layout_rename_source = None;
        }
        if self.mode != CommandPaletteMode::Tasks {
            self.task_intent = TaskIntent::Browse;
        }
    }
}

pub(super) fn ordered_theme_ids_for_palette(
    mut theme_ids: Vec<String>,
    current_theme: &str,
) -> Vec<String> {
    if !theme_ids.iter().any(|theme| theme == SHELL_DECIDE_THEME_ID) {
        theme_ids.push(SHELL_DECIDE_THEME_ID.to_string());
    }

    if !theme_ids.iter().any(|theme| theme == current_theme) {
        theme_ids.push(current_theme.to_string());
    }

    theme_ids.sort_unstable();
    theme_ids.dedup();

    if let Some(current_index) = theme_ids.iter().position(|theme| theme == current_theme) {
        let current = theme_ids.remove(current_index);
        theme_ids.insert(0, current);
    }

    theme_ids
}

pub(super) fn filter_command_palette_item_indices_by_query(
    items: &[CommandPaletteItem],
    query: &str,
) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    let query_terms: Vec<String> = query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if query_terms.is_empty() {
        return (0..items.len()).collect();
    }

    let has_title_matches = items
        .iter()
        .any(|item| command_palette_text_matches_terms(&item.title, &query_terms));

    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let title_match = command_palette_text_matches_terms(&item.title, &query_terms);
            let matches = if has_title_matches {
                title_match
            } else {
                title_match || command_palette_text_matches_terms(&item.keywords, &query_terms)
            };
            matches.then_some(index)
        })
        .collect()
}

fn command_palette_text_matches_terms(text: &str, query_terms: &[String]) -> bool {
    let searchable = text.to_ascii_lowercase();
    let words: Vec<&str> = searchable
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();

    query_terms
        .iter()
        .all(|term| words.iter().any(|word| word.starts_with(term)))
}

pub(super) fn command_palette_viewport_height() -> f32 {
    COMMAND_PALETTE_MAX_ITEMS as f32 * COMMAND_PALETTE_ROW_HEIGHT
}

pub(super) fn command_palette_max_scroll_for_count(item_count: usize) -> f32 {
    (item_count as f32 * COMMAND_PALETTE_ROW_HEIGHT - command_palette_viewport_height()).max(0.0)
}

pub(super) fn command_palette_target_scroll_y(
    current_y: f32,
    selected_index: usize,
    item_count: usize,
) -> Option<f32> {
    if item_count == 0 {
        return None;
    }

    let viewport_height = command_palette_viewport_height();
    let max_scroll = command_palette_max_scroll_for_count(item_count);
    let row_top = selected_index as f32 * COMMAND_PALETTE_ROW_HEIGHT;
    let row_bottom = row_top + COMMAND_PALETTE_ROW_HEIGHT;

    let target = if row_top < current_y {
        row_top
    } else if row_bottom > current_y + viewport_height {
        row_bottom - viewport_height
    } else {
        current_y
    };

    Some(target.clamp(0.0, max_scroll))
}

pub(super) fn command_palette_next_scroll_y(
    current_y: f32,
    target_y: f32,
    max_scroll: f32,
    dt_seconds: f32,
) -> f32 {
    let target_y = target_y.clamp(0.0, max_scroll);
    let delta = target_y - current_y;
    if delta.abs() <= 0.5 {
        return target_y;
    }

    let dt = dt_seconds.clamp(1.0 / 240.0, 0.05);
    let smoothing = 1.0 - (-18.0 * dt).exp();
    let desired_step = delta * smoothing;
    let max_step = 1800.0 * dt;
    let step = desired_step.clamp(-max_step, max_step);
    let next_y = (current_y + step).clamp(0.0, max_scroll);

    if (target_y - next_y).abs() <= 0.5 {
        target_y
    } else {
        next_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_item(title: &str, keywords: &str, action: CommandAction) -> CommandPaletteItem {
        CommandPaletteItem::command_with_state(title, keywords, action, true, None)
    }

    #[test]
    fn query_re_prefers_title_matches_over_keywords() {
        let items = vec![
            command_item("Close Tab", "remove tab", CommandAction::CloseTab),
            command_item("Rename Tab", "title name", CommandAction::RenameTab),
            command_item(
                "Restart App",
                "relaunch reopen restart",
                CommandAction::RestartApp,
            ),
            command_item("Reset Zoom", "font default", CommandAction::ZoomReset),
            command_item(
                "Check for Updates",
                "release version updater",
                CommandAction::CheckForUpdates,
            ),
        ];

        let filtered_indices = filter_command_palette_item_indices_by_query(&items, "re");
        let actions: Vec<CommandAction> = filtered_indices
            .into_iter()
            .filter_map(|index| match items[index].kind {
                CommandPaletteItemKind::Command(action) => Some(action),
                _ => None,
            })
            .collect();

        assert_eq!(
            actions,
            vec![
                CommandAction::RenameTab,
                CommandAction::RestartApp,
                CommandAction::ZoomReset
            ]
        );
    }

    #[test]
    fn query_uses_keywords_when_no_titles_match() {
        let items = vec![
            command_item("Zoom In", "font increase", CommandAction::ZoomIn),
            command_item("Zoom Out", "font decrease", CommandAction::ZoomOut),
            command_item("Reset Zoom", "font default", CommandAction::ZoomReset),
        ];

        let filtered_indices = filter_command_palette_item_indices_by_query(&items, "font");
        let actions: Vec<CommandAction> = filtered_indices
            .into_iter()
            .filter_map(|index| match items[index].kind {
                CommandPaletteItemKind::Command(action) => Some(action),
                _ => None,
            })
            .collect();

        assert_eq!(
            actions,
            vec![
                CommandAction::ZoomIn,
                CommandAction::ZoomOut,
                CommandAction::ZoomReset
            ]
        );
    }

    #[test]
    fn query_splits_hyphenated_terms_on_non_alphanumeric_boundaries() {
        let items = vec![
            command_item("Tokyo Night", "theme", CommandAction::SwitchTheme),
            command_item("Tomorrow Night", "theme", CommandAction::SwitchTheme),
            command_item("Nord", "theme", CommandAction::SwitchTheme),
        ];

        let filtered_indices = filter_command_palette_item_indices_by_query(&items, "tokyo-night");
        let titles: Vec<&str> = filtered_indices
            .into_iter()
            .map(|index| items[index].title.as_str())
            .collect();

        assert_eq!(titles, vec!["Tokyo Night"]);
    }

    #[test]
    fn filtered_index_selection_clamps_after_query_change() {
        let mut state = CommandPaletteState::new(true);
        state.set_items(vec![
            command_item("New Tab", "tab", CommandAction::NewTab),
            command_item("Close Tab", "tab", CommandAction::CloseTab),
            command_item("Switch Theme", "theme", CommandAction::SwitchTheme),
        ]);
        assert!(state.set_selected_filtered_index(2));

        state.input_mut().set_text("close".to_string());
        state.refilter_current_query();
        assert_eq!(state.filtered_len(), 1);
        assert_eq!(state.selected_filtered_index(), Some(0));

        state.input_mut().set_text(String::new());
        state.refilter_current_query();
        assert_eq!(state.filtered_len(), 3);
        assert_eq!(state.selected_filtered_index(), Some(0));
    }

    #[test]
    fn move_selection_handles_empty_and_bounds_without_panics() {
        let mut state = CommandPaletteState::new(true);
        assert!(!state.move_selection_up());
        assert!(!state.move_selection_down());

        state.set_items(vec![
            command_item("New Tab", "tab", CommandAction::NewTab),
            command_item("Close Tab", "tab", CommandAction::CloseTab),
        ]);

        assert!(!state.move_selection_up());
        assert!(state.move_selection_down());
        assert!(!state.move_selection_down());
        assert!(state.move_selection_up());
    }

    #[test]
    fn target_scroll_y_only_moves_when_selection_leaves_viewport() {
        assert_eq!(command_palette_target_scroll_y(0.0, 2, 12), Some(0.0));
        assert_eq!(command_palette_target_scroll_y(0.0, 9, 12), Some(60.0));
        assert_eq!(command_palette_target_scroll_y(90.0, 0, 12), Some(0.0));
        assert_eq!(command_palette_target_scroll_y(0.0, 0, 0), None);
    }

    #[test]
    fn next_scroll_y_is_dt_based_and_respects_bounds() {
        let slow = command_palette_next_scroll_y(0.0, 120.0, 300.0, 1.0 / 240.0);
        let fast = command_palette_next_scroll_y(0.0, 120.0, 300.0, 0.05);
        assert!(fast > slow);
        assert!(fast <= 300.0);

        let snapped = command_palette_next_scroll_y(59.7, 60.0, 300.0, 1.0 / 60.0);
        assert_eq!(snapped, 60.0);

        let clamped = command_palette_next_scroll_y(280.0, 400.0, 300.0, 0.05);
        assert!(clamped <= 300.0);
    }

    #[test]
    fn ordered_theme_ids_pin_current_theme_first() {
        let ordered = ordered_theme_ids_for_palette(
            vec![
                "nord".to_string(),
                "termy".to_string(),
                "dracula".to_string(),
                "nord".to_string(),
            ],
            "termy",
        );

        assert_eq!(
            ordered,
            vec!["termy", "dracula", "nord", SHELL_DECIDE_THEME_ID]
        );

        let ordered_with_missing_current = ordered_theme_ids_for_palette(
            vec!["nord".to_string(), "dracula".to_string()],
            "tokyo-night",
        );

        assert_eq!(
            ordered_with_missing_current,
            vec!["tokyo-night", "dracula", "nord", SHELL_DECIDE_THEME_ID]
        );
    }

    #[test]
    fn close_resets_to_command_mode_and_clears_transient_state() {
        let mut state = CommandPaletteState::new(false);
        state.open(CommandPaletteMode::Themes);
        state.input_mut().set_text("theme".to_string());
        state.set_items(vec![CommandPaletteItem::command_with_state(
            "New Tab",
            "tab",
            CommandAction::NewTab,
            true,
            None,
        )]);
        state.set_selected_filtered_index(999);
        state.set_scroll_target_y(12.0);
        state.set_scroll_max_y_for_count(12);
        state.start_scroll_animation(Instant::now());

        state.close();

        assert!(!state.is_open());
        assert_eq!(state.mode(), CommandPaletteMode::Commands);
        assert!(state.input().text().is_empty());
        assert_eq!(state.filtered_len(), 0);
        assert!(state.scroll_target_y().is_none());
        assert_eq!(state.scroll_max_y(), 0.0);
        assert!(!state.is_scroll_animating());
    }
}
