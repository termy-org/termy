use super::*;
use alacritty_terminal::grid::Dimensions;

impl TerminalView {
    pub(super) fn open_search(&mut self, cx: &mut Context<Self>) {
        if self.search_open {
            return;
        }

        // Close other overlays
        if self.command_palette_open {
            self.close_command_palette(cx);
        }
        if self.renaming_tab.is_some() {
            self.cancel_rename_tab(cx);
        }

        self.search_open = true;
        self.search_state.open();
        self.search_input.clear();
        self.clear_terminal_scrollbar_marker_cache();
        self.reset_cursor_blink_phase();
        cx.notify();
    }

    pub(super) fn close_search(&mut self, cx: &mut Context<Self>) {
        if !self.search_open {
            return;
        }

        self.search_open = false;
        self.search_state.close();
        self.search_input.clear();
        self.clear_terminal_scrollbar_marker_cache();
        cx.notify();
    }

    pub(super) fn search_next(&mut self, cx: &mut Context<Self>) {
        if !self.search_open || self.search_state.results().is_empty() {
            return;
        }

        self.search_state.next_match();
        self.scroll_to_current_match(cx);
        cx.notify();
    }

    pub(super) fn search_previous(&mut self, cx: &mut Context<Self>) {
        if !self.search_open || self.search_state.results().is_empty() {
            return;
        }

        self.search_state.previous_match();
        self.scroll_to_current_match(cx);
        cx.notify();
    }

    fn scroll_to_current_match(&mut self, cx: &mut Context<Self>) {
        let Some(current) = self.search_state.results().current() else {
            return;
        };

        let active_tab = self.active_tab;
        let terminal = &self.tabs[active_tab].terminal;
        let size = terminal.size();
        let rows = size.rows as i32;

        // Calculate required scroll to make match visible
        let (display_offset, history_size) = terminal.scroll_state();

        // Convert match line to viewport-relative position
        // match.line is in Alacritty coordinates (negative = history)
        let viewport_row = current.line + display_offset as i32;

        // Check if match is in the current viewport
        if viewport_row >= 0 && viewport_row < rows {
            // Match is already visible
            return;
        }

        // Scroll to make the match visible (centered if possible)
        let target_offset = if current.line < 0 {
            // Match is in scrollback history
            (-current.line) as usize
        } else {
            // Match is below viewport - scroll down
            0
        };

        // Clamp to valid range
        let target_offset = target_offset.min(history_size);
        let delta = target_offset as i32 - display_offset as i32;

        if delta != 0 {
            self.active_terminal().scroll_display(delta);
            self.mark_terminal_scrollbar_activity(cx);
        }
    }

    pub(super) fn perform_search(&mut self) {
        let query = self.search_input.text().to_string();
        self.search_state.set_query(&query);

        if !self.search_state.has_valid_pattern() {
            return;
        }

        let active_tab = self.active_tab;
        let terminal = &self.tabs[active_tab].terminal;
        let (display_offset, history_size) = terminal.scroll_state();
        let rows = terminal.size().rows as i32;

        // Search range: from deepest history to current viewport
        let start_line = -(history_size as i32);
        let end_line = rows - 1;
        let search_state = &mut self.search_state;

        // Search directly against terminal grid lines to avoid duplicating
        // the entire visible + scrollback range in a temporary map.
        terminal.with_term(|term| {
            let grid = term.grid();
            search_state.search(start_line, end_line, |line_idx| {
                extract_line_text(grid, line_idx, display_offset)
            });
        });

        // Jump to nearest match to current viewport
        let viewport_center = -(display_offset as i32) + rows / 2;
        self.search_state.jump_to_nearest(viewport_center);
        if self.search_state.results().is_empty() {
            self.clear_terminal_scrollbar_marker_cache();
        }
    }

    pub(super) fn handle_search_key_down(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "escape" => {
                self.close_search(cx);
            }
            "enter" => {
                self.search_next(cx);
            }
            "shift-enter" => {
                self.search_previous(cx);
            }
            _ => {
                // Text input is handled elsewhere via InlineInput actions
            }
        }
    }

    pub(super) fn handle_search_input_changed(&mut self, cx: &mut Context<Self>) {
        // Debounce search
        self.search_debounce_token = self.search_debounce_token.wrapping_add(1);
        let token = self.search_debounce_token;

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            smol::Timer::after(Duration::from_millis(SEARCH_DEBOUNCE_MS)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    if view.search_debounce_token == token {
                        view.perform_search();
                        view.scroll_to_current_match(cx);
                        cx.notify();
                    }
                })
            });
        })
        .detach();
    }

    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = &self.colors;
        let overlay_style = self.overlay_style();
        let bar_bg = overlay_style.panel_background(SEARCH_BAR_BG_ALPHA);
        let bar_border = overlay_style.panel_cursor(OVERLAY_PANEL_BORDER_ALPHA);
        let input_bg = overlay_style.panel_background(SEARCH_INPUT_BG_ALPHA);
        let counter_text = overlay_style.panel_foreground(SEARCH_COUNTER_TEXT_ALPHA);
        let button_text = overlay_style.panel_foreground(SEARCH_BUTTON_TEXT_ALPHA);
        let button_hover_bg = overlay_style.panel_cursor(SEARCH_BUTTON_HOVER_BG_ALPHA);

        let (current, total) = self.search_state.results().position().unwrap_or((0, 0));

        let counter_label = if total > 0 {
            format!("{} of {}", current, total)
        } else if self.search_input.text().is_empty() {
            String::new()
        } else {
            "No matches".to_string()
        };

        let has_error = self.search_state.error().is_some();
        let error_color = gpui::Rgba {
            r: 0.98,
            g: 0.48,
            b: 0.48,
            a: 1.0,
        };

        div()
            .id("search-bar")
            .absolute()
            .top(px(12.0))
            .right(px(12.0))
            .w(px(SEARCH_BAR_WIDTH))
            .h(px(SEARCH_BAR_HEIGHT))
            .bg(bar_bg)
            .border_1()
            .border_color(if has_error { error_color } else { bar_border })
            .rounded_md()
            .shadow_lg()
            .flex()
            .items_center()
            .px(px(8.0))
            .gap(px(6.0))
            // Search input
            .child(
                div()
                    .flex_1()
                    .h(px(24.0))
                    .rounded_sm()
                    .bg(input_bg)
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .child(self.render_inline_input_layer(
                        Font::default(),
                        px(12.0),
                        colors.foreground.into(),
                        {
                            overlay_style
                                .panel_cursor(SEARCH_INPUT_SELECTION_ALPHA)
                                .into()
                        },
                        InlineInputAlignment::Left,
                        cx,
                    )),
            )
            // Match counter
            .child(
                div()
                    .min_w(px(60.0))
                    .text_size(px(11.0))
                    .text_color(if has_error { error_color } else { counter_text })
                    .child(counter_label),
            )
            // Navigation buttons
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    // Previous button
                    .child(
                        div()
                            .id("search-prev")
                            .w(px(22.0))
                            .h(px(22.0))
                            .rounded_sm()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .text_color(button_text)
                            .hover(|style| style.bg(button_hover_bg))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.search_previous(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child("\u{2191}"), // Up arrow
                    )
                    // Next button
                    .child(
                        div()
                            .id("search-next")
                            .w(px(22.0))
                            .h(px(22.0))
                            .rounded_sm()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .text_color(button_text)
                            .hover(|style| style.bg(button_hover_bg))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.search_next(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child("\u{2193}"), // Down arrow
                    ),
            )
            // Close button
            .child(
                div()
                    .id("search-close")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded_sm()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(button_text)
                    .hover(|style| style.bg(button_hover_bg))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.close_search(cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child("\u{00d7}"), // X
            )
            .into_any()
    }
}

/// Extract text from a terminal grid line
fn extract_line_text(
    grid: &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>,
    line_idx: i32,
    _display_offset: usize,
) -> Option<String> {
    use alacritty_terminal::index::{Column, Line};

    let line = Line(line_idx);
    let cols = grid.columns();

    // Check if line is within grid bounds
    let total_lines = grid.total_lines();
    if line_idx < -(total_lines as i32 - grid.screen_lines() as i32)
        || line_idx >= grid.screen_lines() as i32
    {
        return None;
    }

    let mut text = String::with_capacity(cols);
    for col in 0..cols {
        let cell = &grid[line][Column(col)];
        let c = cell.c;
        if c == '\0' || cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            text.push(' ');
        } else if c.is_control() {
            text.push(' ');
        } else {
            text.push(c);
        }
    }

    Some(text)
}
