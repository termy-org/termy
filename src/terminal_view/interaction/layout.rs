use super::*;

impl TerminalView {
    pub(in super::super) fn effective_vertical_tab_strip_width(&self) -> f32 {
        if !self.vertical_tabs {
            return 0.0;
        }

        if self.vertical_tabs_minimized {
            VERTICAL_TAB_STRIP_COLLAPSED_WIDTH
        } else {
            self.vertical_tabs_width
                .clamp(VERTICAL_TAB_STRIP_MIN_WIDTH, VERTICAL_TAB_STRIP_MAX_WIDTH)
        }
    }

    fn tab_strip_sidebar_width(&self) -> f32 {
        self.effective_vertical_tab_strip_width()
    }

    fn agent_sidebar_width(&self) -> f32 {
        if self.agent_sidebar_visible() {
            self.agent_sidebar_width
        } else {
            0.0
        }
    }

    pub(in super::super) fn native_pane_min_extent_for_axis(axis: PaneResizeAxis) -> u16 {
        match axis {
            PaneResizeAxis::Horizontal => NATIVE_PANE_MIN_COLS,
            PaneResizeAxis::Vertical => NATIVE_PANE_MIN_ROWS,
        }
    }

    pub(in super::super) fn native_min_extent_allowed(
        total_extent: u16,
        pane_count: usize,
        min_extent: u16,
    ) -> u16 {
        let Some(pane_count) = u16::try_from(pane_count).ok() else {
            return 1;
        };
        if pane_count == 0 {
            return 1;
        }
        let required = min_extent.saturating_mul(pane_count);
        if total_extent >= required {
            min_extent
        } else {
            (total_extent / pane_count).max(1)
        }
    }

    fn compute_terminal_cols(terminal_width: f32, cell_width: f32, edge_to_edge_grid: bool) -> u16 {
        let cols = if edge_to_edge_grid {
            (terminal_width / cell_width).ceil()
        } else {
            (terminal_width / cell_width).floor()
        };
        cols.max(2.0) as u16
    }

    fn compute_terminal_rows(terminal_height: f32, cell_height: f32) -> u16 {
        debug_assert!(
            cell_height > 0.0,
            "compute_terminal_rows: cell_height must be > 0"
        );
        debug_assert!(
            terminal_height >= 0.0,
            "compute_terminal_rows: terminal_height must be >= 0"
        );
        // Always floor rows to avoid over-allocation that clips bottom status/help lines in TUIs.
        (terminal_height / cell_height).floor().max(1.0) as u16
    }

    fn scale_native_pane_edge(edge: u16, old_extent: u16, new_extent: u16) -> u16 {
        if old_extent <= 1 || new_extent == 0 {
            return 0;
        }
        if edge == 0 {
            return 0;
        }
        if edge >= old_extent {
            return new_extent;
        }

        let numerator = u32::from(edge) * u32::from(new_extent) + (u32::from(old_extent) / 2);
        let scaled = numerator / u32::from(old_extent);
        scaled.min(u32::from(new_extent)) as u16
    }

    fn sync_native_tab_pane_geometry(tab: &mut TerminalTab, cols: u16, rows: u16) {
        if tab.panes.is_empty() {
            return;
        }

        let cols = cols.max(1);
        let rows = rows.max(1);

        if tab.panes.len() == 1 {
            if let Some(only) = tab.panes.first_mut() {
                only.left = 0;
                only.top = 0;
                only.width = cols;
                only.height = rows;
                tab.active_pane_id = only.id.clone();
            }
            return;
        }

        let old_cols = tab
            .panes
            .iter()
            .map(|pane| pane.left.saturating_add(pane.width))
            .max()
            .unwrap_or(cols)
            .max(1);
        let old_rows = tab
            .panes
            .iter()
            .map(|pane| pane.top.saturating_add(pane.height))
            .max()
            .unwrap_or(rows)
            .max(1);
        for pane in &mut tab.panes {
            let old_left = pane.left;
            let old_top = pane.top;
            let old_right = pane.left.saturating_add(pane.width);
            let old_bottom = pane.top.saturating_add(pane.height);

            let mut new_left =
                Self::scale_native_pane_edge(old_left, old_cols, cols).min(cols.saturating_sub(1));
            let mut new_top =
                Self::scale_native_pane_edge(old_top, old_rows, rows).min(rows.saturating_sub(1));
            let mut new_right = Self::scale_native_pane_edge(old_right, old_cols, cols).min(cols);
            let mut new_bottom = Self::scale_native_pane_edge(old_bottom, old_rows, rows).min(rows);

            if new_right <= new_left {
                new_right = (new_left + 1).min(cols);
                new_left = new_right.saturating_sub(1);
            }
            if new_bottom <= new_top {
                new_bottom = (new_top + 1).min(rows);
                new_top = new_bottom.saturating_sub(1);
            }

            pane.left = new_left;
            pane.top = new_top;
            pane.width = new_right.saturating_sub(new_left).max(1);
            pane.height = new_bottom.saturating_sub(new_top).max(1);
        }

        if !tab.panes.iter().any(|pane| pane.id == tab.active_pane_id)
            && let Some(pane) = tab.panes.first()
        {
            tab.active_pane_id = pane.id.clone();
        }
    }

    fn should_emit_tmux_resize_error_toast(&mut self, now: Instant) -> bool {
        let debounce_window = Duration::from_millis(TMUX_RESIZE_ERROR_TOAST_DEBOUNCE_MS);
        match self.last_tmux_resize_error_at {
            Some(previous) if now.saturating_duration_since(previous) < debounce_window => false,
            _ => {
                self.last_tmux_resize_error_at = Some(now);
                true
            }
        }
    }

    pub(in super::super) fn terminal_content_position(
        &self,
        position: gpui::Point<Pixels>,
    ) -> (f32, f32) {
        let x: f32 = position.x.into();
        let y: f32 = position.y.into();
        // Mouse coordinates arrive in window space; subtract chrome so terminal hit-testing
        // stays aligned with rendered rows in both native and tmux runtimes.
        (
            x - self.tab_strip_sidebar_width(),
            Self::window_y_to_terminal_content_y(y, self.chrome_height()),
        )
    }

    pub(in super::super) fn window_y_to_terminal_content_y(
        window_y: f32,
        chrome_height: f32,
    ) -> f32 {
        window_y - chrome_height
    }

    pub(in super::super) fn execute_layout_command_action(
        &mut self,
        action: CommandAction,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            CommandAction::ZoomIn => {
                let current: f32 = self.font_size.into();
                self.update_zoom(current + ZOOM_STEP, cx);
                true
            }
            CommandAction::ZoomOut => {
                let current: f32 = self.font_size.into();
                self.update_zoom(current - ZOOM_STEP, cx);
                true
            }
            CommandAction::ZoomReset => {
                self.update_zoom(self.base_font_size, cx);
                true
            }
            _ => false,
        }
    }

    pub(in super::super) fn update_zoom(&mut self, next_size: f32, cx: &mut Context<Self>) {
        let clamped = next_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let current: f32 = self.font_size.into();
        if (current - clamped).abs() < f32::EPSILON {
            return;
        }

        self.font_size = px(clamped);
        // Force cell size recalc so terminal grid reflows at the new zoom.
        self.cell_size = None;
        self.clear_tab_title_width_cache();
        cx.notify();
    }

    pub(in super::super) fn calculate_cell_size(
        &mut self,
        window: &mut Window,
        _cx: &App,
    ) -> Size<Pixels> {
        if let Some(cell_size) = self.cell_size {
            return cell_size;
        }

        let font = Font {
            family: self.font_family.clone(),
            weight: FontWeight::NORMAL,
            ..Default::default()
        };

        // Measure 'M' character width for monospace
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let cell_width = text_system
            .advance(font_id, self.font_size, 'M')
            .map(|advance| advance.width)
            .unwrap_or(px(9.0));

        let cell_height = self.font_size * self.line_height;

        let cell_size = Size {
            width: cell_width,
            height: cell_height,
        };
        self.cell_size = Some(cell_size);
        cell_size
    }

    pub(in super::super) fn sync_terminal_size(
        &mut self,
        window: &Window,
        cell_size: Size<Pixels>,
    ) {
        // Use stable padding for PTY sizing that does NOT depend on alternate_screen_mode.
        // This prevents resize feedback loops when TUI apps (e.g. lazygit) toggle between
        // alternate and normal screen buffers.  Visual padding is handled separately by
        // effective_terminal_padding / native_split_content_padding in the render path.
        let (padding_x, padding_y) = if self
            .tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.panes.len() > 1)
        {
            (0.0, 0.0)
        } else {
            (self.padding_x, self.padding_y)
        };
        let viewport = window.viewport_size();
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let cell_width: f32 = cell_size.width.into();
        let cell_height: f32 = cell_size.height.into();

        if cell_width <= 0.0 || cell_height <= 0.0 {
            return;
        }

        let terminal_width = (viewport_width
            - self.tab_strip_sidebar_width()
            - self.agent_sidebar_width()
            - (padding_x * 2.0))
            .max(cell_width * 2.0);
        let terminal_height =
            (viewport_height - self.chrome_height() - (padding_y * 2.0)).max(cell_height);
        let backend_mode = self.runtime_kind();
        let cols = Self::compute_terminal_cols(terminal_width, cell_width, false);
        let rows = Self::compute_terminal_rows(terminal_height, cell_height);

        match backend_mode {
            RuntimeKind::Tmux => {
                if (self.tmux_client_cols() != cols || self.tmux_client_rows() != rows)
                    && let Err(error) = self.sync_tmux_client_size(cols, rows)
                {
                    let now = Instant::now();
                    if self.should_emit_tmux_resize_error_toast(now) {
                        termy_toast::error(format!("tmux resize failed: {error}"));
                    } else {
                        log::debug!("tmux resize failed (toast debounced): {error}");
                    }
                }
            }
            RuntimeKind::Native => {
                for tab in &mut self.tabs {
                    Self::sync_native_tab_pane_geometry(tab, cols, rows);
                }
            }
        }

        for tab in &self.tabs {
            let tab_uses_native_split_padding = !self.runtime_uses_tmux() && tab.panes.len() > 1;
            let (content_padding_x, content_padding_y) = if tab_uses_native_split_padding {
                (self.padding_x, self.padding_y)
            } else {
                (0.0, 0.0)
            };
            for pane in &tab.panes {
                let mut pane_cols = pane.width.max(1);
                let mut pane_rows = pane.height.max(1);
                if content_padding_x > 0.0 || content_padding_y > 0.0 {
                    let pane_width_px = (f32::from(pane.width) * cell_width).max(cell_width);
                    let pane_height_px = (f32::from(pane.height) * cell_height).max(cell_height);
                    pane_cols = ((pane_width_px - (content_padding_x * 2.0)).max(cell_width)
                        / cell_width)
                        .floor()
                        .max(1.0) as u16;
                    pane_rows = ((pane_height_px - (content_padding_y * 2.0)).max(cell_height)
                        / cell_height)
                        .floor()
                        .max(1.0) as u16;
                }
                let next_size = TerminalSize {
                    cols: pane_cols,
                    rows: pane_rows,
                    cell_width: cell_size.width,
                    cell_height: cell_size.height,
                };
                let current = pane.terminal.size();
                if current.cols != next_size.cols
                    || current.rows != next_size.rows
                    || current.cell_width != next_size.cell_width
                    || current.cell_height != next_size.cell_height
                {
                    pane.terminal.resize(next_size);
                }

                // Detect alternate-screen transitions.  When a TUI app (re-)enters
                // the alternate screen, send SIGWINCH so it refreshes its display
                // even though the PTY dimensions are stable.
                let alt_screen = pane.terminal.alternate_screen_mode();
                let prev_alt_screen = pane.last_alternate_screen.get();
                if alt_screen != prev_alt_screen {
                    pane.last_alternate_screen.set(alt_screen);
                    if alt_screen {
                        pane.terminal.nudge_resize();
                    }
                }
            }
        }
    }

    pub(in super::super) const fn titlebar_height() -> f32 {
        if TITLEBAR_HEIGHT > TABBAR_HEIGHT {
            TITLEBAR_HEIGHT
        } else {
            TABBAR_HEIGHT
        }
    }

    pub(in super::super) fn update_banner_height(&self) -> f32 {
        #[cfg(target_os = "macos")]
        if self.show_update_banner {
            return UPDATE_BANNER_HEIGHT;
        }
        0.0
    }

    pub(in super::super) fn chrome_height(&self) -> f32 {
        Self::titlebar_height() + self.update_banner_height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_y_to_terminal_content_y_subtracts_non_zero_chrome() {
        assert_eq!(
            TerminalView::window_y_to_terminal_content_y(120.0, 34.0),
            86.0
        );
    }

    #[test]
    fn window_y_to_terminal_content_y_is_identity_when_chrome_is_zero() {
        assert_eq!(
            TerminalView::window_y_to_terminal_content_y(120.0, 0.0),
            120.0
        );
    }

    #[test]
    fn window_y_to_terminal_content_y_can_be_negative_when_cursor_is_above_chrome() {
        assert_eq!(
            TerminalView::window_y_to_terminal_content_y(20.0, 40.0),
            -20.0
        );
    }

    #[test]
    fn compute_terminal_rows_floors_fractional_row_count() {
        assert_eq!(TerminalView::compute_terminal_rows(24.1, 12.0), 2);
        assert_eq!(TerminalView::compute_terminal_rows(23.9, 12.0), 1);
    }

    #[test]
    fn compute_terminal_rows_enforces_minimum_one_row() {
        assert_eq!(TerminalView::compute_terminal_rows(0.5, 12.0), 1);
    }

    #[test]
    fn compute_terminal_cols_preserves_edge_to_edge_ceil_behavior() {
        assert_eq!(TerminalView::compute_terminal_cols(30.1, 10.0, true), 4);
        assert_eq!(TerminalView::compute_terminal_cols(30.1, 10.0, false), 3);
        assert_eq!(TerminalView::compute_terminal_cols(0.1, 10.0, true), 2);
        assert_eq!(TerminalView::compute_terminal_cols(0.1, 10.0, false), 2);
    }
}
