use super::*;
use crate::ui::scrollbar::{self, ScrollbarPaintStyle, ScrollbarRange};
use gpui::{point, uniform_list};
use std::ops::Range;

impl CommandPaletteItem {
    fn command(title: &str, keywords: &str, action: CommandAction) -> Self {
        Self {
            title: title.to_string(),
            keywords: keywords.to_string(),
            kind: CommandPaletteItemKind::Command(action),
        }
    }

    fn theme(theme_id: String, is_active: bool) -> Self {
        let title = if is_active {
            format!("✓ {}", theme_id)
        } else {
            theme_id.clone()
        };
        let keywords = format!("theme palette colors {}", theme_id.replace('-', " "));

        Self {
            title,
            keywords,
            kind: CommandPaletteItemKind::Theme(theme_id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteEscapeAction {
    ClosePalette,
    BackToCommands,
}

impl TerminalView {
    fn command_palette_base_scroll_handle(&self) -> gpui::ScrollHandle {
        self.command_palette_scroll_handle
            .0
            .borrow()
            .base_handle
            .clone()
    }

    fn reset_command_palette_scroll_animation_state(&mut self) {
        self.command_palette_scroll_target_y = None;
        self.command_palette_scroll_max_y = 0.0;
        self.command_palette_scroll_animating = false;
        self.command_palette_scroll_last_tick = None;
    }

    fn reset_command_palette_state(&mut self) {
        self.command_palette_input.clear();
        self.command_palette_filtered_items.clear();
        self.command_palette_selected = 0;
        self.command_palette_scroll_handle = UniformListScrollHandle::new();
        self.reset_command_palette_scroll_animation_state();
        self.inline_input_selecting = false;
    }

    fn command_palette_shortcut(&self, action: CommandAction, window: &Window) -> Option<String> {
        if !self.command_palette_show_keybinds {
            return None;
        }

        action.keybinding_label(window, &self.focus_handle)
    }

    pub(super) fn set_command_palette_mode(
        &mut self,
        mode: CommandPaletteMode,
        animate_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.command_palette_mode = mode;
        self.reset_command_palette_state();
        self.refresh_command_palette_matches(animate_selection, cx);
        self.reset_cursor_blink_phase();

        cx.notify();
    }

    pub(super) fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette_open = true;
        self.set_command_palette_mode(CommandPaletteMode::Commands, false, cx);
    }

    pub(super) fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        if !self.command_palette_open {
            return;
        }

        self.command_palette_open = false;
        self.command_palette_mode = CommandPaletteMode::Commands;
        self.reset_command_palette_state();
        cx.notify();
    }

    fn command_palette_items(&self) -> Vec<CommandPaletteItem> {
        match self.command_palette_mode {
            CommandPaletteMode::Commands => CommandAction::palette_entries(self.use_tabs)
                .into_iter()
                .map(|entry| CommandPaletteItem::command(entry.title, entry.keywords, entry.action))
                .collect(),
            CommandPaletteMode::Themes => self.command_palette_theme_items(),
        }
    }

    fn command_palette_theme_items(&self) -> Vec<CommandPaletteItem> {
        let theme_ids: Vec<String> = termy_themes::available_theme_ids()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();

        Self::ordered_theme_ids_for_palette(theme_ids, &self.theme_id)
            .into_iter()
            .map(|theme| {
                let is_active = theme == self.theme_id;
                CommandPaletteItem::theme(theme, is_active)
            })
            .collect()
    }

    fn ordered_theme_ids_for_palette(
        mut theme_ids: Vec<String>,
        current_theme: &str,
    ) -> Vec<String> {
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

    pub(super) fn filtered_command_palette_items(&self) -> &[CommandPaletteItem] {
        &self.command_palette_filtered_items
    }

    pub(super) fn refresh_command_palette_matches(
        &mut self,
        animate_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.command_palette_filtered_items = Self::filter_command_palette_items_by_query(
            self.command_palette_items(),
            self.command_palette_query(),
        );
        let len = self.command_palette_filtered_items.len();
        self.clamp_command_palette_selection(len);

        if len == 0 {
            self.reset_command_palette_scroll_animation_state();
            return;
        }

        if animate_selection {
            self.animate_command_palette_to_selected(len, cx);
        }
    }

    fn filter_command_palette_items_by_query(
        items: Vec<CommandPaletteItem>,
        query: &str,
    ) -> Vec<CommandPaletteItem> {
        let query = query.trim().to_ascii_lowercase();
        let query_terms: Vec<String> = query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        if query_terms.is_empty() {
            return items;
        }

        let has_title_matches = items
            .iter()
            .any(|item| Self::command_palette_text_matches_terms(&item.title, &query_terms));

        items
            .into_iter()
            .filter(|item| {
                let title_match =
                    Self::command_palette_text_matches_terms(&item.title, &query_terms);
                if has_title_matches {
                    title_match
                } else {
                    title_match
                        || Self::command_palette_text_matches_terms(&item.keywords, &query_terms)
                }
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

    pub(super) fn clamp_command_palette_selection(&mut self, len: usize) {
        if len == 0 {
            self.command_palette_selected = 0;
        } else if self.command_palette_selected >= len {
            self.command_palette_selected = len - 1;
        }
    }

    fn command_palette_viewport_height() -> f32 {
        COMMAND_PALETTE_MAX_ITEMS as f32 * COMMAND_PALETTE_ROW_HEIGHT
    }

    fn command_palette_max_scroll_for_count(item_count: usize) -> f32 {
        (item_count as f32 * COMMAND_PALETTE_ROW_HEIGHT - Self::command_palette_viewport_height())
            .max(0.0)
    }

    fn command_palette_target_scroll_y(
        current_y: f32,
        selected_index: usize,
        item_count: usize,
    ) -> Option<f32> {
        if item_count == 0 {
            return None;
        }

        let viewport_height = Self::command_palette_viewport_height();
        let max_scroll = Self::command_palette_max_scroll_for_count(item_count);
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

    fn command_palette_next_scroll_y(
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

    pub(super) fn animate_command_palette_to_selected(
        &mut self,
        item_count: usize,
        cx: &mut Context<Self>,
    ) {
        if item_count == 0 {
            self.reset_command_palette_scroll_animation_state();
            return;
        }

        let max_scroll = Self::command_palette_max_scroll_for_count(item_count);
        self.command_palette_scroll_max_y = max_scroll;

        let scroll_handle = self.command_palette_base_scroll_handle();
        let offset = scroll_handle.offset();
        let current_y = -Into::<f32>::into(offset.y);
        let Some(target_y) = Self::command_palette_target_scroll_y(
            current_y,
            self.command_palette_selected,
            item_count,
        ) else {
            self.reset_command_palette_scroll_animation_state();
            return;
        };

        if (target_y - current_y).abs() <= f32::EPSILON {
            self.command_palette_scroll_target_y = None;
            self.command_palette_scroll_animating = false;
            self.command_palette_scroll_last_tick = None;
            return;
        }

        self.command_palette_scroll_target_y = Some(target_y);
        self.start_command_palette_scroll_animation(cx);
    }

    fn start_command_palette_scroll_animation(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_scroll_animating {
            return;
        }
        self.command_palette_scroll_animating = true;
        self.command_palette_scroll_last_tick = Some(Instant::now());

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(16)).await;
                let keep_animating = match cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        let changed = view.tick_command_palette_scroll_animation();
                        if changed {
                            cx.notify();
                        }
                        view.command_palette_scroll_animating
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
        if !self.command_palette_open {
            self.reset_command_palette_scroll_animation_state();
            return false;
        }

        let Some(target_y) = self.command_palette_scroll_target_y else {
            self.command_palette_scroll_animating = false;
            self.command_palette_scroll_last_tick = None;
            return false;
        };

        let scroll_handle = self.command_palette_base_scroll_handle();
        let offset = scroll_handle.offset();
        let current_y = -Into::<f32>::into(offset.y);
        let max_offset_from_handle: f32 = scroll_handle.max_offset().height.into();
        let max_scroll = max_offset_from_handle
            .max(self.command_palette_scroll_max_y)
            .max(0.0);
        let now = Instant::now();
        let dt = self
            .command_palette_scroll_last_tick
            .map(|last| (now - last).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        self.command_palette_scroll_last_tick = Some(now);

        let next_y = Self::command_palette_next_scroll_y(current_y, target_y, max_scroll, dt);
        scroll_handle.set_offset(point(offset.x, px(-next_y)));

        if (target_y - next_y).abs() <= 0.5 {
            self.command_palette_scroll_target_y = None;
            self.command_palette_scroll_animating = false;
            self.command_palette_scroll_last_tick = None;
            return true;
        }

        true
    }

    pub(super) fn handle_command_palette_key_down(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "escape" => {
                match Self::command_palette_escape_action(self.command_palette_mode) {
                    CommandPaletteEscapeAction::ClosePalette => self.close_command_palette(cx),
                    CommandPaletteEscapeAction::BackToCommands => {
                        self.set_command_palette_mode(CommandPaletteMode::Commands, false, cx);
                    }
                }
                return;
            }
            "enter" => {
                self.execute_command_palette_selection(cx);
                return;
            }
            "up" => {
                let len = self.filtered_command_palette_items().len();
                if len > 0 && self.command_palette_selected > 0 {
                    self.command_palette_selected -= 1;
                    self.animate_command_palette_to_selected(len, cx);
                    cx.notify();
                }
                return;
            }
            "down" => {
                let len = self.filtered_command_palette_items().len();
                if len > 0 && self.command_palette_selected + 1 < len {
                    self.command_palette_selected += 1;
                    self.animate_command_palette_to_selected(len, cx);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }
    }

    fn command_palette_escape_action(mode: CommandPaletteMode) -> CommandPaletteEscapeAction {
        match mode {
            CommandPaletteMode::Commands => CommandPaletteEscapeAction::ClosePalette,
            CommandPaletteMode::Themes => CommandPaletteEscapeAction::BackToCommands,
        }
    }

    fn execute_command_palette_selection(&mut self, cx: &mut Context<Self>) {
        let items = self.filtered_command_palette_items();
        if items.is_empty() {
            return;
        }

        let index = self.command_palette_selected.min(items.len() - 1);
        let item_kind = items[index].kind.clone();

        self.execute_command_palette_item(item_kind, cx);
    }

    fn execute_command_palette_item(
        &mut self,
        item_kind: CommandPaletteItemKind,
        cx: &mut Context<Self>,
    ) {
        match item_kind {
            CommandPaletteItemKind::Command(action) => {
                self.execute_command_palette_action(action, cx)
            }
            CommandPaletteItemKind::Theme(theme_id) => {
                self.select_theme_from_palette(&theme_id, cx)
            }
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

    fn execute_command_palette_action(&mut self, action: CommandAction, cx: &mut Context<Self>) {
        let keep_open = action == CommandAction::SwitchTheme;
        if !keep_open {
            self.command_palette_open = false;
            self.command_palette_mode = CommandPaletteMode::Commands;
            self.reset_command_palette_state();
        }

        self.execute_command_action(action, false, cx);

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
            | CommandAction::OpenSettings => {}
        }
    }

    fn command_palette_scrollbar_metrics(
        &self,
        viewport_height: f32,
        item_count: usize,
    ) -> Option<scrollbar::ScrollbarMetrics> {
        let scroll_handle = self.command_palette_base_scroll_handle();
        let max_offset_from_handle: f32 = scroll_handle.max_offset().height.into();
        let estimated_content_height = item_count as f32 * COMMAND_PALETTE_ROW_HEIGHT;
        let estimated_max_offset = (estimated_content_height - viewport_height).max(0.0);
        let max_offset = max_offset_from_handle.max(estimated_max_offset);
        let offset_y: f32 = scroll_handle.offset().y.into();
        let offset = (-offset_y).max(0.0);
        let range = ScrollbarRange {
            offset,
            max_offset,
            viewport_extent: viewport_height,
            track_extent: viewport_height,
        };

        scrollbar::compute_metrics(range, COMMAND_PALETTE_SCROLLBAR_MIN_THUMB_HEIGHT)
    }

    fn render_command_palette_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let items = self.filtered_command_palette_items();
        let selected = if items.is_empty() {
            0
        } else {
            self.command_palette_selected.min(items.len() - 1)
        };
        let overlay_style = self.overlay_style();
        let selected_bg = overlay_style.panel_cursor(COMMAND_PALETTE_ROW_SELECTED_BG_ALPHA);
        let selected_border = overlay_style.panel_cursor(COMMAND_PALETTE_ROW_SELECTED_BORDER_ALPHA);
        let transparent = overlay_style.transparent_background();
        let primary_text = overlay_style.panel_foreground(OVERLAY_PRIMARY_TEXT_ALPHA);
        let shortcut_bg = overlay_style.panel_cursor(COMMAND_PALETTE_SHORTCUT_BG_ALPHA);
        let shortcut_border = overlay_style.panel_cursor(COMMAND_PALETTE_SHORTCUT_BORDER_ALPHA);
        let shortcut_text = overlay_style.panel_foreground(COMMAND_PALETTE_SHORTCUT_TEXT_ALPHA);

        let mut rows = Vec::with_capacity(range.len());
        for index in range {
            let Some(item) = items.get(index).cloned() else {
                continue;
            };

            let is_selected = index == selected;
            let shortcut = match item.kind {
                CommandPaletteItemKind::Command(action) => {
                    self.command_palette_shortcut(action, window)
                }
                CommandPaletteItemKind::Theme(_) => None,
            };
            let item_kind = item.kind.clone();

            rows.push(
                div()
                    .id(("command-palette-item", index))
                    .w_full()
                    .h(px(COMMAND_PALETTE_ROW_HEIGHT))
                    .px(px(10.0))
                    .rounded_sm()
                    .bg(if is_selected {
                        selected_bg
                    } else {
                        transparent
                    })
                    .border_1()
                    .border_color(if is_selected {
                        selected_border
                    } else {
                        transparent
                    })
                    .cursor_pointer()
                    .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                        if this.command_palette_selected != index {
                            this.command_palette_selected = index;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.command_palette_selected = index;
                            this.execute_command_palette_item(item_kind.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .text_size(px(12.0))
                    .text_color(primary_text)
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(div().flex_1().truncate().child(item.title.clone()))
                            .children(shortcut.map(|label| {
                                div()
                                    .flex_none()
                                    .h(px(20.0))
                                    .px(px(6.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .bg(shortcut_bg)
                                    .border_1()
                                    .border_color(shortcut_border)
                                    .text_size(px(10.0))
                                    .text_color(shortcut_text)
                                    .child(label)
                            })),
                    )
                    .into_any_element(),
            );
        }
        rows
    }

    pub(super) fn render_command_palette_modal(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let items = self.filtered_command_palette_items();
        let list_height = COMMAND_PALETTE_MAX_ITEMS as f32 * COMMAND_PALETTE_ROW_HEIGHT;
        let mode_title = match self.command_palette_mode {
            CommandPaletteMode::Commands => "Commands".to_string(),
            CommandPaletteMode::Themes => format!("Theme: {}", self.theme_id),
        };
        let footer_hint = match self.command_palette_mode {
            CommandPaletteMode::Commands => "Enter: Run  Esc: Close  Up/Down: Navigate",
            CommandPaletteMode::Themes => "Enter: Apply Theme  Esc: Back  Up/Down: Navigate",
        };
        let overlay_style = self.overlay_style();
        let overlay_bg = overlay_style.dim_background(COMMAND_PALETTE_DIM_ALPHA);
        let panel_bg = overlay_style.panel_background_with_floor(
            COMMAND_PALETTE_PANEL_BG_ALPHA,
            COMMAND_PALETTE_PANEL_SOLID_ALPHA,
        );
        let panel_border = overlay_style.panel_cursor(OVERLAY_PANEL_BORDER_ALPHA);
        let primary_text = overlay_style.panel_foreground(OVERLAY_PRIMARY_TEXT_ALPHA);
        let muted_text = overlay_style.panel_foreground(OVERLAY_MUTED_TEXT_ALPHA);
        let input_bg = overlay_style.panel_background_with_floor(
            COMMAND_PALETTE_INPUT_BG_ALPHA,
            COMMAND_PALETTE_INPUT_SOLID_ALPHA,
        );
        let input_font = Font {
            family: self.font_family.clone(),
            ..Font::default()
        };
        let input_selection = overlay_style.panel_cursor(COMMAND_PALETTE_INPUT_SELECTION_ALPHA);
        let scrollbar_track =
            self.scrollbar_color(overlay_style, COMMAND_PALETTE_SCROLLBAR_TRACK_ALPHA);
        let scrollbar_thumb =
            self.scrollbar_color(overlay_style, COMMAND_PALETTE_SCROLLBAR_THUMB_ALPHA);

        let list = if items.is_empty() {
            div()
                .w_full()
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(8.0))
                        .text_size(px(12.0))
                        .text_color(muted_text)
                        .child("No matching items"),
                )
                .into_any_element()
        } else {
            let list = uniform_list(
                "command-palette-list",
                items.len(),
                cx.processor(Self::render_command_palette_rows),
            )
            .flex_1()
            .h(px(list_height))
            .track_scroll(&self.command_palette_scroll_handle)
            .into_any_element();
            let mut list_container = div()
                .w_full()
                .h(px(list_height))
                .flex()
                .items_start()
                .child(list);

            if let Some(metrics) = self.command_palette_scrollbar_metrics(list_height, items.len())
            {
                let style = ScrollbarPaintStyle {
                    width: COMMAND_PALETTE_SCROLLBAR_WIDTH,
                    track_radius: 0.0,
                    thumb_radius: 0.0,
                    thumb_inset: 0.0,
                    marker_inset: 0.0,
                    marker_radius: 0.0,
                    track_color: scrollbar_track,
                    thumb_color: scrollbar_thumb,
                    active_thumb_color: scrollbar_thumb,
                    marker_color: None,
                    current_marker_color: None,
                };
                list_container = list_container.child(
                    div()
                        .w(px(COMMAND_PALETTE_SCROLLBAR_WIDTH + 4.0))
                        .h_full()
                        .pl(px(2.0))
                        .pr(px(2.0))
                        .child(scrollbar::render_vertical(
                            "command-palette-scrollbar",
                            metrics,
                            style,
                            false,
                            &[],
                            None,
                            0.0,
                        )),
                );
            }

            list_container.into_any_element()
        };

        div()
            .id("command-palette-modal")
            .size_full()
            .absolute()
            .top_0()
            .left_0()
            .occlude()
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.close_command_palette(cx);
            }))
            .child(div().size_full().bg(overlay_bg).absolute().top_0().left_0())
            .child(
                div()
                    .size_full()
                    .absolute()
                    .top_0()
                    .left_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(36.0))
                    .child(
                        div()
                            .id("command-palette-panel")
                            .w(px(COMMAND_PALETTE_WIDTH))
                            .px(px(10.0))
                            .py(px(10.0))
                            .rounded_md()
                            .bg(panel_bg)
                            .border_1()
                            .border_color(panel_border)
                            .on_click(cx.listener(|_this, _event, _window, cx| {
                                cx.stop_propagation();
                            }))
                            .child(
                                div()
                                    .w_full()
                                    .pb(px(6.0))
                                    .text_size(px(11.0))
                                    .text_color(muted_text)
                                    .child(mode_title),
                            )
                            .child(
                                div()
                                    .id("command-palette-input")
                                    .w_full()
                                    .h(px(34.0))
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .relative()
                                    .rounded_sm()
                                    .bg(input_bg)
                                    .border_1()
                                    .border_color(panel_border)
                                    .child(div().w_full().h_full().relative().child(
                                        self.render_inline_input_layer(
                                            input_font.clone(),
                                            px(13.0),
                                            primary_text.into(),
                                            input_selection.into(),
                                            InlineInputAlignment::Left,
                                            cx,
                                        ),
                                    )),
                            )
                            .child(div().h(px(8.0)))
                            .child(list)
                            .child(
                                div()
                                    .pt(px(8.0))
                                    .text_size(px(11.0))
                                    .text_color(muted_text)
                                    .child(footer_hint),
                            ),
                    ),
            )
            .into_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_item(title: &str, keywords: &str, action: CommandAction) -> CommandPaletteItem {
        CommandPaletteItem::command(title, keywords, action)
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

        let filtered = TerminalView::filter_command_palette_items_by_query(items, "re");
        let actions: Vec<CommandAction> = filtered
            .into_iter()
            .filter_map(|item| match item.kind {
                CommandPaletteItemKind::Command(action) => Some(action),
                CommandPaletteItemKind::Theme(_) => None,
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

        let filtered = TerminalView::filter_command_palette_items_by_query(items, "font");
        let actions: Vec<CommandAction> = filtered
            .into_iter()
            .filter_map(|item| match item.kind {
                CommandPaletteItemKind::Command(action) => Some(action),
                CommandPaletteItemKind::Theme(_) => None,
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
    fn target_scroll_y_only_moves_when_selection_leaves_viewport() {
        // Viewport fits 8 rows at 30px each => 240px.
        assert_eq!(
            TerminalView::command_palette_target_scroll_y(0.0, 2, 12),
            Some(0.0)
        );
        assert_eq!(
            TerminalView::command_palette_target_scroll_y(0.0, 9, 12),
            Some(60.0)
        );
        assert_eq!(
            TerminalView::command_palette_target_scroll_y(90.0, 0, 12),
            Some(0.0)
        );
        assert_eq!(
            TerminalView::command_palette_target_scroll_y(0.0, 0, 0),
            None
        );
    }

    #[test]
    fn next_scroll_y_is_dt_based_and_respects_bounds() {
        let slow = TerminalView::command_palette_next_scroll_y(0.0, 120.0, 300.0, 1.0 / 240.0);
        let fast = TerminalView::command_palette_next_scroll_y(0.0, 120.0, 300.0, 0.05);
        assert!(fast > slow);
        assert!(fast <= 300.0);

        // Close enough should snap to target.
        let snapped = TerminalView::command_palette_next_scroll_y(59.7, 60.0, 300.0, 1.0 / 60.0);
        assert_eq!(snapped, 60.0);

        // Must clamp to max scroll.
        let clamped = TerminalView::command_palette_next_scroll_y(280.0, 400.0, 300.0, 0.05);
        assert!(clamped <= 300.0);
    }

    #[test]
    fn ordered_theme_ids_pin_current_theme_first() {
        let ordered = TerminalView::ordered_theme_ids_for_palette(
            vec![
                "nord".to_string(),
                "termy".to_string(),
                "dracula".to_string(),
                "nord".to_string(),
            ],
            "termy",
        );

        assert_eq!(ordered, vec!["termy", "dracula", "nord"]);

        let ordered_with_missing_current = TerminalView::ordered_theme_ids_for_palette(
            vec!["nord".to_string(), "dracula".to_string()],
            "tokyo-night",
        );

        assert_eq!(
            ordered_with_missing_current,
            vec!["tokyo-night", "dracula", "nord"]
        );
    }

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
}
