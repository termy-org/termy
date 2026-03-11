use super::super::*;
use super::style::{
    COMMAND_PALETTE_INPUT_RADIUS, COMMAND_PALETTE_PANEL_RADIUS, COMMAND_PALETTE_ROW_RADIUS,
    COMMAND_PALETTE_SHORTCUT_RADIUS, CommandPaletteStyle,
};
use super::*;
use crate::ui::scrollbar::{self, ScrollbarPaintStyle, ScrollbarRange};
use gpui::prelude::FluentBuilder;
use gpui::uniform_list;
use std::ops::Range;

impl TerminalView {
    fn command_palette_scrollbar_metrics(
        &self,
        viewport_height: f32,
        item_count: usize,
    ) -> Option<scrollbar::ScrollbarMetrics> {
        let scroll_handle = self.command_palette.base_scroll_handle();
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
        let selected = self.command_palette.selected_filtered_index().unwrap_or(0);
        let style = CommandPaletteStyle::resolve(self);
        let transparent = self.overlay_style().transparent_background();

        let mut rows = Vec::with_capacity(range.len());
        for index in range {
            let Some(item) = self.command_palette.filtered_item(index).cloned() else {
                continue;
            };

            let is_selected = index == selected;
            let is_enabled = item.enabled;
            let shortcut = match &item.kind {
                CommandPaletteItemKind::Command(action) => {
                    self.command_palette_shortcut(*action, window)
                }
                CommandPaletteItemKind::PluginCommand { .. } => None,
                CommandPaletteItemKind::Theme(_)
                | CommandPaletteItemKind::TmuxSessionAttachOrSwitch { .. }
                | CommandPaletteItemKind::TmuxSessionCreateAndAttach { .. }
                | CommandPaletteItemKind::TmuxSessionDetachCurrent
                | CommandPaletteItemKind::TmuxSessionOpenRenameMode
                | CommandPaletteItemKind::TmuxSessionOpenKillMode
                | CommandPaletteItemKind::TmuxSessionRenameSelect { .. }
                | CommandPaletteItemKind::TmuxSessionRenameApply { .. }
                | CommandPaletteItemKind::TmuxSessionKill { .. }
                | CommandPaletteItemKind::SavedLayoutOpen { .. }
                | CommandPaletteItemKind::SavedLayoutOpenTasksMode { .. }
                | CommandPaletteItemKind::SavedLayoutOpenSaveMode
                | CommandPaletteItemKind::SavedLayoutSaveAs { .. }
                | CommandPaletteItemKind::SavedLayoutOpenRenameMode
                | CommandPaletteItemKind::SavedLayoutRenameSelect { .. }
                | CommandPaletteItemKind::SavedLayoutRenameApply { .. }
                | CommandPaletteItemKind::SavedLayoutOpenDeleteMode
                | CommandPaletteItemKind::SavedLayoutDelete { .. }
                | CommandPaletteItemKind::TaskOpenCreateGlobalMode
                | CommandPaletteItemKind::TaskOpenCreateLayoutMode { .. }
                | CommandPaletteItemKind::TaskOpenSaveCurrentCommandGlobalMode
                | CommandPaletteItemKind::TaskOpenSaveCurrentCommandLayoutMode { .. }
                | CommandPaletteItemKind::TaskCreate { .. }
                | CommandPaletteItemKind::Task { .. } => None,
            };
            let title = item.title.clone();
            let status_hint = item.status_hint;
            let text_color = if is_enabled {
                style.primary_text
            } else {
                style.muted_text
            };
            let shortcut_text = if is_enabled {
                style.shortcut_text
            } else {
                style.muted_text
            };

            rows.push(
                div()
                    .id(("command-palette-item", index))
                    .w_full()
                    .h(px(COMMAND_PALETTE_ROW_HEIGHT))
                    .px(px(10.0))
                    .rounded(px(COMMAND_PALETTE_ROW_RADIUS))
                    .bg(if is_selected {
                        style.selected_bg
                    } else {
                        transparent
                    })
                    .border_1()
                    .border_color(if is_selected {
                        style.selected_border
                    } else {
                        transparent
                    })
                    .when(is_enabled, |row| row.cursor_pointer())
                    .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                        if this.command_palette.set_selected_filtered_index(index) {
                            this.notify_overlay(cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.execute_command_palette_filtered_index(index, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .text_size(px(12.0))
                    .text_color(text_color)
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(div().flex_1().truncate().child(title))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .children(status_hint.map(|label| {
                                        div()
                                            .flex_none()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(COMMAND_PALETTE_SHORTCUT_RADIUS))
                                            .bg(style.shortcut_bg)
                                            .border_1()
                                            .border_color(style.shortcut_border)
                                            .text_size(px(10.0))
                                            .text_color(style.muted_text)
                                            .child(label)
                                    }))
                                    .children(shortcut.map(|label| {
                                        div()
                                            .flex_none()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(COMMAND_PALETTE_SHORTCUT_RADIUS))
                                            .bg(style.shortcut_bg)
                                            .border_1()
                                            .border_color(style.shortcut_border)
                                            .text_size(px(10.0))
                                            .text_color(shortcut_text)
                                            .child(label)
                                    })),
                            ),
                    )
                    .into_any_element(),
            );
        }
        rows
    }

    pub(in super::super) fn render_command_palette_modal(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item_count = self.command_palette.filtered_len();
        let list_height = COMMAND_PALETTE_MAX_ITEMS as f32 * COMMAND_PALETTE_ROW_HEIGHT;
        let mode_title = match self.command_palette.mode() {
            CommandPaletteMode::Commands => "Commands".to_string(),
            CommandPaletteMode::Themes => format!("Theme: {}", self.theme_id),
            CommandPaletteMode::TmuxSessions => match self.command_palette.tmux_session_intent() {
                TmuxSessionIntent::AttachOrSwitch => "tmux Sessions".to_string(),
                TmuxSessionIntent::RenameSelect => "tmux Sessions: Rename".to_string(),
                TmuxSessionIntent::RenameInput => "tmux Sessions: Rename".to_string(),
                TmuxSessionIntent::Kill => "tmux Sessions: Kill".to_string(),
            },
            CommandPaletteMode::Layouts => match self.command_palette.saved_layout_intent() {
                SavedLayoutIntent::Browse => "Saved Layouts".to_string(),
                SavedLayoutIntent::SaveInput => "Saved Layouts: Save".to_string(),
                SavedLayoutIntent::RenameSelect => "Saved Layouts: Rename".to_string(),
                SavedLayoutIntent::RenameInput => "Saved Layouts: Rename".to_string(),
                SavedLayoutIntent::Delete => "Saved Layouts: Delete".to_string(),
            },
            CommandPaletteMode::Tasks => match self.current_named_layout.as_deref() {
                Some(layout_name) if self.command_palette.task_intent() == TaskIntent::Browse => {
                    format!("Tasks: {}", layout_name)
                }
                _ => match self.command_palette.task_intent() {
                    TaskIntent::Browse => "Tasks".to_string(),
                    TaskIntent::CreateGlobalInput => "Tasks: New".to_string(),
                    TaskIntent::CreateLayoutInput => match self.current_named_layout.as_deref() {
                        Some(layout_name) => format!("Tasks: New for {}", layout_name),
                        None => "Tasks: New".to_string(),
                    },
                },
            },
        };
        let footer_hint = match self.command_palette.mode() {
            CommandPaletteMode::Commands => "Enter: Run  Esc: Close  Up/Down: Navigate",
            CommandPaletteMode::Themes => "Enter: Apply Theme  Esc: Back  Up/Down: Navigate",
            CommandPaletteMode::TmuxSessions => match self.command_palette.tmux_session_intent() {
                TmuxSessionIntent::AttachOrSwitch => {
                    "Enter: Open/Create/Manage Session  Esc: Back  Up/Down: Navigate"
                }
                TmuxSessionIntent::RenameSelect => {
                    "Enter: Select Session  Esc: Back  Up/Down: Navigate"
                }
                TmuxSessionIntent::RenameInput => {
                    "Enter: Rename Session  Esc: Back  Up/Down: Navigate"
                }
                TmuxSessionIntent::Kill => "Enter: Kill Session  Esc: Back  Up/Down: Navigate",
            },
            CommandPaletteMode::Layouts => match self.command_palette.saved_layout_intent() {
                SavedLayoutIntent::Browse => {
                    "Enter: Load/Save/Manage Layout  Esc: Back  Up/Down: Navigate"
                }
                SavedLayoutIntent::SaveInput => "Enter: Save Layout  Esc: Back  Up/Down: Navigate",
                SavedLayoutIntent::RenameSelect => {
                    "Enter: Select Layout  Esc: Back  Up/Down: Navigate"
                }
                SavedLayoutIntent::RenameInput => {
                    "Enter: Rename Layout  Esc: Back  Up/Down: Navigate"
                }
                SavedLayoutIntent::Delete => "Enter: Delete Layout  Esc: Back  Up/Down: Navigate",
            },
            CommandPaletteMode::Tasks => match self.command_palette.task_intent() {
                TaskIntent::Browse => "Enter: Run Task  Esc: Back  Up/Down: Navigate",
                TaskIntent::CreateGlobalInput | TaskIntent::CreateLayoutInput => {
                    "Format: name: command  Enter: Save Task  Esc: Back"
                }
            },
        };
        let style = CommandPaletteStyle::resolve(self);
        let input_font = Font {
            family: self.font_family.clone(),
            ..Font::default()
        };
        let empty_state_message = match self.command_palette.mode() {
            CommandPaletteMode::TmuxSessions
                if self.command_palette.tmux_session_intent()
                    == TmuxSessionIntent::AttachOrSwitch
                    && self.command_palette.input().text().trim().is_empty() =>
            {
                "No tmux sessions found. Type a name and press Enter to create one."
            }
            CommandPaletteMode::Layouts
                if self.command_palette.saved_layout_intent() == SavedLayoutIntent::Browse
                    && self.command_palette.input().text().trim().is_empty() =>
            {
                "No saved layouts yet. Save the current split setup from here."
            }
            CommandPaletteMode::Tasks => match self.command_palette.task_intent() {
                TaskIntent::Browse if self.command_palette.input().text().trim().is_empty() => {
                    "No tasks configured. Create one here or add task.<name>.command entries to config.txt."
                }
                TaskIntent::CreateGlobalInput | TaskIntent::CreateLayoutInput => {
                    "Enter a task as name: command"
                }
                _ => "No matching items",
            },
            _ => "No matching items",
        };

        let list = if item_count == 0 {
            div()
                .w_full()
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(8.0))
                        .text_size(px(12.0))
                        .text_color(style.muted_text)
                        .child(empty_state_message),
                )
                .into_any_element()
        } else {
            let list = uniform_list(
                "command-palette-list",
                item_count,
                cx.processor(Self::render_command_palette_rows),
            )
            .flex_1()
            .h(px(list_height))
            .track_scroll(self.command_palette.scroll_handle())
            .into_any_element();
            let mut list_container = div()
                .w_full()
                .h(px(list_height))
                .flex()
                .items_start()
                .child(list);

            if let Some(metrics) = self.command_palette_scrollbar_metrics(list_height, item_count) {
                let paint_style = ScrollbarPaintStyle {
                    width: COMMAND_PALETTE_SCROLLBAR_WIDTH,
                    track_radius: 0.0,
                    thumb_radius: 0.0,
                    thumb_inset: 0.0,
                    marker_inset: 0.0,
                    marker_radius: 0.0,
                    track_color: style.scrollbar_track,
                    thumb_color: style.scrollbar_thumb,
                    active_thumb_color: style.scrollbar_thumb,
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
                            paint_style,
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_command_palette(cx);
                    cx.stop_propagation();
                }),
            )
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
                            .rounded(px(COMMAND_PALETTE_PANEL_RADIUS))
                            .bg(style.panel_bg)
                            .border_1()
                            .border_color(style.panel_border)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_this, _event, _window, cx| {
                                    cx.stop_propagation();
                                }),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .pb(px(6.0))
                                    .text_size(px(11.0))
                                    .text_color(style.muted_text)
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
                                    .rounded(px(COMMAND_PALETTE_INPUT_RADIUS))
                                    .bg(style.input_bg)
                                    .border_1()
                                    .border_color(style.panel_border)
                                    .child(div().w_full().h_full().relative().child(
                                        self.render_inline_input_layer(
                                            input_font.clone(),
                                            px(13.0),
                                            style.primary_text.into(),
                                            style.input_selection.into(),
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
                                    .text_color(style.muted_text)
                                    .child(footer_hint),
                            ),
                    ),
            )
            .into_any()
    }
}
