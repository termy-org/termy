use super::*;
use gpui::point;
use state::{
    command_palette_next_scroll_y, command_palette_target_scroll_y,
    ordered_theme_ids_for_palette, CommandPaletteItem, CommandPaletteItemKind,
};

mod render;
mod state;
mod style;

pub(super) use state::{CommandPaletteMode, CommandPaletteState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteEscapeAction {
    ClosePalette,
    BackToCommands,
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
        self.command_palette.cache_shortcut(action, shortcut.clone());
        shortcut
    }

    fn command_palette_action_available_for_state(
        action: CommandAction,
        install_cli_available: bool,
    ) -> bool {
        match action {
            CommandAction::InstallCli => install_cli_available,
            _ => true,
        }
    }

    fn command_palette_action_status_hint_for_state(
        action: CommandAction,
        install_cli_available: bool,
    ) -> Option<&'static str> {
        match action {
            CommandAction::InstallCli if !install_cli_available => Some("Installed"),
            _ => None,
        }
    }

    fn command_palette_command_item_for_state(
        action: CommandAction,
        title: &str,
        keywords: &str,
        install_cli_available: bool,
    ) -> CommandPaletteItem {
        let enabled = Self::command_palette_action_available_for_state(action, install_cli_available);
        let status_hint =
            Self::command_palette_action_status_hint_for_state(action, install_cli_available);
        CommandPaletteItem::command_with_state(title, keywords, action, enabled, status_hint)
    }

    fn command_palette_command_items_for_state(
        install_cli_available: bool,
        tmux_enabled: bool,
    ) -> Vec<CommandPaletteItem> {
        CommandAction::palette_entries()
            .into_iter()
            .filter(|entry| tmux_enabled || !entry.action.requires_tmux())
            .map(|entry| {
                Self::command_palette_command_item_for_state(
                    entry.action,
                    entry.title,
                    entry.keywords,
                    install_cli_available,
                )
            })
            .collect()
    }

    fn command_palette_items_for_mode(&self, mode: CommandPaletteMode) -> Vec<CommandPaletteItem> {
        match mode {
            CommandPaletteMode::Commands => {
                Self::command_palette_command_items_for_state(
                    self.install_cli_available(),
                    self.runtime_uses_tmux(),
                )
            }
            CommandPaletteMode::Themes => self.command_palette_theme_items(),
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
        self.command_palette.refilter_current_query();
        let len = self.command_palette.filtered_len();

        if len == 0 {
            self.command_palette.reset_scroll_animation_state();
            return;
        }

        if animate_selection {
            self.animate_command_palette_to_selected(len, cx);
        }
    }

    pub(super) fn refresh_command_palette_items_for_current_mode(&mut self, cx: &mut Context<Self>) {
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
                match Self::command_palette_escape_action(self.command_palette.mode()) {
                    CommandPaletteEscapeAction::ClosePalette => self.close_command_palette(cx),
                    CommandPaletteEscapeAction::BackToCommands => {
                        self.set_command_palette_mode(CommandPaletteMode::Commands, false, cx);
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

    fn command_palette_escape_action(mode: CommandPaletteMode) -> CommandPaletteEscapeAction {
        match mode {
            CommandPaletteMode::Commands => CommandPaletteEscapeAction::ClosePalette,
            CommandPaletteMode::Themes => CommandPaletteEscapeAction::BackToCommands,
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

        self.command_palette.set_selected_filtered_index(filtered_index);
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
                    termy_toast::info(Self::command_palette_disabled_action_message(action));
                    cx.notify();
                    return;
                }
                self.execute_command_palette_action(action, window, cx)
            }
            CommandPaletteItemKind::Theme(theme_id) => {
                self.select_theme_from_palette(theme_id.as_str(), cx)
            }
        }
    }

    fn command_palette_disabled_action_message(action: CommandAction) -> &'static str {
        match action {
            CommandAction::InstallCli => "CLI is already installed",
            _ => "Command is currently unavailable",
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
            CommandAction::ZoomIn => termy_toast::info("Zoomed in"),
            CommandAction::ZoomOut => termy_toast::info("Zoomed out"),
            CommandAction::ZoomReset => termy_toast::info("Zoom reset"),
            CommandAction::ImportColors => {}
            CommandAction::Quit
            | CommandAction::SwitchTheme
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
            | CommandAction::InstallCli => {}
        }
    }

    fn command_palette_should_stay_open(action: CommandAction) -> bool {
        action == CommandAction::SwitchTheme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_action_is_mode_dependent() {
        assert_eq!(
            TerminalView::command_palette_escape_action(CommandPaletteMode::Commands),
            CommandPaletteEscapeAction::ClosePalette
        );
        assert_eq!(
            TerminalView::command_palette_escape_action(CommandPaletteMode::Themes),
            CommandPaletteEscapeAction::BackToCommands
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
        assert_eq!(CommandPaletteNavKey::parse("up"), Some(CommandPaletteNavKey::Up));
        assert_eq!(
            CommandPaletteNavKey::parse("down"),
            Some(CommandPaletteNavKey::Down)
        );
        assert_eq!(CommandPaletteNavKey::parse("left"), None);
    }

    #[test]
    fn switch_theme_is_the_only_action_that_keeps_palette_open() {
        assert!(TerminalView::command_palette_should_stay_open(
            CommandAction::SwitchTheme
        ));
        assert!(!TerminalView::command_palette_should_stay_open(
            CommandAction::NewTab
        ));
    }

    #[test]
    fn install_cli_command_is_present_and_tracks_availability_state() {
        let available_items = TerminalView::command_palette_command_items_for_state(true, true);
        let unavailable_items = TerminalView::command_palette_command_items_for_state(false, true);

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
    fn tmux_commands_are_hidden_when_tmux_is_disabled() {
        let items = TerminalView::command_palette_command_items_for_state(true, false);
        assert!(!items.iter().any(|item| matches!(
            item.kind,
            CommandPaletteItemKind::Command(CommandAction::NewTab
                | CommandAction::CloseTab
                | CommandAction::SplitPaneVertical
                | CommandAction::SplitPaneHorizontal
                | CommandAction::TogglePaneZoom
                | CommandAction::RenameTab)
        )));
    }

    #[test]
    fn install_cli_disabled_message_matches_expected_copy() {
        assert_eq!(
            TerminalView::command_palette_disabled_action_message(CommandAction::InstallCli),
            "CLI is already installed"
        );
    }
}
