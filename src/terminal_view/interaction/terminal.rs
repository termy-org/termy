use super::*;

impl TerminalView {
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

        let terminal_width =
            (viewport_width - self.tab_strip_sidebar_width() - (padding_x * 2.0)).max(cell_width * 2.0);
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
