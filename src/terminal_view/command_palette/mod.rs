use super::*;
use crate::theme_store;
use gpui::point;
use state::{
    CommandPaletteCommandIntent, CommandPaletteItem, CommandPaletteItemKind,
    command_palette_next_scroll_y, command_palette_target_scroll_y, ordered_theme_ids_for_palette,
};
use termy_command_core::{CommandAvailability, CommandCapabilities, CommandUnavailableReason};

mod render;
mod state;
mod state_layouts;
mod state_tmux;
pub(super) mod style;
mod tmux_sessions;

pub(super) use state::{CommandPaletteMode, CommandPaletteState, TaskIntent};
pub(super) use state_layouts::SavedLayoutIntent;
pub(super) use state_tmux::TmuxSessionIntent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteEscapeAction {
    ClosePalette,
    BackToCommands,
    BackToTmuxRenameSelect,
    BackToSavedLayoutRenameSelect,
    BackToTaskBrowse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteNavKey {
    Escape,
    Enter,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteNotifyTarget {
    Parent,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteNotifyEvent {
    OpenCloseTransition,
    InteractionOnly,
}

impl CommandPaletteNavKey {
    fn parse(key: &str) -> Option<Self> {
        match key {
            "escape" => Some(Self::Escape),
            "enter" => Some(Self::Enter),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            _ => None,
        }
    }
}

impl TerminalView {
    fn command_palette_notify_target_for_event(
        event: CommandPaletteNotifyEvent,
    ) -> CommandPaletteNotifyTarget {
        match event {
            CommandPaletteNotifyEvent::OpenCloseTransition => CommandPaletteNotifyTarget::Parent,
            CommandPaletteNotifyEvent::InteractionOnly => CommandPaletteNotifyTarget::Overlay,
        }
    }

    fn notify_for_command_palette_event(
        &mut self,
        event: CommandPaletteNotifyEvent,
        cx: &mut Context<Self>,
    ) {
        match Self::command_palette_notify_target_for_event(event) {
            CommandPaletteNotifyTarget::Parent => {
                cx.notify();
                if event == CommandPaletteNotifyEvent::OpenCloseTransition {
                    self.notify_overlay(cx);
                }
            }
            CommandPaletteNotifyTarget::Overlay => self.notify_overlay(cx),
        }
    }

    pub(super) fn is_command_palette_open(&self) -> bool {
        self.command_palette.is_open()
    }

    pub(super) fn set_command_palette_show_keybinds(&mut self, show_keybinds: bool) {
        self.command_palette.set_show_keybinds(show_keybinds);
        self.command_palette.clear_shortcut_cache();
    }

    pub(super) fn command_palette_input(&self) -> &InlineInputState {
        self.command_palette.input()
    }

    pub(super) fn command_palette_input_mut(&mut self) -> &mut InlineInputState {
        self.command_palette.input_mut()
    }

    fn command_palette_shortcut(
        &mut self,
        action: CommandAction,
        window: &Window,
    ) -> Option<String> {
        if !self.command_palette.show_keybinds() {
            return None;
        }

        if let Some(cached) = self.command_palette.cached_shortcut(action) {
            return cached;
        }

        let shortcut = action.keybinding_label(window, &self.focus_handle);
        self.command_palette
            .cache_shortcut(action, shortcut.clone());
        shortcut
    }

    fn command_palette_action_availability_for_state(
        action: CommandAction,
        install_cli_available: bool,
        tmux_enabled: bool,
    ) -> CommandAvailability {
        action.availability(CommandCapabilities {
            tmux_runtime_active: tmux_enabled,
            install_cli_available,
        })
    }

    fn command_palette_status_hint_for_unavailable_reason(
        reason: CommandUnavailableReason,
    ) -> &'static str {
        match reason {
            CommandUnavailableReason::RequiresTmuxRuntime => "tmux required",
            CommandUnavailableReason::InstallCliAlreadyInstalled => "Installed",
        }
    }

    fn command_palette_command_item_for_state(
        action: CommandAction,
        title: &str,
        keywords: &str,
        install_cli_available: bool,
        tmux_enabled: bool,
    ) -> CommandPaletteItem {
        let availability = Self::command_palette_action_availability_for_state(
            action,
            install_cli_available,
            tmux_enabled,
        );
        let status_hint = availability
            .reason
            .map(Self::command_palette_status_hint_for_unavailable_reason);

        CommandPaletteItem::command_with_state(
            title,
            keywords,
            action,
            availability.enabled,
            status_hint,
        )
    }

    fn command_palette_core_command_items_for_state(
        install_cli_available: bool,
        tmux_enabled: bool,
    ) -> Vec<CommandPaletteItem> {
        CommandAction::palette_entries()
            .into_iter()
            .map(|entry| {
                Self::command_palette_command_item_for_state(
                    entry.action,
                    entry.title,
                    entry.keywords,
                    install_cli_available,
                    tmux_enabled,
                )
            })
            .collect()
    }

    fn command_palette_command_items_for_state(
        &self,
        install_cli_available: bool,
        tmux_enabled: bool,
    ) -> Vec<CommandPaletteItem> {
        let mut items =
            Self::command_palette_core_command_items_for_state(install_cli_available, tmux_enabled);

        if let Ok(plugin_entries) = crate::plugins::command_palette_entries() {
            items.extend(plugin_entries.into_iter().map(|entry| {
                CommandPaletteItem::plugin_command(
                    entry.title,
                    entry.keywords,
                    entry.plugin_id,
                    entry.command_id,
                    entry.enabled,
                    (!entry.enabled).then_some("plugin not running"),
                )
            }));
        }

        items
    }

    fn command_palette_items_for_mode(&self, mode: CommandPaletteMode) -> Vec<CommandPaletteItem> {
        match mode {
            CommandPaletteMode::Commands => self.command_palette_command_items_for_state(
                self.install_cli_available(),
                self.runtime_uses_tmux(),
            ),
            CommandPaletteMode::Themes => self.command_palette_theme_items(),
            CommandPaletteMode::TmuxSessions => self.command_palette.tmux_session_items_for_query(
                self.command_palette.input().text(),
                self.tmux_active_session_name_for_session_palette()
                    .as_deref(),
            ),
            CommandPaletteMode::Layouts => {
                let mut items = self
                    .command_palette
                    .saved_layout_items_for_query(self.command_palette.input().text());
                self.insert_saved_layout_tasks_item(&mut items);
                items
            }
            CommandPaletteMode::Tasks => self.command_palette_task_items(),
        }
    }

    fn saved_layout_tasks_item_for_state(
        saved_layout_intent: SavedLayoutIntent,
        query_text: &str,
        current_named_layout: Option<&str>,
        layout_has_tasks: bool,
    ) -> Option<CommandPaletteItem> {
        if saved_layout_intent != SavedLayoutIntent::Browse
            || !query_text.trim().is_empty()
            || !layout_has_tasks
        {
            return None;
        }

        let layout_name = current_named_layout?;
        Some(CommandPaletteItem {
            title: format!("Run Tasks for \"{}\"", layout_name),
            keywords: format!("saved layout tasks run {}", layout_name.replace('-', " ")),
            enabled: true,
            status_hint: None,
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::SavedLayoutOpenTasksMode {
                layout_name: layout_name.to_string(),
            },
        })
    }

    fn insert_saved_layout_tasks_item(&self, items: &mut Vec<CommandPaletteItem>) {
        let current_named_layout = self.current_named_layout.as_deref();
        let layout_has_tasks =
            current_named_layout.is_some_and(|layout_name| self.layout_has_tasks(layout_name));
        if let Some(item) = Self::saved_layout_tasks_item_for_state(
            self.command_palette.saved_layout_intent(),
            self.command_palette.input().text(),
            current_named_layout,
            layout_has_tasks,
        ) {
            items.insert(1.min(items.len()), item);
        }
    }

    fn palette_task_name_is_valid(task_name: &str) -> bool {
        !task_name.trim().contains('.')
    }

    fn layout_has_tasks(&self, layout_name: &str) -> bool {
        self.tasks.iter().any(|task| {
            task.layout
                .as_deref()
                .is_some_and(|task_layout| task_layout.eq_ignore_ascii_case(layout_name))
        })
    }

    fn active_current_command(&self) -> Option<&str> {
        self.tabs
            .get(self.active_tab)
            .and_then(|tab| tab.current_command.as_deref())
            .map(str::trim)
            .filter(|command| !command.is_empty())
    }

    fn suggested_task_name_for_command(command: &str) -> String {
        let first_token = command.split_whitespace().next().unwrap_or("task");
        let base = std::path::Path::new(first_token)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(first_token)
            .trim_start_matches('-')
            .trim_end_matches(".exe");

        let mut normalized = String::with_capacity(base.len());
        let mut last_was_sep = false;
        for ch in base.chars() {
            if ch.is_ascii_alphanumeric() {
                normalized.push(ch.to_ascii_lowercase());
                last_was_sep = false;
            } else if !last_was_sep {
                normalized.push('_');
                last_was_sep = true;
            }
        }
        let normalized = normalized.trim_matches('_');
        if normalized.is_empty() {
            "task".to_string()
        } else {
            normalized.to_string()
        }
    }

    fn command_palette_task_items(&self) -> Vec<CommandPaletteItem> {
        let query = self.command_palette.input().text().trim();
        let current_layout = self.current_named_layout.as_deref();

        match self.command_palette.task_intent() {
            TaskIntent::Browse => {
                let mut items = self
                    .tasks
                    .iter()
                    .filter(|task| match (task.layout.as_deref(), current_layout) {
                        (None, _) => true,
                        (Some(task_layout), Some(current_layout)) => {
                            task_layout.eq_ignore_ascii_case(current_layout)
                        }
                        (Some(_), None) => false,
                    })
                    .map(|task| {
                        CommandPaletteItem::task(
                            task.name.as_str(),
                            task.command.as_str(),
                            task.working_dir.as_deref(),
                            task.layout.as_deref(),
                        )
                    })
                    .collect::<Vec<_>>();

                if query.is_empty() {
                    items.insert(
                        0,
                        CommandPaletteItem {
                            title: "New Task…".to_string(),
                            keywords: "task new create add".to_string(),
                            enabled: true,
                            status_hint: None,
                            tmux_status_hint: None,
                            kind: CommandPaletteItemKind::TaskOpenCreateGlobalMode,
                        },
                    );

                    let active_command = self.active_current_command();
                    items.insert(
                        1.min(items.len()),
                        CommandPaletteItem {
                            title: "Save Current Command as Task…".to_string(),
                            keywords: "task save current command active".to_string(),
                            enabled: active_command.is_some(),
                            status_hint: active_command.is_none().then_some("no active command"),
                            tmux_status_hint: None,
                            kind: CommandPaletteItemKind::TaskOpenSaveCurrentCommandGlobalMode,
                        },
                    );

                    if let Some(layout_name) = current_layout {
                        items.insert(
                            2.min(items.len()),
                            CommandPaletteItem {
                                title: format!("New Task for \"{}\"…", layout_name),
                                keywords: format!(
                                    "task new create add layout {}",
                                    layout_name.replace('-', " ")
                                ),
                                enabled: true,
                                status_hint: None,
                                tmux_status_hint: None,
                                kind: CommandPaletteItemKind::TaskOpenCreateLayoutMode {
                                    layout_name: layout_name.to_string(),
                                },
                            },
                        );
                        items.insert(
                            3.min(items.len()),
                            CommandPaletteItem {
                                title: format!("Save Current Command for \"{}\"…", layout_name),
                                keywords: format!(
                                    "task save current command active layout {}",
                                    layout_name.replace('-', " ")
                                ),
                                enabled: active_command.is_some(),
                                status_hint: active_command
                                    .is_none()
                                    .then_some("no active command"),
                                tmux_status_hint: None,
                                kind:
                                    CommandPaletteItemKind::TaskOpenSaveCurrentCommandLayoutMode {
                                        layout_name: layout_name.to_string(),
                                    },
                            },
                        );
                    }
                }

                items
            }
            TaskIntent::CreateGlobalInput | TaskIntent::CreateLayoutInput => {
                let layout_name = match self.command_palette.task_intent() {
                    TaskIntent::CreateLayoutInput => current_layout.map(ToOwned::to_owned),
                    _ => None,
                };

                vec![self.command_palette_task_create_item(query, layout_name)]
            }
        }
    }

    fn command_palette_task_create_item(
        &self,
        query: &str,
        layout_name: Option<String>,
    ) -> CommandPaletteItem {
        let Some((task_name, command)) = Self::parse_task_definition_input(query) else {
            return CommandPaletteItem {
                title: "Create Task".to_string(),
                keywords: "task new create add".to_string(),
                enabled: false,
                status_hint: Some("use name: command"),
                tmux_status_hint: None,
                kind: CommandPaletteItemKind::TaskCreate {
                    task_name: String::new(),
                    command: String::new(),
                    layout_name,
                },
            };
        };

        let already_exists = self
            .tasks
            .iter()
            .any(|task| task.name.eq_ignore_ascii_case(task_name));

        CommandPaletteItem {
            title: match layout_name.as_deref() {
                Some(layout_name) => format!("Save Task \"{}\" for \"{}\"", task_name, layout_name),
                None => format!("Save Task \"{}\"", task_name),
            },
            keywords: format!(
                "task save create {} {}",
                task_name.replace('-', " "),
                command
            ),
            enabled: !already_exists,
            status_hint: already_exists.then_some("task exists"),
            tmux_status_hint: None,
            kind: CommandPaletteItemKind::TaskCreate {
                task_name: task_name.to_string(),
                command: command.to_string(),
                layout_name,
            },
        }
    }

    fn parse_task_definition_input(query: &str) -> Option<(&str, &str)> {
        let (task_name, command) = query.split_once(':')?;
        let task_name = task_name.trim();
        let command = command.trim();
        if task_name.is_empty() || command.is_empty() {
            return None;
        }
        Some((task_name, command))
    }

    fn command_palette_theme_items(&self) -> Vec<CommandPaletteItem> {
        let theme_ids = theme_store::load_installed_theme_ids();

        ordered_theme_ids_for_palette(theme_ids, &self.theme_id)
            .into_iter()
            .map(|theme| {
                let is_active = theme == self.theme_id;
                CommandPaletteItem::theme(theme, is_active)
            })
            .collect()
    }

    fn apply_command_palette_mode_setup(
        &mut self,
        mode: CommandPaletteMode,
        animate_selection: bool,
        notify_event: CommandPaletteNotifyEvent,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.clear_shortcut_cache();
        if mode == CommandPaletteMode::TmuxSessions
            && let Err(error) = self.reload_tmux_session_palette_items()
        {
            // Keep the tmux session palette usable when list-sessions fails by
            // preserving the selected socket target and rendering intent-specific rows.
            self.command_palette.set_tmux_session_rows(
                Vec::new(),
                self.tmux_primary_socket_target_for_session_palette(),
            );
            termy_toast::error(format!("Failed to list tmux sessions: {error}"));
        }
        if mode == CommandPaletteMode::Layouts
            && let Err(error) = self.reload_saved_layout_palette_items()
        {
            termy_toast::error(format!("Failed to load saved layouts: {error}"));
        }
        let items = self.command_palette_items_for_mode(mode);
        self.command_palette.set_items(items);
        self.inline_input_selecting = false;

        let item_count = self.command_palette.filtered_len();
        if item_count == 0 {
            self.command_palette.reset_scroll_animation_state();
        } else if animate_selection {
            self.animate_command_palette_to_selected(item_count, cx);
        }

        self.reset_cursor_blink_phase();
        self.notify_for_command_palette_event(notify_event, cx);
    }

    pub(super) fn set_command_palette_mode(
        &mut self,
        mode: CommandPaletteMode,
        animate_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.set_mode(mode);
        self.apply_command_palette_mode_setup(
            mode,
            animate_selection,
            CommandPaletteNotifyEvent::InteractionOnly,
            cx,
        );
    }

    pub(super) fn open_command_palette_in_mode(
        &mut self,
        mode: CommandPaletteMode,
        cx: &mut Context<Self>,
    ) {
        let _ = self.close_terminal_context_menu(cx);
        let was_open = self.command_palette.is_open();
        self.command_palette.open(mode);
        let notify_event = if was_open {
            CommandPaletteNotifyEvent::InteractionOnly
        } else {
            CommandPaletteNotifyEvent::OpenCloseTransition
        };
        self.apply_command_palette_mode_setup(mode, false, notify_event, cx);
    }

    pub(super) fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.open_command_palette_in_mode(CommandPaletteMode::Commands, cx);
    }

    pub(super) fn open_saved_layouts_palette(&mut self, cx: &mut Context<Self>) {
        if self.runtime_kind() != RuntimeKind::Native {
            termy_toast::info("Switch to the native runtime to use saved layouts");
            self.notify_overlay(cx);
            return;
        }
        self.open_command_palette_in_mode(CommandPaletteMode::Layouts, cx);
    }

    pub(super) fn open_tasks_palette(&mut self, cx: &mut Context<Self>) {
        self.open_command_palette_in_mode(CommandPaletteMode::Tasks, cx);
    }

    pub(super) fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        if !self.command_palette.is_open() {
            return;
        }

        self.command_palette.close();
        self.inline_input_selecting = false;
        self.notify_for_command_palette_event(CommandPaletteNotifyEvent::OpenCloseTransition, cx);
    }

    pub(super) fn refresh_command_palette_matches(
        &mut self,
        animate_selection: bool,
        cx: &mut Context<Self>,
    ) {
        if self.command_palette.mode() == CommandPaletteMode::TmuxSessions {
            let items = self.command_palette.tmux_session_items_for_query(
                self.command_palette.input().text(),
                self.tmux_active_session_name_for_session_palette()
                    .as_deref(),
            );
            self.command_palette.set_items(items);
        } else if self.command_palette.mode() == CommandPaletteMode::Layouts {
            let mut items = self
                .command_palette
                .saved_layout_items_for_query(self.command_palette.input().text());
            self.insert_saved_layout_tasks_item(&mut items);
            self.command_palette.set_items(items);
        } else if self.command_palette.mode() == CommandPaletteMode::Tasks {
            let items = self.command_palette_task_items();
            self.command_palette.set_items(items);
        } else {
            self.command_palette.refilter_current_query();
        }
        let len = self.command_palette.filtered_len();

        if len == 0 {
            self.command_palette.reset_scroll_animation_state();
            return;
        }

        if animate_selection {
            self.animate_command_palette_to_selected(len, cx);
        }
    }

    pub(super) fn refresh_command_palette_items_for_current_mode(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.is_command_palette_open() {
            return;
        }

        let mode = self.command_palette.mode();
        self.apply_command_palette_mode_setup(
            mode,
            false,
            CommandPaletteNotifyEvent::InteractionOnly,
            cx,
        );
    }

    pub(super) fn animate_command_palette_to_selected(
        &mut self,
        item_count: usize,
        cx: &mut Context<Self>,
    ) {
        if item_count == 0 {
            self.command_palette.reset_scroll_animation_state();
            return;
        }

        self.command_palette.set_scroll_max_y_for_count(item_count);

        let scroll_handle = self.command_palette.base_scroll_handle();
        let offset = scroll_handle.offset();
        let current_y = -Into::<f32>::into(offset.y);
        let selected_index = self.command_palette.selected_filtered_index().unwrap_or(0);
        let Some(target_y) = command_palette_target_scroll_y(current_y, selected_index, item_count)
        else {
            self.command_palette.reset_scroll_animation_state();
            return;
        };

        if (target_y - current_y).abs() <= f32::EPSILON {
            self.command_palette.clear_scroll_target_y();
            self.command_palette.stop_scroll_animation();
            return;
        }

        self.command_palette.set_scroll_target_y(target_y);
        self.start_command_palette_scroll_animation(cx);
    }

    fn start_command_palette_scroll_animation(&mut self, cx: &mut Context<Self>) {
        if self.command_palette.is_scroll_animating() {
            return;
        }
        self.command_palette.start_scroll_animation(Instant::now());

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(16)).await;
                let keep_animating = match cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        let changed = view.tick_command_palette_scroll_animation();
                        if changed {
                            view.notify_for_command_palette_event(
                                CommandPaletteNotifyEvent::InteractionOnly,
                                cx,
                            );
                        }
                        view.command_palette.is_scroll_animating()
                    })
                }) {
                    Ok(keep_animating) => keep_animating,
                    _ => break,
                };

                if !keep_animating {
                    break;
                }
            }
        })
        .detach();
    }

    fn tick_command_palette_scroll_animation(&mut self) -> bool {
        if !self.command_palette.is_open() {
            self.command_palette.reset_scroll_animation_state();
            return false;
        }

        let Some(target_y) = self.command_palette.scroll_target_y() else {
            self.command_palette.stop_scroll_animation();
            return false;
        };

        let scroll_handle = self.command_palette.base_scroll_handle();
        let offset = scroll_handle.offset();
        let current_y = -Into::<f32>::into(offset.y);
        let max_offset_from_handle: f32 = scroll_handle.max_offset().height.into();
        let max_scroll = max_offset_from_handle
            .max(self.command_palette.scroll_max_y())
            .max(0.0);
        let now = Instant::now();
        let dt = self.command_palette.scroll_dt_seconds(now);

        let next_y = command_palette_next_scroll_y(current_y, target_y, max_scroll, dt);
        scroll_handle.set_offset(point(offset.x, px(-next_y)));

        if (target_y - next_y).abs() <= 0.5 {
            self.command_palette.clear_scroll_target_y();
            self.command_palette.stop_scroll_animation();
            return true;
        }

        true
    }

    pub(super) fn handle_command_palette_key_down(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(nav_key) = CommandPaletteNavKey::parse(key) else {
            return;
        };

        match nav_key {
            CommandPaletteNavKey::Escape => {
                match Self::command_palette_escape_action(
                    self.command_palette.mode(),
                    self.command_palette.tmux_session_intent(),
                    self.command_palette.command_intent(),
                    self.command_palette.saved_layout_intent(),
                    self.command_palette.task_intent(),
                ) {
                    CommandPaletteEscapeAction::ClosePalette => self.close_command_palette(cx),
                    CommandPaletteEscapeAction::BackToCommands => {
                        self.set_command_palette_mode(CommandPaletteMode::Commands, false, cx);
                    }
                    CommandPaletteEscapeAction::BackToTmuxRenameSelect => {
                        if self.command_palette.back_from_tmux_rename_input() {
                            self.apply_command_palette_mode_setup(
                                CommandPaletteMode::TmuxSessions,
                                false,
                                CommandPaletteNotifyEvent::InteractionOnly,
                                cx,
                            );
                        }
                    }
                    CommandPaletteEscapeAction::BackToSavedLayoutRenameSelect => {
                        if self.command_palette.back_from_saved_layout_rename_input() {
                            self.apply_command_palette_mode_setup(
                                CommandPaletteMode::Layouts,
                                false,
                                CommandPaletteNotifyEvent::InteractionOnly,
                                cx,
                            );
                        }
                    }
                    CommandPaletteEscapeAction::BackToTaskBrowse => {
                        self.command_palette.set_task_intent(TaskIntent::Browse);
                        self.apply_command_palette_mode_setup(
                            CommandPaletteMode::Tasks,
                            false,
                            CommandPaletteNotifyEvent::InteractionOnly,
                            cx,
                        );
                    }
                }
            }
            CommandPaletteNavKey::Enter => {
                self.execute_command_palette_selection(window, cx);
            }
            CommandPaletteNavKey::Up => {
                let len = self.command_palette.filtered_len();
                if self.command_palette.move_selection_up() {
                    self.animate_command_palette_to_selected(len, cx);
                    self.notify_for_command_palette_event(
                        CommandPaletteNotifyEvent::InteractionOnly,
                        cx,
                    );
                }
            }
            CommandPaletteNavKey::Down => {
                let len = self.command_palette.filtered_len();
                if self.command_palette.move_selection_down() {
                    self.animate_command_palette_to_selected(len, cx);
                    self.notify_for_command_palette_event(
                        CommandPaletteNotifyEvent::InteractionOnly,
                        cx,
                    );
                }
            }
        }
    }

    fn command_palette_escape_action(
        mode: CommandPaletteMode,
        tmux_session_intent: TmuxSessionIntent,
        _command_intent: CommandPaletteCommandIntent,
        saved_layout_intent: SavedLayoutIntent,
        task_intent: TaskIntent,
    ) -> CommandPaletteEscapeAction {
        match mode {
            CommandPaletteMode::Commands => CommandPaletteEscapeAction::ClosePalette,
            CommandPaletteMode::Themes => CommandPaletteEscapeAction::BackToCommands,
            CommandPaletteMode::TmuxSessions
                if tmux_session_intent == TmuxSessionIntent::RenameInput =>
            {
                CommandPaletteEscapeAction::BackToTmuxRenameSelect
            }
            CommandPaletteMode::TmuxSessions => CommandPaletteEscapeAction::BackToCommands,
            CommandPaletteMode::Layouts
                if saved_layout_intent == SavedLayoutIntent::RenameInput =>
            {
                CommandPaletteEscapeAction::BackToSavedLayoutRenameSelect
            }
            CommandPaletteMode::Layouts => CommandPaletteEscapeAction::BackToCommands,
            CommandPaletteMode::Tasks
                if matches!(
                    task_intent,
                    TaskIntent::CreateGlobalInput | TaskIntent::CreateLayoutInput
                ) =>
            {
                CommandPaletteEscapeAction::BackToTaskBrowse
            }
            CommandPaletteMode::Tasks => CommandPaletteEscapeAction::BackToCommands,
        }
    }

    fn execute_command_palette_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(filtered_index) = self.command_palette.selected_filtered_index() else {
            return;
        };

        self.execute_command_palette_filtered_index(filtered_index, window, cx);
    }

    fn execute_command_palette_filtered_index(
        &mut self,
        filtered_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.command_palette.filtered_item(filtered_index).cloned() else {
            return;
        };

        self.command_palette
            .set_selected_filtered_index(filtered_index);
        self.execute_command_palette_item(item, window, cx);
    }

    fn execute_command_palette_item(
        &mut self,
        item: CommandPaletteItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item.kind {
            CommandPaletteItemKind::Command(action) => {
                if !item.enabled {
                    termy_toast::info(Self::command_palette_disabled_action_message_for_state(
                        action,
                        self.install_cli_available(),
                        self.runtime_uses_tmux(),
                    ));
                    self.notify_overlay(cx);
                    return;
                }
                self.execute_command_palette_action(action, window, cx)
            }
            CommandPaletteItemKind::PluginCommand {
                plugin_id,
                command_id,
            } => {
                if !item.enabled {
                    termy_toast::info("Start the plugin to use this command");
                    self.notify_overlay(cx);
                    return;
                }
                self.execute_plugin_command_palette_action(
                    plugin_id.as_str(),
                    command_id.as_str(),
                    cx,
                );
            }
            CommandPaletteItemKind::Theme(theme_id) => {
                self.select_theme_from_palette(theme_id.as_str(), cx)
            }
            CommandPaletteItemKind::TmuxSessionAttachOrSwitch {
                session_name,
                socket_target,
            }
            | CommandPaletteItemKind::TmuxSessionCreateAndAttach {
                session_name,
                socket_target,
            } => self.activate_tmux_session_from_palette(
                session_name.as_str(),
                socket_target,
                item.enabled,
                item.tmux_status_hint,
                cx,
            ),
            CommandPaletteItemKind::TmuxSessionDetachCurrent => {
                self.detach_current_tmux_session_from_palette(cx)
            }
            CommandPaletteItemKind::TmuxSessionOpenRenameMode => {
                self.open_tmux_session_rename_mode_from_palette(cx)
            }
            CommandPaletteItemKind::TmuxSessionOpenKillMode => {
                self.open_tmux_session_kill_mode_from_palette(cx)
            }
            CommandPaletteItemKind::TmuxSessionRenameSelect {
                session_name,
                socket_target,
            } => self.select_tmux_session_for_rename_from_palette(
                session_name.as_str(),
                socket_target,
                item.enabled,
                item.tmux_status_hint,
                cx,
            ),
            CommandPaletteItemKind::TmuxSessionRenameApply {
                current_session_name,
                next_session_name,
                socket_target,
            } => self.apply_tmux_session_rename_from_palette(
                current_session_name.as_str(),
                next_session_name.as_str(),
                socket_target,
                item.enabled,
                item.tmux_status_hint,
                cx,
            ),
            CommandPaletteItemKind::TmuxSessionKill {
                session_name,
                socket_target,
            } => self.confirm_kill_tmux_session_from_palette(
                session_name.as_str(),
                socket_target,
                item.enabled,
                item.tmux_status_hint,
                cx,
            ),
            CommandPaletteItemKind::SavedLayoutOpen { layout_name } => {
                self.load_saved_layout_from_palette(layout_name.as_str(), cx)
            }
            CommandPaletteItemKind::SavedLayoutOpenTasksMode { layout_name } => {
                self.open_tasks_palette_from_saved_layout(layout_name.as_str(), cx)
            }
            CommandPaletteItemKind::SavedLayoutOpenSaveMode => {
                self.open_save_layout_input_from_palette(cx)
            }
            CommandPaletteItemKind::SavedLayoutSaveAs { layout_name } => {
                self.save_current_layout_from_palette(layout_name.as_str(), item.enabled, cx)
            }
            CommandPaletteItemKind::SavedLayoutOpenRenameMode => {
                self.open_saved_layout_rename_mode_from_palette(cx)
            }
            CommandPaletteItemKind::SavedLayoutRenameSelect { layout_name } => {
                self.select_saved_layout_for_rename_from_palette(layout_name.as_str(), cx)
            }
            CommandPaletteItemKind::SavedLayoutRenameApply {
                current_layout_name,
                next_layout_name,
            } => self.apply_saved_layout_rename_from_palette(
                current_layout_name.as_str(),
                next_layout_name.as_str(),
                item.enabled,
                cx,
            ),
            CommandPaletteItemKind::SavedLayoutOpenDeleteMode => {
                self.open_saved_layout_delete_mode_from_palette(cx)
            }
            CommandPaletteItemKind::SavedLayoutDelete { layout_name } => {
                self.delete_saved_layout_from_palette(layout_name.as_str(), cx)
            }
            CommandPaletteItemKind::TaskOpenCreateGlobalMode => {
                self.open_task_create_input_from_palette(None, cx)
            }
            CommandPaletteItemKind::TaskOpenCreateLayoutMode { layout_name } => {
                self.open_task_create_input_from_palette(Some(layout_name.as_str()), cx)
            }
            CommandPaletteItemKind::TaskOpenSaveCurrentCommandGlobalMode => {
                self.open_save_current_command_task_input_from_palette(None, cx)
            }
            CommandPaletteItemKind::TaskOpenSaveCurrentCommandLayoutMode { layout_name } => self
                .open_save_current_command_task_input_from_palette(Some(layout_name.as_str()), cx),
            CommandPaletteItemKind::TaskCreate {
                task_name,
                command,
                layout_name,
            } => self.save_task_from_palette(
                task_name.as_str(),
                command.as_str(),
                layout_name.as_deref(),
                item.enabled,
                cx,
            ),
            CommandPaletteItemKind::Task {
                task_name,
                command,
                working_dir,
                layout_name,
            } => self.run_task_from_palette(
                task_name.as_str(),
                command.as_str(),
                working_dir.as_deref(),
                layout_name.as_deref(),
                cx,
            ),
        }
    }

    fn run_task_from_palette(
        &mut self,
        task_name: &str,
        command: &str,
        working_dir: Option<&str>,
        layout_name: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let command = command.trim();
        if command.is_empty() {
            termy_toast::error(format!("Task \"{task_name}\" has no command"));
            self.notify_overlay(cx);
            return;
        }

        self.close_command_palette(cx);

        let mut command_input = command.to_string();
        if !command_input.ends_with('\n') {
            command_input.push('\n');
        }

        self.add_tab_with_working_dir(working_dir, cx);
        if let Some(tab) = self.tabs.get(self.active_tab)
            && let Some(terminal) = tab.active_terminal()
        {
            terminal.write_input(command_input.as_bytes());
            cx.notify();
        }
        match layout_name {
            Some(layout_name) => termy_toast::success(format!(
                "Started task \"{task_name}\" for layout \"{layout_name}\""
            )),
            None => termy_toast::success(format!("Started task \"{task_name}\"")),
        }
        self.notify_overlay(cx);
    }

    fn open_task_create_input_from_palette(
        &mut self,
        layout_name: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let intent = if layout_name.is_some() {
            TaskIntent::CreateLayoutInput
        } else {
            TaskIntent::CreateGlobalInput
        };
        self.command_palette.set_task_intent(intent);
        self.command_palette.input_mut().clear();
        self.apply_command_palette_mode_setup(
            CommandPaletteMode::Tasks,
            false,
            CommandPaletteNotifyEvent::InteractionOnly,
            cx,
        );
    }

    fn open_save_current_command_task_input_from_palette(
        &mut self,
        layout_name: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = self.active_current_command().map(ToOwned::to_owned) else {
            termy_toast::info("No active command to save");
            self.notify_overlay(cx);
            return;
        };

        let suggested_name = Self::suggested_task_name_for_command(command.as_str());
        let prefill = format!("{suggested_name}: {command}");
        self.open_task_create_input_from_palette(layout_name, cx);
        self.command_palette.input_mut().set_text(prefill);
        self.refresh_command_palette_matches(false, cx);
        self.notify_overlay(cx);
    }

    fn save_task_from_palette(
        &mut self,
        task_name: &str,
        command: &str,
        layout_name: Option<&str>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if !enabled {
            termy_toast::info("Use format name: command and pick a unique task name");
            self.notify_overlay(cx);
            return;
        }

        let task_name = task_name.trim();
        if !Self::palette_task_name_is_valid(task_name) {
            termy_toast::error("Task names cannot contain '.'");
            self.notify_overlay(cx);
            return;
        }

        let command = command.trim();

        let task = config::TaskConfig {
            name: task_name.to_string(),
            command: command.to_string(),
            layout: layout_name.map(|value| value.trim().to_string()),
            working_dir: None,
        };

        match config::upsert_task(task.clone()) {
            Ok(()) => {
                self.command_palette.set_task_intent(TaskIntent::Browse);
                self.close_command_palette(cx);
                self.reload_config(cx);
                match task.layout.as_deref() {
                    Some(layout_name) => termy_toast::success(format!(
                        "Saved task \"{}\" for layout \"{}\"",
                        task.name, layout_name
                    )),
                    None => termy_toast::success(format!("Saved task \"{}\"", task.name)),
                }
                self.notify_overlay(cx);
            }
            Err(error) => {
                termy_toast::error(error);
                self.notify_overlay(cx);
            }
        }
    }

    fn reload_saved_layout_palette_items(&mut self) -> Result<(), String> {
        let names = self.saved_layout_names()?;
        self.command_palette.set_saved_layout_names(
            names,
            self.current_named_layout.clone(),
            self.native_layout_autosave,
        );
        Ok(())
    }

    fn open_save_layout_input_from_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette
            .set_saved_layout_intent(SavedLayoutIntent::SaveInput);
        self.apply_command_palette_mode_setup(
            CommandPaletteMode::Layouts,
            false,
            CommandPaletteNotifyEvent::InteractionOnly,
            cx,
        );
    }

    fn open_tasks_palette_from_saved_layout(&mut self, layout_name: &str, cx: &mut Context<Self>) {
        let Some(current_layout) = self.current_named_layout.as_deref() else {
            termy_toast::info("Load a saved layout before running layout tasks");
            self.notify_overlay(cx);
            return;
        };
        if !current_layout.eq_ignore_ascii_case(layout_name) {
            termy_toast::info("Load that saved layout first to run its tasks");
            self.notify_overlay(cx);
            return;
        }
        self.open_tasks_palette(cx);
    }

    fn save_current_layout_from_palette(
        &mut self,
        layout_name: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if !enabled {
            termy_toast::info("Enter a layout name first");
            self.notify_overlay(cx);
            return;
        }
        match self.save_current_workspace_as_named_layout(layout_name) {
            Ok(()) => {
                self.close_command_palette(cx);
                termy_toast::success(format!("Saved layout \"{}\"", layout_name.trim()));
                self.notify_overlay(cx);
            }
            Err(error) => {
                termy_toast::error(error);
                self.notify_overlay(cx);
            }
        }
    }

    fn load_saved_layout_from_palette(&mut self, layout_name: &str, cx: &mut Context<Self>) {
        match self.load_named_layout(layout_name, cx) {
            Ok(()) => {
                self.close_command_palette(cx);
                termy_toast::success(format!("Loaded layout \"{}\"", layout_name));
                self.notify_overlay(cx);
            }
            Err(error) => {
                termy_toast::error(error);
                self.notify_overlay(cx);
            }
        }
    }

    fn open_saved_layout_rename_mode_from_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette
            .set_saved_layout_intent(SavedLayoutIntent::RenameSelect);
        self.apply_command_palette_mode_setup(
            CommandPaletteMode::Layouts,
            false,
            CommandPaletteNotifyEvent::InteractionOnly,
            cx,
        );
    }

    fn select_saved_layout_for_rename_from_palette(
        &mut self,
        layout_name: &str,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.begin_saved_layout_rename(layout_name);
        self.apply_command_palette_mode_setup(
            CommandPaletteMode::Layouts,
            false,
            CommandPaletteNotifyEvent::InteractionOnly,
            cx,
        );
    }

    fn apply_saved_layout_rename_from_palette(
        &mut self,
        current_layout_name: &str,
        next_layout_name: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if !enabled {
            termy_toast::info("Enter a different layout name");
            self.notify_overlay(cx);
            return;
        }
        match self.rename_named_layout(current_layout_name, next_layout_name) {
            Ok(()) => {
                self.close_command_palette(cx);
                termy_toast::success(format!(
                    "Renamed layout \"{}\" to \"{}\"",
                    current_layout_name,
                    next_layout_name.trim()
                ));
                self.notify_overlay(cx);
            }
            Err(error) => {
                termy_toast::error(error);
                self.notify_overlay(cx);
            }
        }
    }

    fn open_saved_layout_delete_mode_from_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette
            .set_saved_layout_intent(SavedLayoutIntent::Delete);
        self.apply_command_palette_mode_setup(
            CommandPaletteMode::Layouts,
            false,
            CommandPaletteNotifyEvent::InteractionOnly,
            cx,
        );
    }

    fn delete_saved_layout_from_palette(&mut self, layout_name: &str, cx: &mut Context<Self>) {
        match self.delete_named_layout(layout_name) {
            Ok(()) => {
                self.close_command_palette(cx);
                termy_toast::success(format!("Deleted layout \"{}\"", layout_name));
                self.notify_overlay(cx);
            }
            Err(error) => {
                termy_toast::error(error);
                self.notify_overlay(cx);
            }
        }
    }

    fn command_palette_disabled_action_message_for_state(
        action: CommandAction,
        install_cli_available: bool,
        tmux_enabled: bool,
    ) -> &'static str {
        let availability = Self::command_palette_action_availability_for_state(
            action,
            install_cli_available,
            tmux_enabled,
        );

        match availability.reason {
            Some(CommandUnavailableReason::RequiresTmuxRuntime) => {
                "Attach a tmux session to use this command"
            }
            Some(CommandUnavailableReason::InstallCliAlreadyInstalled) => {
                "CLI is already installed"
            }
            None => "Command is currently unavailable",
        }
    }

    fn select_theme_from_palette(&mut self, theme_id: &str, cx: &mut Context<Self>) {
        match self.persist_theme_selection(theme_id, cx) {
            Ok(true) => {
                self.close_command_palette(cx);
                termy_toast::success(format!("Theme set to {}", self.theme_id));
                self.notify_overlay(cx);
            }
            Ok(false) => {
                self.close_command_palette(cx);
                termy_toast::info(format!("Theme already set to {}", theme_id));
                self.notify_overlay(cx);
            }
            Err(error) => {
                termy_toast::error(error);
                self.notify_overlay(cx);
            }
        }
    }

    fn execute_command_palette_action(
        &mut self,
        action: CommandAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keep_open = Self::command_palette_should_stay_open(action);
        if !keep_open {
            self.close_command_palette(cx);
        }

        self.execute_command_action(action, false, window, cx);

        if keep_open {
            return;
        }

        match action {
            CommandAction::OpenConfig => {
                termy_toast::info("Opened settings file");
                self.notify_overlay(cx);
            }
            CommandAction::PrettifyConfig => {
                termy_toast::success("Prettified settings file");
                self.notify_overlay(cx);
            }
            CommandAction::NewTab => termy_toast::success("Opened new tab"),
            CommandAction::CloseTab => termy_toast::info("Closed active tab"),
            CommandAction::ClosePaneOrTab => termy_toast::info("Closed active pane or tab"),
            CommandAction::ZoomIn => termy_toast::info("Zoomed in"),
            CommandAction::ZoomOut => termy_toast::info("Zoomed out"),
            CommandAction::ZoomReset => termy_toast::info("Zoom reset"),
            CommandAction::ImportThemeStoreAuth | CommandAction::ImportColors => {}
            CommandAction::Quit
            | CommandAction::SwitchTheme
            | CommandAction::ManageTmuxSessions
            | CommandAction::ManageSavedLayouts
            | CommandAction::RunTask
            | CommandAction::AppInfo
            | CommandAction::RestartApp
            | CommandAction::RenameTab
            | CommandAction::MoveTabLeft
            | CommandAction::MoveTabRight
            | CommandAction::SwitchTabLeft
            | CommandAction::SwitchTabRight
            | CommandAction::SwitchToTab1
            | CommandAction::SwitchToTab2
            | CommandAction::SwitchToTab3
            | CommandAction::SwitchToTab4
            | CommandAction::SwitchToTab5
            | CommandAction::SwitchToTab6
            | CommandAction::SwitchToTab7
            | CommandAction::SwitchToTab8
            | CommandAction::SwitchToTab9
            | CommandAction::SplitPaneVertical
            | CommandAction::SplitPaneHorizontal
            | CommandAction::ClosePane
            | CommandAction::FocusPaneLeft
            | CommandAction::FocusPaneRight
            | CommandAction::FocusPaneUp
            | CommandAction::FocusPaneDown
            | CommandAction::FocusPaneNext
            | CommandAction::FocusPanePrevious
            | CommandAction::ResizePaneLeft
            | CommandAction::ResizePaneRight
            | CommandAction::ResizePaneUp
            | CommandAction::ResizePaneDown
            | CommandAction::TogglePaneZoom
            | CommandAction::CheckForUpdates
            | CommandAction::ToggleCommandPalette
            | CommandAction::Copy
            | CommandAction::Paste
            | CommandAction::OpenSearch
            | CommandAction::CloseSearch
            | CommandAction::SearchNext
            | CommandAction::SearchPrevious
            | CommandAction::ToggleSearchCaseSensitive
            | CommandAction::ToggleSearchRegex
            | CommandAction::OpenSettings
            | CommandAction::MinimizeWindow
            | CommandAction::InstallCli
            | CommandAction::ToggleAiInput
            | CommandAction::ToggleAgentSidebar
            | CommandAction::ToggleVerticalTabSidebar => {}
        }
    }

    fn execute_plugin_command_palette_action(
        &mut self,
        plugin_id: &str,
        command_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.close_command_palette(cx);
        match crate::plugins::invoke_plugin_command(plugin_id, command_id) {
            Ok(()) => {
                termy_toast::success(format!("Ran {}", command_id));
                self.notify_overlay(cx);
            }
            Err(error) => {
                termy_toast::error(error);
                self.notify_overlay(cx);
            }
        }
    }

    fn command_palette_should_stay_open(action: CommandAction) -> bool {
        matches!(
            action,
            CommandAction::SwitchTheme
                | CommandAction::ManageTmuxSessions
                | CommandAction::ManageSavedLayouts
                | CommandAction::RunTask
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_action_is_mode_dependent() {
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::Commands,
                TmuxSessionIntent::AttachOrSwitch,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
                TaskIntent::Browse,
            ),
            CommandPaletteEscapeAction::ClosePalette
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::Themes,
                TmuxSessionIntent::AttachOrSwitch,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
                TaskIntent::Browse,
            ),
            CommandPaletteEscapeAction::BackToCommands
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::TmuxSessions,
                TmuxSessionIntent::AttachOrSwitch,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
                TaskIntent::Browse,
            ),
            CommandPaletteEscapeAction::BackToCommands
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::TmuxSessions,
                TmuxSessionIntent::RenameInput,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
                TaskIntent::Browse,
            ),
            CommandPaletteEscapeAction::BackToTmuxRenameSelect
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::Tasks,
                TmuxSessionIntent::AttachOrSwitch,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
                TaskIntent::CreateGlobalInput,
            ),
            CommandPaletteEscapeAction::BackToTaskBrowse
        );
    }

    #[test]
    fn nav_key_parser_maps_expected_keys() {
        assert_eq!(
            CommandPaletteNavKey::parse("escape"),
            Some(CommandPaletteNavKey::Escape)
        );
        assert_eq!(
            CommandPaletteNavKey::parse("enter"),
            Some(CommandPaletteNavKey::Enter)
        );
        assert_eq!(
            CommandPaletteNavKey::parse("up"),
            Some(CommandPaletteNavKey::Up)
        );
        assert_eq!(
            CommandPaletteNavKey::parse("down"),
            Some(CommandPaletteNavKey::Down)
        );
        assert_eq!(CommandPaletteNavKey::parse("left"), None);
    }

    #[test]
    fn palette_mode_actions_keep_palette_open() {
        assert!(TerminalView::command_palette_should_stay_open(
            CommandAction::SwitchTheme
        ));
        assert!(TerminalView::command_palette_should_stay_open(
            CommandAction::ManageTmuxSessions
        ));
        assert!(TerminalView::command_palette_should_stay_open(
            CommandAction::ManageSavedLayouts
        ));
        assert!(TerminalView::command_palette_should_stay_open(
            CommandAction::RunTask
        ));
        assert!(!TerminalView::command_palette_should_stay_open(
            CommandAction::NewTab
        ));
    }

    #[test]
    fn notify_target_routes_overlay_only_palette_interactions() {
        assert_eq!(
            TerminalView::command_palette_notify_target_for_event(
                CommandPaletteNotifyEvent::OpenCloseTransition
            ),
            CommandPaletteNotifyTarget::Parent
        );
        assert_eq!(
            TerminalView::command_palette_notify_target_for_event(
                CommandPaletteNotifyEvent::InteractionOnly
            ),
            CommandPaletteNotifyTarget::Overlay
        );
    }

    #[test]
    fn install_cli_command_is_present_and_tracks_availability_state() {
        let available_items =
            TerminalView::command_palette_core_command_items_for_state(true, true);
        let unavailable_items =
            TerminalView::command_palette_core_command_items_for_state(false, true);

        let available_install_cli = available_items
            .iter()
            .find(|item| {
                matches!(
                    item.kind,
                    CommandPaletteItemKind::Command(CommandAction::InstallCli)
                )
            })
            .expect("missing Install CLI in available command palette state");
        assert!(available_install_cli.enabled);
        assert_eq!(available_install_cli.status_hint, None);

        let unavailable_install_cli = unavailable_items
            .iter()
            .find(|item| {
                matches!(
                    item.kind,
                    CommandPaletteItemKind::Command(CommandAction::InstallCli)
                )
            })
            .expect("missing Install CLI in unavailable command palette state");
        assert!(!unavailable_install_cli.enabled);
        assert_eq!(unavailable_install_cli.status_hint, Some("Installed"));
    }

    #[test]
    fn tmux_query_surfaces_only_tmux_sessions_entry() {
        let items = TerminalView::command_palette_core_command_items_for_state(true, true);
        let filtered_indices =
            super::state::filter_command_palette_item_indices_by_query(&items, "tmux");
        let filtered_actions = filtered_indices
            .into_iter()
            .filter_map(|index| match items[index].kind {
                CommandPaletteItemKind::Command(action) => Some(action),
                _ => None,
            })
            .collect::<Vec<_>>();

        #[cfg(not(target_os = "windows"))]
        assert_eq!(filtered_actions, vec![CommandAction::ManageTmuxSessions]);
        #[cfg(target_os = "windows")]
        assert!(filtered_actions.is_empty());
    }

    #[test]
    fn resize_commands_remain_available_when_tmux_runtime_is_off() {
        let items = TerminalView::command_palette_core_command_items_for_state(false, false);
        let resize = items.iter().find(|item| {
            matches!(
                item.kind,
                CommandPaletteItemKind::Command(CommandAction::ResizePaneLeft)
            )
        });
        #[cfg(not(target_os = "windows"))]
        {
            let resize = resize.expect("missing resize pane command");
            assert!(resize.enabled);
            assert_eq!(resize.status_hint, None);
        }
        #[cfg(target_os = "windows")]
        assert!(
            resize.is_none(),
            "resize pane command should be hidden from Windows command palette"
        );
    }

    #[test]
    fn install_cli_disabled_message_matches_expected_copy() {
        assert_eq!(
            TerminalView::command_palette_disabled_action_message_for_state(
                CommandAction::InstallCli,
                false,
                true,
            ),
            "CLI is already installed"
        );
    }

    #[test]
    fn unknown_disabled_message_matches_expected_copy() {
        assert_eq!(
            TerminalView::command_palette_disabled_action_message_for_state(
                CommandAction::ResizePaneLeft,
                true,
                false,
            ),
            "Command is currently unavailable"
        );
    }

    #[test]
    fn saved_layout_tasks_item_requires_browse_mode_empty_query_and_matching_tasks() {
        let item = TerminalView::saved_layout_tasks_item_for_state(
            SavedLayoutIntent::Browse,
            "",
            Some("dashboard"),
            true,
        )
        .expect("saved layout tasks item");
        assert_eq!(item.title, "Run Tasks for \"dashboard\"");
        assert_eq!(
            item.kind,
            CommandPaletteItemKind::SavedLayoutOpenTasksMode {
                layout_name: "dashboard".to_string(),
            }
        );

        assert_eq!(
            TerminalView::saved_layout_tasks_item_for_state(
                SavedLayoutIntent::Browse,
                "dash",
                Some("dashboard"),
                true,
            ),
            None
        );
        assert_eq!(
            TerminalView::saved_layout_tasks_item_for_state(
                SavedLayoutIntent::SaveInput,
                "",
                Some("dashboard"),
                true,
            ),
            None
        );
        assert_eq!(
            TerminalView::saved_layout_tasks_item_for_state(
                SavedLayoutIntent::Browse,
                "",
                Some("dashboard"),
                false,
            ),
            None
        );
    }

    #[test]
    fn palette_task_name_validation_rejects_dot_names() {
        assert!(TerminalView::palette_task_name_is_valid("build"));
        assert!(TerminalView::palette_task_name_is_valid(" build "));
        assert!(!TerminalView::palette_task_name_is_valid("build.web"));
    }
}
