use super::*;

impl TerminalView {
    fn font_size_cache_key(font_size: Pixels) -> u32 {
        let font_size_px: f32 = font_size.into();
        font_size_px.to_bits()
    }

    pub(in super::super) fn cached_cell_size_for_font_size(
        &self,
        font_size: Pixels,
    ) -> Option<Size<Pixels>> {
        self.cell_size_cache
            .get(&Self::font_size_cache_key(font_size))
            .copied()
    }

    fn fallback_cell_size() -> Size<Pixels> {
        let default = TerminalSize::default();
        Size {
            width: default.cell_width,
            height: default.cell_height,
        }
    }

    pub(in super::super) fn layout_cell_size(&self) -> Size<Pixels> {
        self.cached_cell_size_for_font_size(self.font_size)
            .unwrap_or_else(Self::fallback_cell_size)
    }

    fn pane_local_zoom_enabled_for_tab(&self, tab: &TerminalTab) -> bool {
        self.runtime_kind() == RuntimeKind::Native && tab.panes.len() > 1
    }

    fn clamped_font_size_for_zoom_steps(
        base_font_size: Pixels,
        zoom_steps: i16,
        use_local_zoom: bool,
    ) -> Pixels {
        let base_font_size: f32 = base_font_size.into();
        let effective_font_size = if use_local_zoom {
            base_font_size + (zoom_steps as f32 * ZOOM_STEP)
        } else {
            base_font_size
        };
        px(effective_font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE))
    }

    fn effective_font_size_for_zoom_steps(&self, zoom_steps: i16, use_local_zoom: bool) -> Pixels {
        Self::clamped_font_size_for_zoom_steps(self.font_size, zoom_steps, use_local_zoom)
    }

    pub(in super::super) fn effective_font_size_for_pane_in_tab(
        &self,
        tab: &TerminalTab,
        pane: &TerminalPane,
    ) -> Pixels {
        self.effective_font_size_for_zoom_steps(
            pane.pane_zoom_steps,
            self.pane_local_zoom_enabled_for_tab(tab),
        )
    }

    fn active_pane_uses_local_zoom(&self) -> bool {
        self.active_tab_ref()
            .is_some_and(|tab| self.pane_local_zoom_enabled_for_tab(tab))
    }

    fn adjust_active_pane_zoom_steps(&mut self, step_delta: i16, cx: &mut Context<Self>) {
        if !self.active_pane_uses_local_zoom() {
            return;
        }

        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let Some(active_pane_index) = tab.active_pane_index() else {
            return;
        };
        let Some(active_pane) = tab.panes.get_mut(active_pane_index) else {
            return;
        };

        let current_steps = active_pane.pane_zoom_steps;
        let current_font_size =
            Self::clamped_font_size_for_zoom_steps(self.font_size, current_steps, true);
        let next_steps = current_steps.saturating_add(step_delta);
        let next_font_size =
            Self::clamped_font_size_for_zoom_steps(self.font_size, next_steps, true);
        if current_font_size == next_font_size {
            return;
        }

        active_pane.pane_zoom_steps = next_steps;
        self.clear_terminal_scrollbar_marker_cache();
        cx.notify();
    }

    fn reset_active_pane_zoom_steps(&mut self, cx: &mut Context<Self>) {
        if !self.active_pane_uses_local_zoom() {
            return;
        }

        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let Some(active_pane_index) = tab.active_pane_index() else {
            return;
        };
        let Some(active_pane) = tab.panes.get_mut(active_pane_index) else {
            return;
        };
        if active_pane.pane_zoom_steps == 0 {
            return;
        }

        active_pane.pane_zoom_steps = 0;
        self.clear_terminal_scrollbar_marker_cache();
        cx.notify();
    }

    fn terminal_grid_size_for_pane_count(
        pane_count: usize,
        viewport_width: f32,
        viewport_height: f32,
        sidebar_width: f32,
        content_top_inset: f32,
        padding_x: f32,
        padding_y: f32,
        cell_width: f32,
        cell_height: f32,
    ) -> (u16, u16) {
        let (outer_padding_x, outer_padding_y) = if Self::uses_outer_terminal_padding(pane_count) {
            (padding_x, padding_y)
        } else {
            (0.0, 0.0)
        };
        let terminal_width =
            (viewport_width - sidebar_width - (outer_padding_x * 2.0)).max(cell_width * 2.0);
        let terminal_height =
            (viewport_height - content_top_inset - (outer_padding_y * 2.0)).max(cell_height);
        (
            Self::compute_terminal_cols(terminal_width, cell_width, false),
            Self::compute_terminal_rows(terminal_height, cell_height),
        )
    }

    fn repair_native_tab_active_pane_for_resize(tab: &mut TerminalTab) -> bool {
        if tab.panes.is_empty() {
            return false;
        }
        if tab.has_active_pane() {
            return true;
        }

        // Restored native layouts can carry a pane id that no longer exists.
        // Repair that invariant here so a resize never crashes the app.
        let fallback_id = tab
            .panes
            .first()
            .map(|pane| pane.id.clone())
            .expect("non-empty native tab must have a first pane");
        log::warn!(
            "native resize repaired stale active pane id '{}' for tab '{}'; falling back to '{}'",
            tab.active_pane_id,
            tab.window_id,
            fallback_id
        );
        tab.active_pane_id = fallback_id;
        true
    }

    pub(in super::super) fn execute_layout_command_action(
        &mut self,
        action: CommandAction,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            CommandAction::ZoomIn => {
                if self.active_pane_uses_local_zoom() {
                    self.adjust_active_pane_zoom_steps(1, cx);
                } else {
                    let current: f32 = self.font_size.into();
                    self.update_zoom(current + ZOOM_STEP, cx);
                }
                true
            }
            CommandAction::ZoomOut => {
                if self.active_pane_uses_local_zoom() {
                    self.adjust_active_pane_zoom_steps(-1, cx);
                } else {
                    let current: f32 = self.font_size.into();
                    self.update_zoom(current - ZOOM_STEP, cx);
                }
                true
            }
            CommandAction::ZoomReset => {
                if self.active_pane_uses_local_zoom() {
                    self.reset_active_pane_zoom_steps(cx);
                } else {
                    self.update_zoom(self.base_font_size, cx);
                }
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
        self.cell_size_cache.clear();
        self.clear_tab_title_width_cache();
        cx.notify();
    }

    pub(in super::super) fn calculate_cell_size_for_font_size(
        &mut self,
        font_size: Pixels,
        window: &mut Window,
        _cx: &App,
    ) -> Size<Pixels> {
        if let Some(cell_size) = self.cached_cell_size_for_font_size(font_size) {
            return cell_size;
        }

        let font = Font {
            family: self.font_family.clone(),
            weight: FontWeight::NORMAL,
            ..Default::default()
        };

        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let cell_width = text_system
            .advance(font_id, font_size, 'M')
            .map(|advance| advance.width)
            .unwrap_or(px(9.0));

        let cell_height = font_size * self.line_height;

        let cell_size = Size {
            width: cell_width,
            height: cell_height,
        };
        self.cell_size_cache
            .insert(Self::font_size_cache_key(font_size), cell_size);
        cell_size
    }

    pub(in super::super) fn calculate_cell_size(
        &mut self,
        window: &mut Window,
        cx: &App,
    ) -> Size<Pixels> {
        self.calculate_cell_size_for_font_size(self.font_size, window, cx)
    }

    pub(in super::super) fn sync_terminal_size(
        &mut self,
        window: &mut Window,
        layout_cell_size: Size<Pixels>,
        cx: &App,
    ) {
        let viewport = window.viewport_size();
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let cell_width: f32 = layout_cell_size.width.into();
        let cell_height: f32 = layout_cell_size.height.into();

        if cell_width <= 0.0 || cell_height <= 0.0 {
            return;
        }

        let sidebar_width = self.terminal_left_sidebar_width();
        let content_top_inset = self.terminal_content_top_inset();
        let backend_mode = self.runtime_kind();
        let runtime_uses_tmux = matches!(backend_mode, RuntimeKind::Tmux);
        let active_pane_count = self
            .tabs
            .get(self.active_tab)
            .map_or(0, |tab| tab.panes.len());
        let total_sidebar_width = sidebar_width + self.terminal_right_panel_width();
        let (cols, rows) = Self::terminal_grid_size_for_pane_count(
            active_pane_count,
            viewport_width,
            viewport_height,
            total_sidebar_width,
            content_top_inset,
            self.padding_x,
            self.padding_y,
            cell_width,
            cell_height,
        );

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
                    if !Self::repair_native_tab_active_pane_for_resize(tab) {
                        continue;
                    }
                    let (cols, rows) = Self::terminal_grid_size_for_pane_count(
                        tab.panes.len(),
                        viewport_width,
                        viewport_height,
                        total_sidebar_width,
                        content_top_inset,
                        self.padding_x,
                        self.padding_y,
                        cell_width,
                        cell_height,
                    );
                    Self::sync_native_tab_pane_geometry(tab, cols, rows);
                }
            }
        }

        for tab_index in 0..self.tabs.len() {
            let pane_count = self.tabs[tab_index].panes.len();
            let tab_uses_native_split_padding =
                Self::uses_native_split_content_padding(runtime_uses_tmux, pane_count);
            let (content_padding_x, content_padding_y) = if tab_uses_native_split_padding {
                (self.padding_x, self.padding_y)
            } else {
                (0.0, 0.0)
            };
            let use_local_zoom = backend_mode == RuntimeKind::Native && pane_count > 1;
            for pane_index in 0..pane_count {
                let pane_font_size = {
                    let pane = &self.tabs[tab_index].panes[pane_index];
                    self.effective_font_size_for_zoom_steps(pane.pane_zoom_steps, use_local_zoom)
                };
                let pane_cell_size = if backend_mode == RuntimeKind::Native {
                    self.calculate_cell_size_for_font_size(pane_font_size, window, cx)
                } else {
                    layout_cell_size
                };
                let pane = &self.tabs[tab_index].panes[pane_index];
                let mut pane_cols = pane.width.max(1);
                let mut pane_rows = pane.height.max(1);
                if content_padding_x > 0.0 || content_padding_y > 0.0 {
                    let pane_width_px = (f32::from(pane.width) * cell_width).max(cell_width);
                    let pane_height_px = (f32::from(pane.height) * cell_height).max(cell_height);
                    let pane_cell_width: f32 = pane_cell_size.width.into();
                    let pane_cell_height: f32 = pane_cell_size.height.into();
                    pane_cols = ((pane_width_px - (content_padding_x * 2.0)).max(pane_cell_width)
                        / pane_cell_width)
                        .floor()
                        .max(1.0) as u16;
                    pane_rows = ((pane_height_px - (content_padding_y * 2.0)).max(pane_cell_height)
                        / pane_cell_height)
                        .floor()
                        .max(1.0) as u16;
                }
                let next_size = TerminalSize {
                    cols: pane_cols,
                    rows: pane_rows,
                    cell_width: pane_cell_size.width,
                    cell_height: pane_cell_size.height,
                };
                let current = pane.terminal.size();
                if current.cols != next_size.cols
                    || current.rows != next_size.rows
                    || current.cell_width != next_size.cell_width
                    || current.cell_height != next_size.cell_height
                {
                    pane.terminal.resize(next_size);
                }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_terminal() -> Terminal {
        Terminal::new_tmux(TerminalSize::default(), TerminalOptions::default())
    }

    fn test_pane(id: &str) -> TerminalPane {
        TerminalPane {
            id: id.to_string(),
            left: 0,
            top: 0,
            width: 1,
            height: 1,
            pane_zoom_steps: 0,
            degraded: false,
            terminal: test_terminal(),
            render_cache: RefCell::new(TerminalPaneRenderCache::default()),
            last_alternate_screen: Cell::new(false),
        }
    }

    #[test]
    fn terminal_grid_size_uses_outer_padding_only_for_single_pane_tabs() {
        let single_pane = TerminalView::terminal_grid_size_for_pane_count(
            1, 800.0, 600.0, 0.0, 32.0, 12.0, 8.0, 10.0, 20.0,
        );
        let split_pane = TerminalView::terminal_grid_size_for_pane_count(
            2, 800.0, 600.0, 0.0, 32.0, 12.0, 8.0, 10.0, 20.0,
        );

        assert_eq!(single_pane, (77, 27));
        assert_eq!(split_pane, (80, 28));
    }

    #[test]
    fn repair_native_tab_active_pane_for_resize_falls_back_to_first_pane() {
        let mut tab = TerminalTab {
            id: 1,
            window_id: "@native-1".to_string(),
            window_index: 0,
            panes: vec![test_pane("%native-1"), test_pane("%native-2")],
            active_pane_id: "%missing".to_string(),
            agent_thread_id: None,
            pinned: false,
            manual_title: None,
            explicit_title: None,
            explicit_title_is_prediction: false,
            shell_title: None,
            current_command: None,
            pending_command_title: None,
            pending_command_token: 0,
            last_prompt_cwd: None,
            title: DEFAULT_TAB_TITLE.to_string(),
            title_text_width: 0.0,
            sticky_title_width: 0.0,
            display_width: TAB_MIN_WIDTH,
            running_process: false,
            agent_command_has_started: false,
        };

        assert!(TerminalView::repair_native_tab_active_pane_for_resize(
            &mut tab
        ));
        assert_eq!(tab.active_pane_id, "%native-1");
    }
}
