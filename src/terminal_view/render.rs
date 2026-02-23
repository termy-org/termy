use super::scrollbar as terminal_scrollbar;
use super::*;
use crate::ui::scrollbar::{self as ui_scrollbar, ScrollbarPaintStyle};

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalView {
    fn refresh_terminal_scrollbar_marker_cache(
        &mut self,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
        marker_height: f32,
    ) -> Option<f32> {
        if !self.search_open {
            self.clear_terminal_scrollbar_marker_cache();
            return None;
        }

        let marker_height = marker_height.max(0.0);
        let marker_top_limit =
            terminal_scrollbar::marker_top_limit(layout.metrics.track_height, marker_height);
        let cache_key = TerminalScrollbarMarkerCacheKey {
            results_revision: self.search_state.results_revision(),
            history_size: layout.history_size,
            viewport_rows: layout.viewport_rows,
            marker_top_limit_bucket: terminal_scrollbar::marker_top_limit_bucket(marker_top_limit),
        };
        let rebuild_markers = self.terminal_scrollbar_marker_cache.key.as_ref() != Some(&cache_key);

        let (is_empty, current_line, new_marker_tops) = {
            let results = self.search_state.results();
            if results.is_empty() {
                (true, None, None)
            } else {
                let current_line = results.current().map(|current| current.line);
                let new_marker_tops = rebuild_markers.then(|| {
                    terminal_scrollbar::deduped_marker_tops(
                        results
                            .matches()
                            .iter()
                            .map(|search_match| search_match.line),
                        layout.history_size,
                        layout.viewport_rows,
                        marker_height,
                        marker_top_limit,
                    )
                });
                (false, current_line, new_marker_tops)
            }
        };

        if is_empty {
            self.clear_terminal_scrollbar_marker_cache();
            return None;
        }

        if let Some(marker_tops) = new_marker_tops {
            self.terminal_scrollbar_marker_cache.marker_tops = marker_tops;
            self.terminal_scrollbar_marker_cache.key = Some(cache_key);
        }

        current_line.map(|line| {
            terminal_scrollbar::marker_top_for_line(
                line,
                layout.history_size,
                layout.viewport_rows,
                marker_top_limit,
            )
        })
    }

    fn render_terminal_scrollbar_overlay(
        &mut self,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
        force_visible: bool,
    ) -> Option<AnyElement> {
        let now = Instant::now();
        let force_visible = force_visible
            && self.terminal_scrollbar_mode() != ui_scrollbar::ScrollbarVisibilityMode::AlwaysOff;
        let alpha = if force_visible {
            1.0
        } else {
            self.terminal_scrollbar_alpha(now)
        };
        if alpha <= f32::EPSILON && !self.terminal_scrollbar_visibility_controller.is_dragging() {
            return None;
        }
        let overlay_style = self.overlay_style();
        let gutter_bg = overlay_style.panel_background(TERMINAL_SCROLLBAR_GUTTER_ALPHA);
        let style = ScrollbarPaintStyle {
            width: TERMINAL_SCROLLBAR_TRACK_WIDTH,
            track_radius: TERMINAL_SCROLLBAR_TRACK_RADIUS,
            thumb_radius: TERMINAL_SCROLLBAR_THUMB_RADIUS,
            thumb_inset: TERMINAL_SCROLLBAR_THUMB_INSET,
            marker_inset: TERMINAL_SCROLLBAR_THUMB_INSET,
            marker_radius: TERMINAL_SCROLLBAR_THUMB_RADIUS,
            track_color: self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_TRACK_ALPHA),
            thumb_color: self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_THUMB_ALPHA),
            active_thumb_color: self
                .scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_THUMB_ACTIVE_ALPHA),
            marker_color: Some(
                self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_MATCH_MARKER_ALPHA),
            ),
            current_marker_color: Some(
                overlay_style.panel_cursor(TERMINAL_SCROLLBAR_CURRENT_MARKER_ALPHA),
            ),
        }
        .scale_alpha(alpha);

        let current_marker_top =
            self.refresh_terminal_scrollbar_marker_cache(layout, TERMINAL_SCROLLBAR_MARKER_HEIGHT);
        let marker_tops = &self.terminal_scrollbar_marker_cache.marker_tops;

        Some(
            div()
                .id("terminal-scrollbar-overlay")
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(TERMINAL_SCROLLBAR_GUTTER_WIDTH))
                .bg(gutter_bg)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(px(TERMINAL_SCROLLBAR_TRACK_WIDTH))
                        .child(ui_scrollbar::render_vertical(
                            "terminal-scrollbar",
                            layout.metrics,
                            style,
                            self.terminal_scrollbar_visibility_controller.is_dragging(),
                            marker_tops,
                            current_marker_top,
                            TERMINAL_SCROLLBAR_MARKER_HEIGHT,
                        )),
                )
                .into_any_element(),
        )
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Process pending OSC 52 clipboard writes
        if let Some(text) = self.pending_clipboard.take() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }

        self.toast_manager.ingest_pending();
        self.toast_manager.tick_with_hovered(self.hovered_toast);
        if let Some((_, copied_at)) = self.copied_toast_feedback
            && copied_at.elapsed() >= Duration::from_millis(TOAST_COPY_FEEDBACK_MS)
        {
            self.copied_toast_feedback = None;
        }

        // Request re-render during toast animations for smooth fade in/out
        // Only schedule one timer at a time to avoid spawning 60 tasks/sec
        if self.toast_manager.is_animating() && !self.toast_animation_scheduled {
            self.toast_animation_scheduled = true;
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                smol::Timer::after(Duration::from_millis(16)).await;
                let _ = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        view.toast_animation_scheduled = false;
                        cx.notify();
                    })
                });
            })
            .detach();
        }

        // Compute update banner state
        #[cfg(target_os = "macos")]
        let banner_state = self.auto_updater.as_ref().map(|e| e.read(cx).state.clone());
        #[cfg(target_os = "macos")]
        {
            self.sync_update_toasts(banner_state.as_ref());
            self.show_update_banner = matches!(
                &banner_state,
                Some(
                    UpdateState::Available { .. }
                        | UpdateState::Downloading { .. }
                        | UpdateState::Downloaded { .. }
                        | UpdateState::Installing { .. }
                        | UpdateState::Installed { .. }
                        | UpdateState::Error(_)
                )
            );
        }

        let cell_size = self.calculate_cell_size(window, cx);
        let colors = self.colors.clone();
        let font_family = self.font_family.clone();
        let font_size = self.font_size;
        self.sync_window_background_appearance(window);
        let effective_background_opacity = self.background_opacity_factor();
        let (effective_padding_x, effective_padding_y) = self.effective_terminal_padding();

        self.sync_terminal_size(window, cell_size);

        // Collect cells to render - pre-allocate based on terminal size to avoid reallocations
        let terminal_size = self.active_terminal().size();
        let estimated_cells = (terminal_size.cols as usize) * (terminal_size.rows as usize);
        let mut cells_to_render: Vec<CellRenderInfo> = Vec::with_capacity(estimated_cells);
        let (cursor_col, cursor_row) = self.active_terminal().cursor_position();
        let terminal_cursor_active =
            !self.command_palette_open && self.renaming_tab.is_none() && !self.search_open;
        let cursor_visible = terminal_cursor_active
            && self.cursor_visible_for_focus(self.focus_handle.is_focused(window));

        // Pre-compute search match info
        let search_active = self.search_open;
        let search_results = if search_active {
            Some(self.search_state.results())
        } else {
            None
        };
        let mut terminal_display_offset = 0usize;

        self.active_terminal().with_term(|term| {
            let content = term.renderable_content();
            terminal_display_offset = content.display_offset;
            let show_cursor = content.display_offset == 0 && cursor_visible;
            for cell in content.display_iter {
                let point = cell.point;
                let cell_content = &cell.cell;
                let term_line = point.line.0;
                let Some(row) =
                    Self::viewport_row_from_term_line(term_line, content.display_offset)
                else {
                    continue;
                };
                let col = point.column.0;

                // Get foreground and background colors
                let mut fg = colors.convert(cell_content.fg);
                let mut bg = colors.convert(cell_content.bg);
                if cell_content.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if cell_content.flags.contains(Flags::DIM) {
                    fg.r *= DIM_TEXT_FACTOR;
                    fg.g *= DIM_TEXT_FACTOR;
                    fg.b *= DIM_TEXT_FACTOR;
                }
                bg.a *= effective_background_opacity;

                let c = cell_content.c;
                let is_cursor = show_cursor && col == cursor_col && row == cursor_row;
                let selected = self.cell_is_selected(col, row);

                // Check search matches
                let (search_current, search_match) = if let Some(results) = &search_results {
                    let is_current = results.is_current_match(term_line, col);
                    let is_any = results.is_any_match(term_line, col);
                    (is_current, is_any && !is_current)
                } else {
                    (false, false)
                };

                cells_to_render.push(CellRenderInfo {
                    col,
                    row,
                    char: c,
                    fg: fg.into(),
                    bg: bg.into(),
                    bold: cell_content.flags.contains(Flags::BOLD),
                    render_text: !cell_content.flags.intersects(
                        Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
                    ),
                    is_cursor,
                    selected,
                    search_current,
                    search_match,
                });
            }
        });

        let focus_handle = self.focus_handle.clone();
        let show_tab_bar = self.show_tab_bar();
        let show_windows_controls = cfg!(target_os = "windows");
        let show_titlebar_plus = self.use_tabs && !show_windows_controls;
        let titlebar_side_slot_width = if show_windows_controls {
            WINDOWS_TITLEBAR_CONTROLS_WIDTH
        } else if show_titlebar_plus {
            (TITLEBAR_PLUS_SIZE * 3.0) + 8.0
        } else {
            (TITLEBAR_PLUS_SIZE * 2.0) + 4.0
        };
        let viewport = window.viewport_size();
        let tab_layout = self.tab_bar_layout(viewport.width.into());
        let titlebar_height = self.titlebar_height();
        let mut titlebar_bg = colors.background;
        titlebar_bg.a = self.scaled_chrome_alpha(0.96);
        let mut titlebar_border = colors.cursor;
        titlebar_border.a = 0.18;
        let mut titlebar_text = colors.foreground;
        titlebar_text.a = 0.82;
        let mut titlebar_plus_bg = colors.cursor;
        titlebar_plus_bg.a = if show_titlebar_plus {
            self.scaled_chrome_alpha(0.2)
        } else {
            0.0
        };
        let mut titlebar_plus_text = colors.foreground;
        titlebar_plus_text.a = if show_titlebar_plus { 0.92 } else { 0.0 };
        let mut tabbar_bg = colors.background;
        tabbar_bg.a = if show_tab_bar {
            self.scaled_chrome_alpha(0.92)
        } else {
            0.0
        };
        let mut tabbar_border = colors.cursor;
        tabbar_border.a = if show_tab_bar { 0.14 } else { 0.0 };
        let mut active_tab_bg = colors.cursor;
        active_tab_bg.a = self.scaled_chrome_alpha(0.2);
        let mut active_tab_border = colors.cursor;
        active_tab_border.a = 0.32;
        let mut active_tab_text = colors.foreground;
        active_tab_text.a = 0.95;
        let mut inactive_tab_bg = colors.background;
        inactive_tab_bg.a = self.scaled_chrome_alpha(0.56);
        let mut inactive_tab_border = colors.cursor;
        inactive_tab_border.a = 0.12;
        let mut inactive_tab_text = colors.foreground;
        inactive_tab_text.a = 0.68;
        let mut selection_bg = colors.cursor;
        selection_bg.a = SELECTION_BG_ALPHA;
        let selection_fg = colors.background;
        let hovered_link_range = self
            .hovered_link
            .as_ref()
            .map(|link| (link.row, link.start_col, link.end_col));

        let mut tabs_row = div()
            .w_full()
            .h(px(if show_tab_bar { TABBAR_HEIGHT } else { 0.0 }))
            .flex()
            .items_center()
            .px(px(TAB_HORIZONTAL_PADDING));

        if show_tab_bar {
            for (index, tab) in self.tabs.iter().enumerate() {
                let switch_tab_index = index;
                let close_tab_index = index;
                let is_active = index == self.active_tab;
                let show_tab_close = Self::tab_shows_close(
                    tab_layout.tab_pill_width,
                    is_active,
                    tab_layout.tab_padding_x,
                );
                let close_slot_width = if show_tab_close {
                    TAB_CLOSE_HITBOX
                } else {
                    0.0
                };
                let is_renaming = self.renaming_tab == Some(index);
                let label = tab.title.clone();
                let rename_text_color = if is_active {
                    active_tab_text
                } else {
                    inactive_tab_text
                };
                let mut rename_selection_color = colors.cursor;
                rename_selection_color.a = if is_active { 0.34 } else { 0.26 };

                tabs_row = tabs_row.child(
                    div()
                        .bg(if is_active {
                            active_tab_bg
                        } else {
                            inactive_tab_bg
                        })
                        .border_1()
                        .border_color(if is_active {
                            active_tab_border
                        } else {
                            inactive_tab_border
                        })
                        .w(px(tab_layout.tab_pill_width))
                        .h(px(TAB_PILL_HEIGHT))
                        .px(px(tab_layout.tab_padding_x))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.switch_tab(switch_tab_index, cx);
                            }),
                        )
                        .child(div().w(px(close_slot_width)).h(px(TAB_CLOSE_HITBOX)))
                        .child(div().flex_1().h_full().relative().child(if is_renaming {
                            self.render_inline_input_layer(
                                Font::default(),
                                px(12.0),
                                rename_text_color.into(),
                                rename_selection_color.into(),
                                InlineInputAlignment::Center,
                                cx,
                            )
                        } else {
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .truncate()
                                .text_color(rename_text_color)
                                .text_size(px(12.0))
                                .child(label)
                                .into_any_element()
                        }))
                        .children(show_tab_close.then(|| {
                            div()
                                .w(px(close_slot_width))
                                .h(px(TAB_CLOSE_HITBOX))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(if is_active {
                                    active_tab_text
                                } else {
                                    inactive_tab_text
                                })
                                .text_size(px(13.0))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.close_tab(close_tab_index, cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child("×")
                        })),
                );

                if index + 1 < self.tabs.len() {
                    tabs_row = tabs_row.child(div().w(px(TAB_PILL_GAP)).h(px(1.0)));
                }
            }
        }

        // Build update banner element (macOS only)
        #[cfg(target_os = "macos")]
        let banner_element: Option<AnyElement> = if self.show_update_banner {
            let mut banner_bg = colors.cursor;
            banner_bg.a = 0.15;
            let banner_text_color = colors.foreground;

            match &banner_state {
                Some(UpdateState::Available { version, .. }) => {
                    let version = version.clone();
                    let updater_weak = self.auto_updater.as_ref().map(|e| e.downgrade());
                    let updater_weak2 = updater_weak.clone();
                    Some(
                        div()
                            .id("update-banner")
                            .w_full()
                            .h(px(UPDATE_BANNER_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(12.0))
                            .bg(banner_bg)
                            .text_color(banner_text_color)
                            .text_size(px(12.0))
                            .child(format!("Update v{} available", version))
                            .child(
                                div()
                                    .id("update-install-btn")
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .rounded_sm()
                                    .bg(colors.cursor)
                                    .text_color(colors.background)
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_this, _event, _window, cx| {
                                            if let Some(ref weak) = updater_weak {
                                                if let Some(entity) = weak.upgrade() {
                                                    AutoUpdater::install(entity.downgrade(), cx);
                                                    termy_toast::info("Downloading update...");
                                                }
                                            }
                                        }),
                                    )
                                    .child("Install"),
                            )
                            .child(
                                div()
                                    .id("update-dismiss-btn")
                                    .px(px(6.0))
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .text_size(px(13.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_this, _event, _window, cx| {
                                            if let Some(ref weak) = updater_weak2 {
                                                if let Some(entity) = weak.upgrade() {
                                                    entity.update(cx, |u, cx| u.dismiss(cx));
                                                }
                                            }
                                        }),
                                    )
                                    .child("\u{00d7}"),
                            )
                            .into_any(),
                    )
                }
                Some(UpdateState::Downloading {
                    version,
                    downloaded,
                    total,
                }) => {
                    let progress_text = if *total > 0 {
                        format!(
                            "Downloading v{}... {}%",
                            version,
                            (*downloaded as f64 / *total as f64 * 100.0) as u32
                        )
                    } else {
                        format!("Downloading v{}... {} KB", version, *downloaded / 1024)
                    };
                    Some(
                        div()
                            .id("update-banner")
                            .w_full()
                            .h(px(UPDATE_BANNER_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(banner_bg)
                            .text_color(banner_text_color)
                            .text_size(px(12.0))
                            .child(progress_text)
                            .into_any(),
                    )
                }
                Some(UpdateState::Downloaded { version, .. }) => {
                    let version = version.clone();
                    let updater_weak = self.auto_updater.as_ref().map(|e| e.downgrade());
                    Some(
                        div()
                            .id("update-banner")
                            .w_full()
                            .h(px(UPDATE_BANNER_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(12.0))
                            .bg(banner_bg)
                            .text_color(banner_text_color)
                            .text_size(px(12.0))
                            .child(format!("v{} downloaded", version))
                            .child(
                                div()
                                    .id("update-install-btn")
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .rounded_sm()
                                    .bg(colors.cursor)
                                    .text_color(colors.background)
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_this, _event, _window, cx| {
                                            if let Some(ref weak) = updater_weak {
                                                if let Some(entity) = weak.upgrade() {
                                                    AutoUpdater::complete_install(
                                                        entity.downgrade(),
                                                        cx,
                                                    );
                                                    termy_toast::info("Starting installation...");
                                                }
                                            }
                                        }),
                                    )
                                    .child("Install Now"),
                            )
                            .into_any(),
                    )
                }
                Some(UpdateState::Installing { version }) => Some(
                    div()
                        .id("update-banner")
                        .w_full()
                        .h(px(UPDATE_BANNER_HEIGHT))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(banner_bg)
                        .text_color(banner_text_color)
                        .text_size(px(12.0))
                        .child(format!("Installing v{}...", version))
                        .into_any(),
                ),
                Some(UpdateState::Installed { version }) => {
                    let updater_weak = self.auto_updater.as_ref().map(|e| e.downgrade());
                    Some(
                        div()
                            .id("update-banner")
                            .w_full()
                            .h(px(UPDATE_BANNER_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(12.0))
                            .bg(banner_bg)
                            .text_color(banner_text_color)
                            .text_size(px(12.0))
                            .child(format!("v{} installed — restart to complete", version))
                            .child(
                                div()
                                    .id("update-restart-btn")
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .rounded_sm()
                                    .bg(colors.cursor)
                                    .text_color(colors.background)
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            match this.restart_application() {
                                                Ok(()) => cx.quit(),
                                                Err(error) => {
                                                    termy_toast::error(format!(
                                                        "Restart failed: {}",
                                                        error
                                                    ));
                                                }
                                            }
                                        }),
                                    )
                                    .child("Restart"),
                            )
                            .child(
                                div()
                                    .id("update-dismiss-btn")
                                    .px(px(6.0))
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .text_size(px(13.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_this, _event, _window, cx| {
                                            if let Some(ref weak) = updater_weak {
                                                if let Some(entity) = weak.upgrade() {
                                                    entity.update(cx, |u, cx| u.dismiss(cx));
                                                }
                                            }
                                        }),
                                    )
                                    .child("\u{00d7}"),
                            )
                            .into_any(),
                    )
                }
                Some(UpdateState::Error(msg)) => {
                    let updater_weak = self.auto_updater.as_ref().map(|e| e.downgrade());
                    Some(
                        div()
                            .id("update-banner")
                            .w_full()
                            .h(px(UPDATE_BANNER_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(12.0))
                            .bg(banner_bg)
                            .text_color(banner_text_color)
                            .text_size(px(12.0))
                            .child(format!("Update error: {}", msg))
                            .child(
                                div()
                                    .id("update-dismiss-btn")
                                    .px(px(6.0))
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .text_size(px(13.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_this, _event, _window, cx| {
                                            if let Some(ref weak) = updater_weak {
                                                if let Some(entity) = weak.upgrade() {
                                                    entity.update(cx, |u, cx| u.dismiss(cx));
                                                }
                                            }
                                        }),
                                    )
                                    .child("\u{00d7}"),
                            )
                            .into_any(),
                    )
                }
                _ => None,
            }
        } else {
            None
        };
        #[cfg(not(target_os = "macos"))]
        let banner_element: Option<AnyElement> = None;
        let mut terminal_surface_bg = colors.background;
        terminal_surface_bg.a = self.scaled_background_alpha(terminal_surface_bg.a);
        let terminal_surface_bg_hsla: gpui::Hsla = terminal_surface_bg.into();

        // Search highlight colors tuned for strong contrast on dark terminal themes.
        let search_match_bg = gpui::Hsla {
            h: 0.14,
            s: 0.92,
            l: 0.62,
            a: 0.62,
        };
        let search_current_bg = gpui::Hsla {
            h: 0.09,
            s: 0.98,
            l: 0.56,
            a: 0.86,
        };

        let terminal_grid = TerminalGrid {
            cells: cells_to_render,
            cell_size,
            cols: terminal_size.cols as usize,
            rows: terminal_size.rows as usize,
            clear_bg: gpui::Hsla::transparent_black(),
            default_bg: terminal_surface_bg_hsla,
            cursor_color: colors.cursor.into(),
            selection_bg: selection_bg.into(),
            selection_fg: selection_fg.into(),
            search_match_bg,
            search_current_bg,
            hovered_link_range,
            font_family: font_family.clone(),
            font_size,
            cursor_style: self.terminal_cursor_style(),
        };
        if self.terminal_scrollbar_mode() == ui_scrollbar::ScrollbarVisibilityMode::OnScroll
            && !self.terminal_scrollbar_animation_active
            && self.terminal_scrollbar_needs_animation(Instant::now())
        {
            self.start_terminal_scrollbar_animation(cx);
        }
        let terminal_track_height = self
            .terminal_surface_geometry(window)
            .map(|geometry| geometry.height)
            .unwrap_or(0.0);
        let terminal_scrollbar_layout =
            self.terminal_scrollbar_layout_for_track(terminal_track_height);
        if terminal_scrollbar_layout.is_none() {
            self.clear_terminal_scrollbar_marker_cache();
        }
        let terminal_scrollbar_overlay = terminal_scrollbar_layout.and_then(|layout| {
            self.render_terminal_scrollbar_overlay(layout, terminal_display_offset > 0)
        });
        let terminal_grid_layer = if let Some(viewport) = self.terminal_viewport_geometry() {
            div()
                .relative()
                .w(px(viewport.width))
                .h(px(viewport.height))
                .child(terminal_grid)
                .into_any_element()
        } else {
            div().child(terminal_grid).into_any_element()
        };
        let command_palette_overlay = if self.command_palette_open {
            Some(self.render_command_palette_modal(cx))
        } else {
            None
        };
        let search_overlay = if self.search_open {
            Some(self.render_search_bar(cx))
        } else {
            None
        };
        let key_context = if self.has_active_inline_input() {
            "Terminal InlineInput"
        } else {
            "Terminal"
        };
        let titlebar_element: Option<AnyElement> = (titlebar_height > 0.0).then(|| {
            div()
                .id("titlebar")
                .w_full()
                .h(px(titlebar_height))
                .flex_none()
                .flex()
                .items_center()
                .window_control_area(WindowControlArea::Drag)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::handle_titlebar_mouse_down),
                )
                .bg(titlebar_bg)
                .border_b(px(1.0))
                .border_color(titlebar_border)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .px(px(TITLEBAR_SIDE_PADDING))
                        .child(
                            div()
                                .w(px(titlebar_side_slot_width))
                                .h(px(TITLEBAR_PLUS_SIZE)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .justify_center()
                                .text_color(titlebar_text)
                                .text_size(px(12.0))
                                .child("Termy"),
                        )
                        .child(if show_windows_controls {
                            div()
                                .w(px(WINDOWS_TITLEBAR_CONTROLS_WIDTH))
                                .h(px(TITLEBAR_HEIGHT))
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .w(px(WINDOWS_TITLEBAR_BUTTON_WIDTH))
                                        .h(px(TITLEBAR_HEIGHT))
                                        .window_control_area(WindowControlArea::Min)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(titlebar_text)
                                        .text_size(px(12.0))
                                        .child("-"),
                                )
                                .child(
                                    div()
                                        .w(px(WINDOWS_TITLEBAR_BUTTON_WIDTH))
                                        .h(px(TITLEBAR_HEIGHT))
                                        .window_control_area(WindowControlArea::Max)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(titlebar_text)
                                        .text_size(px(12.0))
                                        .child("+"),
                                )
                                .child(
                                    div()
                                        .w(px(WINDOWS_TITLEBAR_BUTTON_WIDTH))
                                        .h(px(TITLEBAR_HEIGHT))
                                        .window_control_area(WindowControlArea::Close)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(titlebar_text)
                                        .text_size(px(12.0))
                                        .child("x"),
                                )
                        } else {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .id("titlebar-settings")
                                        .w(px(TITLEBAR_PLUS_SIZE))
                                        .h(px(TITLEBAR_PLUS_SIZE))
                                        .rounded_sm()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(titlebar_plus_bg)
                                        .text_color(titlebar_plus_text)
                                        .text_size(px(14.0))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.execute_command_action(
                                                    CommandAction::OpenSettings,
                                                    false,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child("\u{2699}"),
                                )
                                .child(
                                    div()
                                        .id("titlebar-update")
                                        .w(px(TITLEBAR_PLUS_SIZE))
                                        .h(px(TITLEBAR_PLUS_SIZE))
                                        .rounded_sm()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(titlebar_plus_bg)
                                        .text_color(titlebar_plus_text)
                                        .text_size(px(13.0))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.execute_command_action(
                                                    CommandAction::CheckForUpdates,
                                                    false,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child("\u{21BB}"),
                                )
                                .children(show_titlebar_plus.then(|| {
                                    div()
                                        .id("titlebar-new-tab")
                                        .w(px(TITLEBAR_PLUS_SIZE))
                                        .h(px(TITLEBAR_PLUS_SIZE))
                                        .rounded_sm()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(titlebar_plus_bg)
                                        .text_color(titlebar_plus_text)
                                        .text_size(px(16.0))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.add_tab(cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child("+")
                                }))
                        }),
                )
                .into_any()
        });
        let toast_overlay = if self.toast_manager.active().is_empty() {
            None
        } else {
            let mut container = div().flex().flex_col().gap(px(6.0));
            for toast in self.toast_manager.active().iter() {
                let toast_id = toast.id;
                let toast_message = toast.message.clone();
                let is_hovered = self.hovered_toast == Some(toast_id);
                let is_copied = self
                    .copied_toast_feedback
                    .is_some_and(|(id, _)| id == toast_id);

                // Animation values
                let opacity = toast.opacity();
                let slide_offset = toast.slide_offset();

                // Clean, minimal icons and subtle accent colors
                let (icon, accent, _is_loading) = match toast.kind {
                    termy_toast::ToastKind::Info => (
                        "\u{2139}", // ℹ info symbol
                        gpui::Rgba {
                            r: 0.53,
                            g: 0.70,
                            b: 0.92,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Success => (
                        "\u{2713}", // ✓ checkmark
                        gpui::Rgba {
                            r: 0.42,
                            g: 0.78,
                            b: 0.55,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Warning => (
                        "\u{26A0}", // ⚠ warning
                        gpui::Rgba {
                            r: 0.94,
                            g: 0.76,
                            b: 0.38,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Error => (
                        "\u{2715}", // ✕ x mark
                        gpui::Rgba {
                            r: 0.92,
                            g: 0.45,
                            b: 0.45,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Loading => {
                        // Animated spinner using braille characters
                        const SPINNER_FRAMES: &[&str] =
                            &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                        let elapsed_ms = toast.created_at.elapsed().as_millis() as usize;
                        let frame_index = (elapsed_ms / 80) % SPINNER_FRAMES.len();
                        (
                            SPINNER_FRAMES[frame_index],
                            gpui::Rgba {
                                r: 0.53,
                                g: 0.70,
                                b: 0.92,
                                a: opacity,
                            },
                            true,
                        )
                    }
                };

                // Subtle, glassy background with animation
                let mut bg = colors.background;
                bg.a = 0.88 * opacity;
                let mut border = colors.foreground;
                border.a = 0.08 * opacity;
                let mut text = colors.foreground;
                text.a = 0.92 * opacity;

                container = container.child(
                    div()
                        .id(("toast", toast_id))
                        .w(px(320.0))
                        .mt(px(slide_offset))
                        .rounded_lg()
                        .bg(bg)
                        .border_1()
                        .border_color(border)
                        .shadow_md()
                        .overflow_hidden()
                        .child(
                            div()
                                .w_full()
                                .px(px(14.0))
                                .py(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                // Icon
                                .child(div().text_size(px(14.0)).text_color(accent).child(icon))
                                // Message
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(13.0))
                                        .text_color(text)
                                        .child(toast_message.clone()),
                                )
                                .child(
                                    div()
                                        .w(px(68.0))
                                        .h(px(24.0))
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .children(is_copied.then(|| {
                                            let mut copied_bg = accent;
                                            copied_bg.a = 0.22;
                                            div()
                                                .rounded(px(6.0))
                                                .px(px(8.0))
                                                .py(px(4.0))
                                                .text_size(px(11.0))
                                                .text_color(accent)
                                                .bg(copied_bg)
                                                .child("Copied")
                                        }))
                                        .children((!is_copied && is_hovered).then(|| {
                                            let toast_message_for_copy = toast_message.clone();
                                            div()
                                                .rounded(px(6.0))
                                                .px(px(8.0))
                                                .py(px(4.0))
                                                .text_size(px(11.0))
                                                .text_color(text)
                                                .bg(border)
                                                .hover(|style| style.bg(accent))
                                                .cursor_pointer()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        move |this, _event, _window, cx| {
                                                            cx.write_to_clipboard(
                                                                ClipboardItem::new_string(
                                                                    toast_message_for_copy.clone(),
                                                                ),
                                                            );
                                                            this.copied_toast_feedback =
                                                                Some((toast_id, Instant::now()));
                                                            cx.notify();
                                                            cx.spawn(
                                                                async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                                                                    smol::Timer::after(Duration::from_millis(
                                                                        TOAST_COPY_FEEDBACK_MS,
                                                                    ))
                                                                    .await;
                                                                    let _ = cx.update(|cx| {
                                                                        this.update(cx, |view, cx| {
                                                                            if view
                                                                                .copied_toast_feedback
                                                                                .is_some_and(
                                                                                    |(id, _)| {
                                                                                        id == toast_id
                                                                                    },
                                                                                )
                                                                            {
                                                                                view.copied_toast_feedback = None;
                                                                                cx.notify();
                                                                            }
                                                                        })
                                                                    });
                                                                },
                                                            )
                                                            .detach();
                                                            cx.stop_propagation();
                                                        },
                                                    ),
                                                )
                                                .child("Copy")
                                        })),
                                )
                                .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                                    if this.hovered_toast != Some(toast_id) {
                                        this.hovered_toast = Some(toast_id);
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                })),
                        ),
                );
            }

            Some(
                div()
                    .id("toast-overlay")
                    .size_full()
                    .absolute()
                    .top_0()
                    .left_0()
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_end()
                            .justify_end()
                            .pr(px(20.0))
                            .pb(px(20.0))
                            .on_mouse_move(cx.listener(|this, _event, _window, cx| {
                                if this.hovered_toast.is_some() {
                                    this.hovered_toast = None;
                                    cx.notify();
                                }
                            }))
                            .child(container),
                    )
                    .into_any(),
            )
        };
        let mut root_bg = colors.background;
        root_bg.a = self.scaled_background_alpha(root_bg.a);

        div()
            .id("termy-root")
            .flex()
            .flex_col()
            .size_full()
            .bg(root_bg)
            .children(titlebar_element)
            .child(
                div()
                    .id("tabbar")
                    .w_full()
                    .h(px(self.tab_bar_height()))
                    .flex_none()
                    .overflow_hidden()
                    .bg(tabbar_bg)
                    .border_b(px(if show_tab_bar { 1.0 } else { 0.0 }))
                    .border_color(tabbar_border)
                    .child(tabs_row),
            )
            .children(banner_element)
            .child(
                div()
                    .id("terminal")
                    .track_focus(&focus_handle)
                    .key_context(key_context)
                    .on_action(cx.listener(Self::handle_toggle_command_palette_action))
                    .on_action(cx.listener(Self::handle_import_colors_action))
                    .on_action(cx.listener(Self::handle_switch_theme_action))
                    .on_action(cx.listener(Self::handle_app_info_action))
                    .on_action(cx.listener(Self::handle_native_sdk_example_action))
                    .on_action(cx.listener(Self::handle_restart_app_action))
                    .on_action(cx.listener(Self::handle_rename_tab_action))
                    .on_action(cx.listener(Self::handle_check_for_updates_action))
                    .on_action(cx.listener(Self::handle_new_tab_action))
                    .on_action(cx.listener(Self::handle_close_tab_action))
                    .on_action(cx.listener(Self::handle_copy_action))
                    .on_action(cx.listener(Self::handle_paste_action))
                    .on_action(cx.listener(Self::handle_zoom_in_action))
                    .on_action(cx.listener(Self::handle_zoom_out_action))
                    .on_action(cx.listener(Self::handle_zoom_reset_action))
                    .on_action(cx.listener(Self::handle_open_search_action))
                    .on_action(cx.listener(Self::handle_close_search_action))
                    .on_action(cx.listener(Self::handle_search_next_action))
                    .on_action(cx.listener(Self::handle_search_previous_action))
                    .on_action(cx.listener(Self::handle_toggle_search_case_sensitive_action))
                    .on_action(cx.listener(Self::handle_toggle_search_regex_action))
                    .on_action(cx.listener(Self::handle_inline_backspace_action))
                    .on_action(cx.listener(Self::handle_inline_delete_action))
                    .on_action(cx.listener(Self::handle_inline_move_left_action))
                    .on_action(cx.listener(Self::handle_inline_move_right_action))
                    .on_action(cx.listener(Self::handle_inline_select_left_action))
                    .on_action(cx.listener(Self::handle_inline_select_right_action))
                    .on_action(cx.listener(Self::handle_inline_select_all_action))
                    .on_action(cx.listener(Self::handle_inline_move_to_start_action))
                    .on_action(cx.listener(Self::handle_inline_move_to_end_action))
                    .on_action(cx.listener(Self::handle_inline_delete_word_backward_action))
                    .on_action(cx.listener(Self::handle_inline_delete_word_forward_action))
                    .on_action(cx.listener(Self::handle_inline_delete_to_start_action))
                    .on_action(cx.listener(Self::handle_inline_delete_to_end_action))
                    .on_key_down(cx.listener(Self::handle_key_down))
                    .on_scroll_wheel(cx.listener(Self::handle_terminal_scroll_wheel))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
                    .on_mouse_move(cx.listener(Self::handle_mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
                    .on_drop(cx.listener(Self::handle_file_drop))
                    .relative()
                    .flex_1()
                    .w_full()
                    .px(px(effective_padding_x))
                    .py(px(effective_padding_y))
                    .overflow_hidden()
                    .bg(terminal_surface_bg_hsla)
                    .font_family(font_family.clone())
                    .text_size(font_size)
                    .child(terminal_grid_layer)
                    .children(terminal_scrollbar_overlay)
                    .children(command_palette_overlay)
                    .children(search_overlay),
            )
            .children(toast_overlay)
    }
}
