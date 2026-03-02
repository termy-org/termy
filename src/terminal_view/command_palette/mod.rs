use super::*;
use gpui::point;
use state::{
    CommandPaletteItem, CommandPaletteItemKind, command_palette_next_scroll_y,
    command_palette_target_scroll_y, ordered_theme_ids_for_palette,
};
use termy_command_core::{CommandAvailability, CommandCapabilities, CommandUnavailableReason};

mod render;
mod state;
mod state_tmux;
pub(super) mod style;
mod tmux_sessions;

pub(super) use state::{CommandPaletteMode, CommandPaletteState};
pub(super) use state_tmux::TmuxSessionIntent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteEscapeAction {
    ClosePalette,
    BackToCommands,
    BackToTmuxRenameSelect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteNavKey {
    Escape,
    Enter,
    Up,
    Down,
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
        ai_features_enabled: bool,
    ) -> CommandAvailability {
        action.availability(CommandCapabilities {
            tmux_runtime_active: tmux_enabled,
            install_cli_available,
            ai_features_enabled,
        })
    }

    fn command_palette_status_hint_for_unavailable_reason(
        reason: CommandUnavailableReason,
    ) -> &'static str {
        match reason {
            CommandUnavailableReason::RequiresTmuxRuntime => "tmux required",
            CommandUnavailableReason::InstallCliAlreadyInstalled => "Installed",
            CommandUnavailableReason::AiFeaturesDisabled => "AI disabled",
        }
    }

    fn command_palette_command_item_for_state(
        action: CommandAction,
        title: &str,
        keywords: &str,
        install_cli_available: bool,
        tmux_enabled: bool,
        ai_features_enabled: bool,
    ) -> CommandPaletteItem {
        let availability = Self::command_palette_action_availability_for_state(
            action,
            install_cli_available,
            tmux_enabled,
            ai_features_enabled,
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

    fn command_palette_command_items_for_state(
        install_cli_available: bool,
        tmux_enabled: bool,
        ai_features_enabled: bool,
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
                    ai_features_enabled,
                )
            })
            .collect()
    }

    fn command_palette_items_for_mode(&mut self, mode: CommandPaletteMode) -> Vec<CommandPaletteItem> {
        match mode {
            CommandPaletteMode::Commands => Self::command_palette_command_items_for_state(
                self.install_cli_available(),
                self.runtime_uses_tmux(),
                self.ai_features_enabled(),
            ),
            CommandPaletteMode::Themes => self.command_palette_theme_items(),
            CommandPaletteMode::TmuxSessions => self.command_palette.tmux_session_items_for_query(
                self.command_palette.input().text(),
                self.tmux_active_session_name_for_session_palette()
                    .as_deref(),
            ),
        }
    }

    fn command_palette_theme_items(&self) -> Vec<CommandPaletteItem> {
        let theme_ids: Vec<String> = termy_themes::available_theme_ids()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();

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
        cx: &mut Context<Self>,
    ) {
        self.command_palette.clear_shortcut_cache();
        if mode == CommandPaletteMode::TmuxSessions {
            if let Err(error) = self.reload_tmux_session_palette_items() {
                // Keep the tmux session palette usable when list-sessions fails by
                // preserving the selected socket target and rendering intent-specific rows.
                self.command_palette.set_tmux_session_rows(
                    Vec::new(),
                    self.tmux_primary_socket_target_for_session_palette(),
                );
                termy_toast::error(format!("Failed to list tmux sessions: {error}"));
            }
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
        cx.notify();
    }

    pub(super) fn set_command_palette_mode(
        &mut self,
        mode: CommandPaletteMode,
        animate_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.set_mode(mode);
        self.apply_command_palette_mode_setup(mode, animate_selection, cx);
    }

    pub(super) fn open_command_palette_in_mode(
        &mut self,
        mode: CommandPaletteMode,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.open(mode);
        self.apply_command_palette_mode_setup(mode, false, cx);
    }

    pub(super) fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.open_command_palette_in_mode(CommandPaletteMode::Commands, cx);
    }

    pub(super) fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        if !self.command_palette.is_open() {
            return;
        }

        self.command_palette.close();
        self.inline_input_selecting = false;
        cx.notify();
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
        self.apply_command_palette_mode_setup(mode, false, cx);
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
                            cx.notify();
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
                    cx.notify();
                }
            }
            CommandPaletteNavKey::Down => {
                let len = self.command_palette.filtered_len();
                if self.command_palette.move_selection_down() {
                    self.animate_command_palette_to_selected(len, cx);
                    cx.notify();
                }
            }
        }
    }

    fn command_palette_escape_action(
        mode: CommandPaletteMode,
        tmux_session_intent: TmuxSessionIntent,
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
                        self.ai_features_enabled(),
                    ));
                    cx.notify();
                    return;
                }
                self.execute_command_palette_action(action, window, cx)
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
        }
    }

    fn command_palette_disabled_action_message_for_state(
        action: CommandAction,
        install_cli_available: bool,
        tmux_enabled: bool,
        ai_features_enabled: bool,
    ) -> &'static str {
        let availability = Self::command_palette_action_availability_for_state(
            action,
            install_cli_available,
            tmux_enabled,
            ai_features_enabled,
        );

        match availability.reason {
            Some(CommandUnavailableReason::RequiresTmuxRuntime) => {
                "Attach a tmux session to use this command"
            }
            Some(CommandUnavailableReason::InstallCliAlreadyInstalled) => {
                "CLI is already installed"
            }
            Some(CommandUnavailableReason::AiFeaturesDisabled) => {
                "AI features are disabled in settings"
            }
            None => "Command is currently unavailable",
        }
    }

    fn select_theme_from_palette(&mut self, theme_id: &str, cx: &mut Context<Self>) {
        match self.persist_theme_selection(theme_id, cx) {
            Ok(true) => {
                self.close_command_palette(cx);
                termy_toast::success(format!("Theme set to {}", self.theme_id));
                cx.notify();
            }
            Ok(false) => {
                self.close_command_palette(cx);
                termy_toast::info(format!("Theme already set to {}", theme_id));
            }
            Err(error) => {
                termy_toast::error(error);
                cx.notify();
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
                cx.notify();
            }
            CommandAction::NewTab => termy_toast::success("Opened new tab"),
            CommandAction::CloseTab => termy_toast::info("Closed active tab"),
            CommandAction::ClosePaneOrTab => termy_toast::info("Closed active pane or tab"),
            CommandAction::ZoomIn => termy_toast::info("Zoomed in"),
            CommandAction::ZoomOut => termy_toast::info("Zoomed out"),
            CommandAction::ZoomReset => termy_toast::info("Zoom reset"),
            CommandAction::ImportColors => {}
            CommandAction::Quit
            | CommandAction::SwitchTheme
            | CommandAction::ManageTmuxSessions
            | CommandAction::AppInfo
            | CommandAction::NativeSdkExample
            | CommandAction::RestartApp
            | CommandAction::RenameTab
            | CommandAction::MoveTabLeft
            | CommandAction::MoveTabRight
            | CommandAction::SwitchTabLeft
            | CommandAction::SwitchTabRight
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
            | CommandAction::ToggleChatSidebar => {}
        }
    }

    fn command_palette_should_stay_open(action: CommandAction) -> bool {
        matches!(
            action,
            CommandAction::SwitchTheme | CommandAction::ManageTmuxSessions
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
            ),
            CommandPaletteEscapeAction::ClosePalette
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::Themes,
                TmuxSessionIntent::AttachOrSwitch,
            ),
            CommandPaletteEscapeAction::BackToCommands
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::TmuxSessions,
                TmuxSessionIntent::AttachOrSwitch,
            ),
            CommandPaletteEscapeAction::BackToCommands
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(
                CommandPaletteMode::TmuxSessions,
                TmuxSessionIntent::RenameInput,
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
        assert!(!TerminalView::command_palette_should_stay_open(
            CommandAction::NewTab
        ));
    }

    #[test]
    fn install_cli_command_is_present_and_tracks_availability_state() {
        let available_items =
            TerminalView::command_palette_command_items_for_state(true, true, true);
        let unavailable_items =
            TerminalView::command_palette_command_items_for_state(false, true, true);

        let available_install_cli = available_items
            .iter()
            .find_map(|item| match item.kind {
                CommandPaletteItemKind::Command(CommandAction::InstallCli) => Some(item),
                _ => None,
            })
            .expect("missing Install CLI in available command palette state");
        assert!(available_install_cli.enabled);
        assert_eq!(available_install_cli.status_hint, None);

        let unavailable_install_cli = unavailable_items
            .iter()
            .find_map(|item| match item.kind {
                CommandPaletteItemKind::Command(CommandAction::InstallCli) => Some(item),
                _ => None,
            })
            .expect("missing Install CLI in unavailable command palette state");
        assert!(!unavailable_install_cli.enabled);
        assert_eq!(unavailable_install_cli.status_hint, Some("Installed"));
    }

    #[test]
    fn tmux_query_surfaces_only_tmux_sessions_entry() {
        let items = TerminalView::command_palette_command_items_for_state(true, true, true);
        let filtered_indices =
            super::state::filter_command_palette_item_indices_by_query(&items, "tmux");
        let filtered_actions = filtered_indices
            .into_iter()
            .filter_map(|index| match items[index].kind {
                CommandPaletteItemKind::Command(action) => Some(action),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(filtered_actions, vec![CommandAction::ManageTmuxSessions]);
    }

    #[test]
    fn tmux_commands_are_present_but_disabled_when_tmux_runtime_is_off() {
        let items = TerminalView::command_palette_command_items_for_state(false, false, true);
        let split = items
            .iter()
            .find_map(|item| match item.kind {
                CommandPaletteItemKind::Command(CommandAction::SplitPaneVertical) => Some(item),
                _ => None,
            })
            .expect("missing split pane command");
        assert!(!split.enabled);
        assert_eq!(split.status_hint, Some("tmux required"));
    }

    #[test]
    fn ai_commands_are_present_but_disabled_when_ai_features_are_off() {
        let items = TerminalView::command_palette_command_items_for_state(true, true, false);
        let ai_input = items
            .iter()
            .find_map(|item| match item.kind {
                CommandPaletteItemKind::Command(CommandAction::ToggleAiInput) => Some(item),
                _ => None,
            })
            .expect("missing AI input command");
        assert!(!ai_input.enabled);
        assert_eq!(ai_input.status_hint, Some("AI disabled"));

        let chat_sidebar = items
            .iter()
            .find_map(|item| match item.kind {
                CommandPaletteItemKind::Command(CommandAction::ToggleChatSidebar) => Some(item),
                _ => None,
            })
            .expect("missing chat sidebar command");
        assert!(!chat_sidebar.enabled);
        assert_eq!(chat_sidebar.status_hint, Some("AI disabled"));
    }

    #[test]
    fn install_cli_disabled_message_matches_expected_copy() {
        assert_eq!(
            TerminalView::command_palette_disabled_action_message_for_state(
                CommandAction::InstallCli,
                false,
                true,
                true,
            ),
            "CLI is already installed"
        );
    }

    #[test]
    fn tmux_disabled_message_matches_expected_copy() {
        assert_eq!(
            TerminalView::command_palette_disabled_action_message_for_state(
                CommandAction::SplitPaneVertical,
                true,
                false,
                true,
            ),
            "Attach a tmux session to use this command"
        );
    }

    #[test]
    fn ai_disabled_message_matches_expected_copy() {
        assert_eq!(
            TerminalView::command_palette_disabled_action_message_for_state(
                CommandAction::ToggleAiInput,
                true,
                true,
                false,
            ),
            "AI features are disabled in settings"
        );
    }
}
