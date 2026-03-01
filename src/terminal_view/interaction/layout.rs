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
        // Force cell size recalc so terminal grid reflows at the new zoom.
        self.cell_size = None;
        self.clear_tab_title_width_cache();
        cx.notify();
    }

    pub(in super::super) fn calculate_cell_size(&mut self, window: &mut Window, _cx: &App) -> Size<Pixels> {
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

    pub(in super::super) fn sync_terminal_size(&mut self, window: &Window, cell_size: Size<Pixels>) {
        let (padding_x, padding_y) = self.effective_terminal_padding();
        let viewport = window.viewport_size();
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let cell_width: f32 = cell_size.width.into();
        let cell_height: f32 = cell_size.height.into();

        if cell_width <= 0.0 || cell_height <= 0.0 {
            return;
        }

        let terminal_width = (viewport_width - (padding_x * 2.0)).max(cell_width * 2.0);
        let terminal_height =
            (viewport_height - self.chrome_height() - (padding_y * 2.0)).max(cell_height);
        let cols = (terminal_width / cell_width).floor().max(2.0) as u16;
        let rows = (terminal_height / cell_height).floor().max(1.0) as u16;

        if self.runtime_uses_tmux() {
            if self.tmux_client_cols != cols || self.tmux_client_rows != rows {
                let Some(tmux_client) = self.tmux_client() else {
                    return;
                };
                match tmux_client.set_client_size(cols, rows) {
                    Ok(()) => {
                        self.tmux_client_cols = cols;
                        self.tmux_client_rows = rows;
                        let _ = self.refresh_tmux_snapshot_for_client_size(cols, rows);
                    }
                    Err(error) => {
                        termy_toast::error(format!("tmux resize failed: {error}"));
                    }
                }
            }
        } else {
            self.tmux_client_cols = cols;
            self.tmux_client_rows = rows;
            if let Some(tab) = self.tabs.get_mut(self.active_tab)
                && let Some(pane) = tab.panes.get_mut(0)
            {
                pane.left = 0;
                pane.top = 0;
                pane.width = cols.max(1);
                pane.height = rows.max(1);
                tab.active_pane_id = pane.id.clone();
            }
        }

        for tab in &self.tabs {
            for pane in &tab.panes {
                let next_size = TerminalSize {
                    cols: pane.width.max(1),
                    rows: pane.height.max(1),
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
