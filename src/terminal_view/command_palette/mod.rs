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

pub(super) use state::{CommandPaletteMode, CommandPaletteState};
pub(super) use state_layouts::SavedLayoutIntent;
pub(super) use state_tmux::TmuxSessionIntent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteEscapeAction {
    ClosePalette,
    BackToCommands,
    BackToTmuxRenameSelect,
    BackToSavedLayoutRenameSelect,
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
            CommandPaletteMode::Layouts => self
                .command_palette
                .saved_layout_items_for_query(self.command_palette.input().text()),
        }
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
            let items = self
                .command_palette
                .saved_layout_items_for_query(self.command_palette.input().text());
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
            ),
            CommandPaletteEscapeAction::ClosePalette
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::Themes,
                TmuxSessionIntent::AttachOrSwitch,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
            ),
            CommandPaletteEscapeAction::BackToCommands
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::TmuxSessions,
                TmuxSessionIntent::AttachOrSwitch,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
            ),
            CommandPaletteEscapeAction::BackToCommands
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::TmuxSessions,
                TmuxSessionIntent::RenameInput,
                CommandPaletteCommandIntent::Browse,
                SavedLayoutIntent::Browse,
            ),
            CommandPaletteEscapeAction::BackToTmuxRenameSelect
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
}
