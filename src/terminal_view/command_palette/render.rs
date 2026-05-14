use super::super::*;
use super::style::{
    COMMAND_PALETTE_PANEL_RADIUS, COMMAND_PALETTE_ROW_RADIUS, COMMAND_PALETTE_SHORTCUT_RADIUS,
    CommandPaletteStyle,
};
use super::*;
use crate::ui::scrollbar::{self, ScrollbarPaintStyle, ScrollbarRange};
use gpui::prelude::FluentBuilder;
use gpui::uniform_list;
use std::ops::Range;

impl TerminalView {
    pub(super) fn command_palette_scrollbar_range(
        &self,
        viewport_height: f32,
        item_count: usize,
    ) -> ScrollbarRange {
        let scroll_handle = self.command_palette.base_scroll_handle();
        let max_offset_from_handle: f32 = scroll_handle.max_offset().height.into();
        let estimated_content_height = item_count as f32 * COMMAND_PALETTE_ROW_HEIGHT;
        let estimated_max_offset = (estimated_content_height - viewport_height).max(0.0);
        let max_offset = max_offset_from_handle.max(estimated_max_offset);
        let offset_y: f32 = scroll_handle.offset().y.into();
        let offset = (-offset_y).max(0.0);
        ScrollbarRange {
            offset,
            max_offset,
            viewport_extent: viewport_height,
            track_extent: viewport_height,
        }
    }

    fn apply_command_palette_scroll_offset(&mut self, offset: f32, max_offset: f32) {
        let clamped = offset.clamp(0.0, max_offset);
        self.command_palette
            .base_scroll_handle()
            .set_offset(point(px(0.0), px(-clamped)));
    }

    pub(super) fn handle_command_palette_scrollbar_mouse_down(
        &mut self,
        window_y: f32,
        viewport_height: f32,
        item_count: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.command_palette_scrollbar_lane_bounds else {
            return;
        };
        let lane_top: f32 = bounds.top().into();
        let local_y = window_y - lane_top;
        let range = self.command_palette_scrollbar_range(viewport_height, item_count);
        let Some(metrics) =
            scrollbar::compute_metrics(range, COMMAND_PALETTE_SCROLLBAR_MIN_THUMB_HEIGHT)
        else {
            return;
        };
        let thumb_top = metrics.thumb_top;
        let thumb_bottom = thumb_top + metrics.thumb_height;
        if local_y >= thumb_top && local_y <= thumb_bottom {
            self.command_palette_scrollbar_drag = Some(TerminalScrollbarDragState {
                thumb_grab_offset: local_y - thumb_top,
            });
        } else {
            let new_offset = scrollbar::offset_from_track_click(local_y, range, metrics);
            self.apply_command_palette_scroll_offset(new_offset, range.max_offset);
            self.command_palette_scrollbar_drag = Some(TerminalScrollbarDragState {
                thumb_grab_offset: metrics.thumb_height * 0.5,
            });
        }
        self.notify_overlay(cx);
    }

    pub(super) fn handle_command_palette_scrollbar_drag(
        &mut self,
        window_y: f32,
        viewport_height: f32,
        item_count: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.command_palette_scrollbar_drag else {
            return;
        };
        let Some(bounds) = self.command_palette_scrollbar_lane_bounds else {
            return;
        };
        let lane_top: f32 = bounds.top().into();
        let local_y = window_y - lane_top;
        let range = self.command_palette_scrollbar_range(viewport_height, item_count);
        let Some(metrics) =
            scrollbar::compute_metrics(range, COMMAND_PALETTE_SCROLLBAR_MIN_THUMB_HEIGHT)
        else {
            return;
        };
        let target_thumb_top = (local_y - drag.thumb_grab_offset).clamp(0.0, metrics.travel);
        let new_offset = scrollbar::offset_from_thumb_top(target_thumb_top, range, metrics);
        self.apply_command_palette_scroll_offset(new_offset, range.max_offset);
        self.notify_overlay(cx);
    }

    pub(super) fn finish_command_palette_scrollbar_drag(&mut self) -> bool {
        self.command_palette_scrollbar_drag.take().is_some()
    }

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
                | CommandPaletteItemKind::Task { .. }
                | CommandPaletteItemKind::AppInfoEntry { .. }
                | CommandPaletteItemKind::AppInfoCopyAll { .. } => None,
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
            let icon_path = palette_item_icon_path(&item);
            let icon_tint = if is_selected {
                style.primary_text
            } else if is_enabled {
                style.muted_text
            } else {
                style.muted_text
            };

            rows.push(
                div()
                    .id(("command-palette-item", index))
                    .w_full()
                    .h(px(COMMAND_PALETTE_ROW_HEIGHT))
                    .px(px(COMMAND_PALETTE_ROW_PADDING_X))
                    .rounded(px(COMMAND_PALETTE_ROW_RADIUS))
                    .bg(if is_selected {
                        style.selected_bg
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
                    .text_size(px(13.0))
                    .text_color(text_color)
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        gpui::svg()
                                            .path(gpui::SharedString::from(icon_path))
                                            .size(px(COMMAND_PALETTE_ROW_ICON_SIZE))
                                            .text_color(icon_tint),
                                    )
                                    .child(div().flex_1().truncate().child(title)),
                            )
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
                    format!("Tasks: {layout_name}")
                }
                _ => match self.command_palette.task_intent() {
                    TaskIntent::Browse => "Tasks".to_string(),
                    TaskIntent::CreateGlobalInput => "Tasks: New".to_string(),
                    TaskIntent::CreateLayoutInput => match self.current_named_layout.as_deref() {
                        Some(layout_name) => format!("Tasks: New for {layout_name}"),
                        None => "Tasks: New".to_string(),
                    },
                },
            },
            CommandPaletteMode::AppInfo => "App Info".to_string(),
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
            CommandPaletteMode::AppInfo => "Enter: Copy  Esc: Back  Up/Down: Navigate",
        };
        let style = CommandPaletteStyle::resolve(self);
        let input_font = Font {
            family: self.ui_font_family.clone(),
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
                let drag_active = self.command_palette_scrollbar_drag.is_some();
                let paint_style = ScrollbarPaintStyle {
                    width: COMMAND_PALETTE_SCROLLBAR_WIDTH,
                    track_radius: 4.0,
                    thumb_radius: 4.0,
                    thumb_inset: 1.0,
                    marker_inset: 0.0,
                    marker_radius: 0.0,
                    track_color: style.scrollbar_track,
                    thumb_color: style.scrollbar_thumb,
                    active_thumb_color: style.scrollbar_thumb,
                    marker_color: None,
                    current_marker_color: None,
                };
                let bounds_entity = cx.entity().clone();
                list_container = list_container.child(
                    div()
                        .id("command-palette-scrollbar-lane")
                        .w(px(COMMAND_PALETTE_SCROLLBAR_WIDTH + 4.0))
                        .h(px(list_height))
                        .pl(px(2.0))
                        .pr(px(2.0))
                        .cursor_pointer()
                        .child(
                            gpui::canvas(
                                move |bounds, _, cx| {
                                    bounds_entity.update(cx, |view, _| {
                                        view.command_palette_scrollbar_lane_bounds = Some(bounds);
                                    });
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .child(scrollbar::render_vertical(
                            "command-palette-scrollbar",
                            metrics,
                            paint_style,
                            drag_active,
                            &[],
                            None,
                            0.0,
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                                let y: f32 = event.position.y.into();
                                this.handle_command_palette_scrollbar_mouse_down(
                                    y,
                                    list_height,
                                    item_count,
                                    cx,
                                );
                            }),
                        ),
                );
            }

            list_container.into_any_element()
        };

        let scrim_color = gpui::Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: COMMAND_PALETTE_SCRIM_ALPHA,
        };
        let mut divider = style.muted_text;
        divider.a = COMMAND_PALETTE_DIVIDER_ALPHA;

        let mode_chip: Option<AnyElement> = if matches!(
            self.command_palette.mode(),
            CommandPaletteMode::Commands
        ) {
            None
        } else {
            Some(
                div()
                    .flex_none()
                    .h(px(22.0))
                    .px(px(8.0))
                    .rounded(px(COMMAND_PALETTE_SHORTCUT_RADIUS))
                    .bg(style.shortcut_bg)
                    .border_1()
                    .border_color(style.shortcut_border)
                    .text_size(px(10.0))
                    .text_color(style.muted_text)
                    .flex()
                    .items_center()
                    .child(mode_title.clone())
                    .into_any_element(),
            )
        };

        let input_head = div()
            .id("command-palette-input")
            .w_full()
            .h(px(COMMAND_PALETTE_INPUT_HEAD_HEIGHT))
            .px(px(COMMAND_PALETTE_ROW_PADDING_X + 4.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                gpui::svg()
                    .path(gpui::SharedString::from("icons/settings/search.svg"))
                    .size(px(18.0))
                    .text_color(style.muted_text),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(22.0))
                    .relative()
                    .child(self.render_inline_input_layer(
                        input_font,
                        px(COMMAND_PALETTE_INPUT_TEXT_SIZE),
                        style.primary_text.into(),
                        style.input_selection.into(),
                        InlineInputAlignment::Left,
                        cx,
                    )),
            )
            .children(mode_chip);

        let panel = div()
            .id("command-palette-panel")
            .w(px(COMMAND_PALETTE_WIDTH))
            .rounded(px(COMMAND_PALETTE_PANEL_RADIUS))
            .bg(style.panel_bg)
            .border_1()
            .border_color(style.panel_border)
            .overflow_hidden()
            .flex()
            .flex_col()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event, _window, cx| {
                    cx.stop_propagation();
                }),
            )
            .child(input_head)
            .child(div().h(px(1.0)).w_full().bg(divider))
            .child(div().w_full().px(px(6.0)).py(px(6.0)).child(list));

        let footer = div()
            .pt(px(10.0))
            .text_size(px(11.0))
            .text_color(style.muted_text)
            .child(footer_hint);

        let scrollbar_drag_active = self.command_palette_scrollbar_drag.is_some();
        div()
            .id("command-palette-modal")
            .size_full()
            .absolute()
            .top_0()
            .left_0()
            .occlude()
            .bg(scrim_color)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_command_palette(cx);
                    cx.stop_propagation();
                }),
            )
            .when(scrollbar_drag_active, |s| {
                s.on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    if !event.dragging() {
                        return;
                    }
                    let y: f32 = event.position.y.into();
                    this.handle_command_palette_scrollbar_drag(y, list_height, item_count, cx);
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        if this.finish_command_palette_scrollbar_drag() {
                            this.notify_overlay(cx);
                        }
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        if this.finish_command_palette_scrollbar_drag() {
                            this.notify_overlay(cx);
                        }
                    }),
                )
            })
            .child(
                div()
                    .size_full()
                    .absolute()
                    .top_0()
                    .left_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(COMMAND_PALETTE_TOP_OFFSET))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(panel)
                            .child(footer),
                    ),
            )
            .into_any()
    }
}
