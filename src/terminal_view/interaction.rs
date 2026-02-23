use super::scrollbar as terminal_scrollbar;
use super::*;
use crate::ui::scrollbar as ui_scrollbar;

impl TerminalView {
    fn command_palette_mode_for_action(action: CommandAction) -> Option<CommandPaletteMode> {
        match action {
            CommandAction::SwitchTheme => Some(CommandPaletteMode::Themes),
            _ => None,
        }
    }

    pub(super) fn has_selection(&self) -> bool {
        matches!((self.selection_anchor, self.selection_head), (Some(anchor), Some(head)) if self.selection_moved || anchor != head)
    }

    pub(super) fn selection_range(&self) -> Option<(CellPos, CellPos)> {
        if !self.has_selection() {
            return None;
        }

        let (anchor, head) = (self.selection_anchor?, self.selection_head?);
        if (head.row, head.col) < (anchor.row, anchor.col) {
            Some((head, anchor))
        } else {
            Some((anchor, head))
        }
    }

    pub(super) fn cell_is_selected(&self, col: usize, row: usize) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };

        let here = (row, col);
        here >= (start.row, start.col) && here <= (end.row, end.col)
    }

    pub(super) fn viewport_row_from_term_line(
        term_line: i32,
        display_offset: usize,
    ) -> Option<usize> {
        usize::try_from(term_line + display_offset as i32).ok()
    }

    fn write_copy_fallback_input(&mut self, _cx: &mut Context<Self>) {
        #[cfg(not(target_os = "macos"))]
        {
            self.active_terminal().write(&[0x03]);
            self.clear_selection();
            _cx.notify();
        }
    }

    fn write_paste_fallback_input(&mut self, _cx: &mut Context<Self>) {
        #[cfg(not(target_os = "macos"))]
        {
            self.active_terminal().write(&[0x16]);
            self.clear_selection();
            _cx.notify();
        }
    }

    fn import_colors_action(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("JSON", &["json"])
                .set_title("Import Colors")
                .pick_file()
                .await;

            let Some(file) = file else {
                return;
            };

            let path = file.path().to_path_buf();
            let result = config::import_colors_from_json(&path);

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    match result {
                        Ok(msg) => {
                            termy_toast::success(msg);
                            view.reload_config(cx);
                        }
                        Err(err) => {
                            termy_toast::error(err);
                        }
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn native_sdk_example_action(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            termy_native_sdk::show_alert(
                "Update Available",
                "A new Termy update is available and ready to install.",
            );
            let confirmed = termy_native_sdk::confirm(
                "Install Update",
                "Would you like to install the latest update now?",
            );

            let _ = cx.update(|cx| {
                this.update(cx, |_view, cx| {
                    if confirmed {
                        termy_toast::success("Update install confirmed");
                    } else {
                        termy_toast::info("Update installation postponed");
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    pub(super) fn position_to_cell(
        &self,
        position: gpui::Point<Pixels>,
        clamp: bool,
    ) -> Option<CellPos> {
        let (padding_x, padding_y) = self.effective_terminal_padding();
        let size = self.active_terminal().size();
        if size.cols == 0 || size.rows == 0 {
            return None;
        }

        let mut x: f32 = position.x.into();
        let mut y: f32 = position.y.into();
        x -= padding_x;
        y -= self.chrome_height() + padding_y;

        let cell_width: f32 = size.cell_width.into();
        let cell_height: f32 = size.cell_height.into();
        if cell_width <= 0.0 || cell_height <= 0.0 {
            return None;
        }

        let mut col = (x / cell_width).floor() as i32;
        let mut row = (y / cell_height).floor() as i32;

        let max_col = i32::from(size.cols) - 1;
        let max_row = i32::from(size.rows) - 1;
        if max_col < 0 || max_row < 0 {
            return None;
        }

        if clamp {
            col = col.clamp(0, max_col);
            row = row.clamp(0, max_row);
        } else if col < 0 || col > max_col || row < 0 || row > max_row {
            return None;
        }

        Some(CellPos {
            col: col as usize,
            row: row as usize,
        })
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let size = self.active_terminal().size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        if cols == 0 || rows == 0 {
            return None;
        }

        let mut grid = vec![vec![' '; cols]; rows];
        self.active_terminal().with_term(|term| {
            let content = term.renderable_content();
            for cell in content.display_iter {
                let Some(row) =
                    Self::viewport_row_from_term_line(cell.point.line.0, content.display_offset)
                else {
                    continue;
                };
                let col = cell.point.column.0;
                if row >= rows || col >= cols {
                    continue;
                }

                let c = cell.cell.c;
                if c != '\0' {
                    grid[row][col] = if c.is_control() { ' ' } else { c };
                }
            }
        });

        let mut lines = Vec::new();
        for row in start.row..=end.row {
            let col_start = if row == start.row { start.col } else { 0 };
            let col_end = if row == end.row {
                end.col
            } else {
                cols.saturating_sub(1)
            };
            let mut line: String = grid[row][col_start..=col_end].iter().collect();
            while line.ends_with(' ') {
                line.pop();
            }
            lines.push(line);
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    pub(super) fn row_text(&self, row: usize) -> Option<Vec<char>> {
        let size = self.active_terminal().size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        if cols == 0 || row >= rows {
            return None;
        }

        let mut line = vec![' '; cols];
        self.active_terminal().with_term(|term| {
            let content = term.renderable_content();
            for cell in content.display_iter {
                let Some(cell_row) =
                    Self::viewport_row_from_term_line(cell.point.line.0, content.display_offset)
                else {
                    continue;
                };
                if cell_row != row {
                    continue;
                }

                let col = cell.point.column.0;
                if col >= cols {
                    continue;
                }

                if cell.cell.flags.intersects(
                    Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
                ) {
                    continue;
                }

                let c = cell.cell.c;
                if c != '\0' {
                    line[col] = if c.is_control() { ' ' } else { c };
                }
            }
        });

        Some(line)
    }

    pub(super) fn link_at_cell(&self, cell: CellPos) -> Option<HoveredLink> {
        let line = self.row_text(cell.row)?;
        let detected = find_link_in_line(&line, cell.col)?;

        Some(HoveredLink {
            row: cell.row,
            start_col: detected.start_col,
            end_col: detected.end_col,
            target: detected.target,
        })
    }

    pub(super) fn open_link(url: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            return Command::new("open")
                .arg(url)
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
        }
        #[cfg(target_os = "linux")]
        {
            return Command::new("xdg-open")
                .arg(url)
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
        }
        #[cfg(target_os = "windows")]
        {
            return Command::new("cmd")
                .args(["/C", "start", "", url])
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
        }
    }

    pub(super) fn restart_application(&self) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {}", e))?;

        #[cfg(target_os = "macos")]
        {
            let app_bundle = exe
                .ancestors()
                .find(|path| {
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("app"))
                        .unwrap_or(false)
                })
                .map(PathBuf::from);

            if let Some(app_bundle) = app_bundle {
                let status = Command::new("open")
                    .arg("-n")
                    .arg(&app_bundle)
                    .status()
                    .map_err(|e| format!("failed to launch app bundle: {}", e))?;
                if status.success() {
                    return Ok(());
                }
                return Err(format!("open returned non-success status: {}", status));
            }
        }

        Command::new(&exe)
            .spawn()
            .map_err(|e| format!("failed to spawn executable: {}", e))?;
        Ok(())
    }

    pub(super) fn is_link_modifier(modifiers: gpui::Modifiers) -> bool {
        modifiers.secondary() && !modifiers.alt && !modifiers.function
    }

    pub(super) fn update_zoom(&mut self, next_size: f32, cx: &mut Context<Self>) {
        let clamped = next_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let current: f32 = self.font_size.into();
        if (current - clamped).abs() < f32::EPSILON {
            return;
        }

        self.font_size = px(clamped);
        // Force cell size recalc so terminal grid reflows at the new zoom.
        self.cell_size = None;
        cx.notify();
    }

    pub(super) fn calculate_cell_size(&mut self, window: &mut Window, _cx: &App) -> Size<Pixels> {
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

    pub(super) fn sync_terminal_size(&mut self, window: &Window, cell_size: Size<Pixels>) {
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
        // In alternate-screen UIs (e.g. fullscreen TUIs), use edge-to-edge sizing
        // so partial-cell remainders don't leave a visible strip on the right/bottom.
        let edge_to_edge_grid = self.active_terminal().alternate_screen_mode();
        let cols = if edge_to_edge_grid {
            (terminal_width / cell_width).ceil()
        } else {
            (terminal_width / cell_width).floor()
        }
        .max(2.0) as u16;
        let rows = if edge_to_edge_grid {
            (terminal_height / cell_height).ceil()
        } else {
            (terminal_height / cell_height).floor()
        }
        .max(1.0) as u16;

        for tab in &mut self.tabs {
            let current = tab.terminal.size();
            if current.cols != cols
                || current.rows != rows
                || current.cell_width != cell_size.width
                || current.cell_height != cell_size.height
            {
                tab.terminal.resize(TerminalSize {
                    cols,
                    rows,
                    cell_width: cell_size.width,
                    cell_height: cell_size.height,
                });
            }
        }
    }

    pub(super) fn terminal_scroll_lines_from_pixels(
        accumulated_pixels: &mut f32,
        delta_pixels: f32,
        line_height: f32,
        viewport_height: f32,
    ) -> i32 {
        if line_height <= f32::EPSILON {
            return 0;
        }

        let old_offset = (*accumulated_pixels / line_height) as i32;
        *accumulated_pixels += delta_pixels;
        let new_offset = (*accumulated_pixels / line_height) as i32;

        if viewport_height > 0.0 {
            *accumulated_pixels %= viewport_height;
        }

        new_offset - old_offset
    }

    pub(super) fn terminal_scroll_delta_to_lines(&mut self, event: &ScrollWheelEvent) -> i32 {
        match event.touch_phase {
            TouchPhase::Started => {
                self.terminal_scroll_accumulator_y = 0.0;
                0
            }
            TouchPhase::Ended => 0,
            TouchPhase::Moved => {
                let size = self.active_terminal().size();
                if size.rows == 0 {
                    return 0;
                }

                let line_height: f32 = size.cell_height.into();
                let viewport_height = line_height * f32::from(size.rows);
                let raw_delta_pixels: f32 = event.delta.pixel_delta(size.cell_height).y.into();
                let delta_pixels = raw_delta_pixels * self.mouse_scroll_multiplier;

                Self::terminal_scroll_lines_from_pixels(
                    &mut self.terminal_scroll_accumulator_y,
                    delta_pixels,
                    line_height,
                    viewport_height,
                )
            }
        }
    }

    fn terminal_scrollbar_hit_test(
        &self,
        position: gpui::Point<Pixels>,
        window: &Window,
    ) -> Option<TerminalScrollbarHit> {
        let (display_offset, _) = self.active_terminal().scroll_state();
        let force_visible = display_offset > 0
            && self.terminal_scrollbar_mode() != ui_scrollbar::ScrollbarVisibilityMode::AlwaysOff;
        let alpha = self.terminal_scrollbar_alpha(Instant::now());
        if !force_visible
            && alpha <= f32::EPSILON
            && !self.terminal_scrollbar_visibility_controller.is_dragging()
        {
            return None;
        }

        let surface = self.terminal_surface_geometry(window)?;
        let scrollbar_left = surface.origin_x + surface.width - TERMINAL_SCROLLBAR_GUTTER_WIDTH;
        let scrollbar_right = surface.origin_x + surface.width;

        let x: f32 = position.x.into();
        if x < scrollbar_left || x > scrollbar_right {
            return None;
        }

        let y: f32 = position.y.into();
        if y < surface.origin_y || y > surface.origin_y + surface.height {
            return None;
        }

        let layout = self.terminal_scrollbar_layout_for_track(surface.height)?;
        let metrics = layout.metrics;
        let local_y = y - surface.origin_y;
        let thumb_hit =
            local_y >= metrics.thumb_top && local_y <= metrics.thumb_top + metrics.thumb_height;

        Some(TerminalScrollbarHit {
            local_y,
            thumb_hit,
            thumb_top: metrics.thumb_top,
        })
    }

    fn apply_terminal_scroll_offset(
        &mut self,
        target_offset: f32,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
    ) -> bool {
        let (display_offset, _) = self.active_terminal().scroll_state();
        let line_height = layout.range.viewport_extent / layout.viewport_rows as f32;
        if line_height <= f32::EPSILON {
            return false;
        }

        let target_display_offset = (ui_scrollbar::invert_offset_axis(
            target_offset,
            layout.range.max_offset,
        ) / line_height)
            .round()
            .clamp(0.0, layout.history_size as f32) as i32;
        let delta = target_display_offset - display_offset as i32;
        if delta == 0 {
            return false;
        }

        self.active_terminal().scroll_display(delta)
    }

    fn handle_terminal_scrollbar_mouse_down(
        &mut self,
        hit: TerminalScrollbarHit,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(surface) = self.terminal_surface_geometry(window) else {
            return;
        };
        let Some(layout) = self.terminal_scrollbar_layout_for_track(surface.height) else {
            return;
        };
        let range = layout.range;
        let metrics = layout.metrics;

        if hit.thumb_hit {
            let thumb_grab_offset = (hit.local_y - hit.thumb_top).clamp(0.0, metrics.thumb_height);
            self.start_terminal_scrollbar_drag(thumb_grab_offset, cx);
            cx.notify();
            return;
        }

        let changed = self.apply_terminal_scroll_offset(
            ui_scrollbar::offset_from_track_click(hit.local_y, range, metrics),
            layout,
        );
        if changed {
            self.terminal_scroll_accumulator_y = 0.0;
        }
        self.mark_terminal_scrollbar_activity(cx);
        cx.notify();
    }

    fn handle_terminal_scrollbar_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.terminal_scrollbar_drag else {
            return;
        };
        let Some(surface) = self.terminal_surface_geometry(window) else {
            return;
        };
        let Some(layout) = self.terminal_scrollbar_layout_for_track(surface.height) else {
            return;
        };
        let range = layout.range;
        let metrics = layout.metrics;

        let y: f32 = position.y.into();
        let local_y = (y - surface.origin_y).clamp(0.0, surface.height);
        let thumb_top = (local_y - drag.thumb_grab_offset).clamp(0.0, metrics.travel);
        let changed = self.apply_terminal_scroll_offset(
            ui_scrollbar::offset_from_thumb_top(thumb_top, range, metrics),
            layout,
        );
        if changed {
            self.terminal_scroll_accumulator_y = 0.0;
            cx.notify();
        }
    }

    fn command_shortcuts_suspended(&self) -> bool {
        self.has_active_inline_input()
    }

    pub(super) fn execute_command_action(
        &mut self,
        action: CommandAction,
        respect_shortcut_suspend: bool,
        cx: &mut Context<Self>,
    ) {
        let shortcuts_suspended = respect_shortcut_suspend && self.command_shortcuts_suspended();

        match action {
            CommandAction::ToggleCommandPalette => {
                if self.command_palette_open {
                    self.close_command_palette(cx);
                } else {
                    self.open_command_palette(cx);
                }
            }
            CommandAction::SwitchTheme => {
                if let Some(mode) = Self::command_palette_mode_for_action(action) {
                    self.command_palette_open = true;
                    self.set_command_palette_mode(mode, false, cx);
                }
            }
            _ if shortcuts_suspended => {}
            CommandAction::Quit => cx.quit(),
            CommandAction::OpenConfig => config::open_config_file(),
            CommandAction::ImportColors => self.import_colors_action(cx),
            CommandAction::AppInfo => {
                let config_path = self
                    .config_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unknown".to_string());
                let message = format!(
                    "Termy v{} | {}-{} | config: {}",
                    crate::APP_VERSION,
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    config_path
                );
                termy_toast::info(message);
                cx.notify();
            }
            CommandAction::NativeSdkExample => {
                self.native_sdk_example_action(cx);
            }
            CommandAction::RestartApp => match self.restart_application() {
                Ok(()) => cx.quit(),
                Err(error) => {
                    termy_toast::error(format!("Restart failed: {}", error));
                    cx.notify();
                }
            },
            CommandAction::RenameTab => {
                if !self.use_tabs {
                    return;
                }

                self.renaming_tab = Some(self.active_tab);
                self.rename_input
                    .set_text(self.tabs[self.active_tab].title.clone());
                self.reset_cursor_blink_phase();
                self.inline_input_selecting = false;
                termy_toast::info("Rename mode enabled");
                cx.notify();
            }
            CommandAction::CheckForUpdates => {
                #[cfg(target_os = "macos")]
                {
                    if let Some(updater) = self.auto_updater.as_ref() {
                        AutoUpdater::check(updater.downgrade(), cx);
                    }
                    self.update_check_toast_id = Some(termy_toast::loading("Checking for updates"));
                    cx.notify();
                }

                #[cfg(not(target_os = "macos"))]
                {
                    termy_toast::info("Auto updates are only available on macOS");
                    cx.notify();
                }
            }
            CommandAction::NewTab => self.add_tab(cx),
            CommandAction::CloseTab => self.close_active_tab(cx),
            CommandAction::Copy => {
                if let Some(selected) = self.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected));
                } else {
                    self.write_copy_fallback_input(cx);
                }
            }
            CommandAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let terminal = self.active_terminal();
                    if terminal.bracketed_paste_mode() {
                        terminal.write(b"\x1b[200~");
                        terminal.write(text.as_bytes());
                        terminal.write(b"\x1b[201~");
                    } else {
                        terminal.write(text.as_bytes());
                    }
                    self.clear_selection();
                    cx.notify();
                } else {
                    self.write_paste_fallback_input(cx);
                }
            }
            CommandAction::ZoomIn => {
                let current: f32 = self.font_size.into();
                self.update_zoom(current + ZOOM_STEP, cx);
            }
            CommandAction::ZoomOut => {
                let current: f32 = self.font_size.into();
                self.update_zoom(current - ZOOM_STEP, cx);
            }
            CommandAction::ZoomReset => self.update_zoom(self.base_font_size, cx),
            // Search
            CommandAction::OpenSearch => self.open_search(cx),
            CommandAction::CloseSearch => self.close_search(cx),
            CommandAction::SearchNext => self.search_next(cx),
            CommandAction::SearchPrevious => self.search_previous(cx),
            CommandAction::ToggleSearchCaseSensitive => {
                self.search_state.toggle_case_sensitive();
                self.perform_search();
                cx.notify();
            }
            CommandAction::ToggleSearchRegex => {
                self.search_state.toggle_regex_mode();
                self.perform_search();
                cx.notify();
            }
            CommandAction::OpenSettings => {
                use crate::settings_view::SettingsWindow;
                use gpui::{Bounds, WindowBounds, WindowOptions, px, size};
                let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);

                #[cfg(target_os = "macos")]
                let titlebar = Some(gpui::TitlebarOptions {
                    title: Some("Settings".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(gpui::point(px(12.0), px(10.0))),
                    ..Default::default()
                });
                #[cfg(target_os = "windows")]
                let titlebar = Some(gpui::TitlebarOptions {
                    title: Some("Settings".into()),
                    ..Default::default()
                });
                #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
                let titlebar = Some(gpui::TitlebarOptions {
                    title: Some("Settings".into()),
                    appears_transparent: true,
                    ..Default::default()
                });

                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar,
                        ..Default::default()
                    },
                    |window, cx| cx.new(|cx| SettingsWindow::new(window, cx)),
                )
                .ok();
            }
        }
    }

    pub(super) fn handle_toggle_command_palette_action(
        &mut self,
        _: &commands::ToggleCommandPalette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ToggleCommandPalette, true, cx);
    }

    pub(super) fn handle_import_colors_action(
        &mut self,
        _: &commands::ImportColors,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ImportColors, true, cx);
    }

    pub(super) fn handle_switch_theme_action(
        &mut self,
        _: &commands::SwitchTheme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SwitchTheme, true, cx);
    }

    pub(super) fn handle_app_info_action(
        &mut self,
        _: &commands::AppInfo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::AppInfo, true, cx);
    }

    pub(super) fn handle_native_sdk_example_action(
        &mut self,
        _: &commands::NativeSdkExample,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::NativeSdkExample, true, cx);
    }

    pub(super) fn handle_restart_app_action(
        &mut self,
        _: &commands::RestartApp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::RestartApp, true, cx);
    }

    pub(super) fn handle_rename_tab_action(
        &mut self,
        _: &commands::RenameTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::RenameTab, true, cx);
    }

    pub(super) fn handle_check_for_updates_action(
        &mut self,
        _: &commands::CheckForUpdates,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::CheckForUpdates, true, cx);
    }

    pub(super) fn handle_new_tab_action(
        &mut self,
        _: &commands::NewTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::NewTab, true, cx);
    }

    pub(super) fn handle_close_tab_action(
        &mut self,
        _: &commands::CloseTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::CloseTab, true, cx);
    }

    pub(super) fn handle_copy_action(
        &mut self,
        _: &commands::Copy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::Copy, true, cx);
    }

    pub(super) fn handle_paste_action(
        &mut self,
        _: &commands::Paste,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::Paste, true, cx);
    }

    pub(super) fn handle_zoom_in_action(
        &mut self,
        _: &commands::ZoomIn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ZoomIn, true, cx);
    }

    pub(super) fn handle_zoom_out_action(
        &mut self,
        _: &commands::ZoomOut,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ZoomOut, true, cx);
    }

    pub(super) fn handle_zoom_reset_action(
        &mut self,
        _: &commands::ZoomReset,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ZoomReset, true, cx);
    }

    pub(super) fn handle_open_search_action(
        &mut self,
        _: &commands::OpenSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::OpenSearch, true, cx);
    }

    pub(super) fn handle_close_search_action(
        &mut self,
        _: &commands::CloseSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::CloseSearch, true, cx);
    }

    pub(super) fn handle_search_next_action(
        &mut self,
        _: &commands::SearchNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SearchNext, true, cx);
    }

    pub(super) fn handle_search_previous_action(
        &mut self,
        _: &commands::SearchPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SearchPrevious, true, cx);
    }

    pub(super) fn handle_toggle_search_case_sensitive_action(
        &mut self,
        _: &commands::ToggleSearchCaseSensitive,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ToggleSearchCaseSensitive, true, cx);
    }

    pub(super) fn handle_toggle_search_regex_action(
        &mut self,
        _: &commands::ToggleSearchRegex,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ToggleSearchRegex, true, cx);
    }

    pub(super) fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_cursor_blink_phase();
        let key = event.keystroke.key.as_str();

        if self.command_palette_open {
            self.handle_command_palette_key_down(key, cx);
            return;
        }

        if self.search_open {
            self.handle_search_key_down(key, cx);
            return;
        }

        if self.renaming_tab.is_some() {
            match key {
                "enter" => {
                    self.commit_rename_tab(cx);
                    return;
                }
                "escape" => {
                    self.cancel_rename_tab(cx);
                    return;
                }
                _ => return,
            }
        }

        if let Some(input) = keystroke_to_input(&event.keystroke) {
            // Check if this is Ctrl+C (0x03) - scroll to bottom to show where we are
            if input == [0x03] {
                self.scroll_to_bottom(cx);
            }

            self.active_terminal().write(&input);
            self.clear_selection();
            // Request a redraw to show the typed character
            cx.notify();
        }
    }

    fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        let (display_offset, _) = self.active_terminal().scroll_state();
        if display_offset > 0 {
            // Scroll down to offset 0 (live output)
            self.active_terminal()
                .scroll_display(-(display_offset as i32));
            self.mark_terminal_scrollbar_activity(cx);
        }
    }

    pub(super) fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Focus the terminal on click
        self.focus_handle.focus(window, cx);
        self.reset_cursor_blink_phase();

        if event.button != MouseButton::Left {
            return;
        }

        if let Some(hit) = self.terminal_scrollbar_hit_test(event.position, window) {
            self.handle_terminal_scrollbar_mouse_down(hit, window, cx);
            cx.stop_propagation();
            return;
        }

        if Self::is_link_modifier(event.modifiers) {
            if let Some(cell) = self.position_to_cell(event.position, false) {
                if let Some(link) = self.link_at_cell(cell) {
                    if !Self::open_link(&link.target) {
                        termy_toast::error("Failed to open link");
                    }
                    if self.clear_hovered_link() {
                        cx.notify();
                    }
                    return;
                }
            }
        }

        let Some(cell) = self.position_to_cell(event.position, false) else {
            self.clear_selection();
            self.clear_hovered_link();
            cx.notify();
            return;
        };

        self.selection_anchor = Some(cell);
        self.selection_head = Some(cell);
        self.selection_dragging = true;
        self.selection_moved = false;
        self.clear_hovered_link();
        cx.notify();
    }

    pub(super) fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_scrollbar_drag.is_some() {
            if event.dragging() {
                self.handle_terminal_scrollbar_drag(event.position, window, cx);
            } else if self.finish_terminal_scrollbar_drag(cx) {
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        if !self.selection_dragging || !event.dragging() {
            if Self::is_link_modifier(event.modifiers) {
                let next = self
                    .position_to_cell(event.position, false)
                    .and_then(|cell| self.link_at_cell(cell));
                if self.hovered_link != next {
                    self.hovered_link = next;
                    cx.notify();
                }
            } else if self.clear_hovered_link() {
                cx.notify();
            }
            return;
        }

        let Some(next_cell) = self.position_to_cell(event.position, true) else {
            return;
        };

        if self.selection_head != Some(next_cell) {
            self.selection_head = Some(next_cell);
            if self.selection_anchor != self.selection_head {
                self.selection_moved = true;
            }
            self.clear_hovered_link();
            cx.notify();
        }
    }

    pub(super) fn handle_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left && self.finish_terminal_scrollbar_drag(cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if event.button != MouseButton::Left || !self.selection_dragging {
            return;
        }

        if let Some(next_cell) = self.position_to_cell(event.position, true) {
            self.selection_head = Some(next_cell);
            if self.selection_anchor != self.selection_head {
                self.selection_moved = true;
            }
        }

        self.selection_dragging = false;
        if !self.selection_moved {
            self.clear_selection();
        }
        self.clear_hovered_link();
        cx.notify();
    }

    pub(super) fn handle_titlebar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        if event.click_count == 2 {
            #[cfg(target_os = "macos")]
            window.titlebar_double_click();
            #[cfg(not(target_os = "macos"))]
            window.zoom_window();
            return;
        }

        window.start_window_move();
    }

    pub(super) fn handle_terminal_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if matches!(event.touch_phase, TouchPhase::Moved) {
            self.mark_terminal_scrollbar_activity(cx);
        }

        let delta_lines = self.terminal_scroll_delta_to_lines(event);
        if delta_lines == 0 {
            return;
        }

        if self.active_terminal().scroll_display(delta_lines) {
            cx.notify();
        } else {
            self.terminal_scroll_accumulator_y = 0.0;
        }
    }

    pub(super) fn handle_file_drop(
        &mut self,
        paths: &ExternalPaths,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths_list = paths.paths();
        if paths_list.is_empty() {
            return;
        }

        let mut text = String::new();
        for (i, path) in paths_list.iter().enumerate() {
            if i > 0 {
                text.push(' ');
            }
            let path_str = path.to_string_lossy();
            text.push('\'');
            text.push_str(&path_str.replace('\'', "'\\''"));
            text.push('\'');
        }

        let terminal = self.active_terminal();
        if terminal.bracketed_paste_mode() {
            terminal.write(b"\x1b[200~");
            terminal.write(text.as_bytes());
            terminal.write(b"\x1b[201~");
        } else {
            terminal.write(text.as_bytes());
        }
        cx.notify();
    }

    pub(super) fn tab_bar_height(&self) -> f32 {
        if self.show_tab_bar() {
            TABBAR_HEIGHT
        } else {
            0.0
        }
    }

    pub(super) fn titlebar_height(&self) -> f32 {
        #[cfg(target_os = "windows")]
        {
            0.0
        }
        #[cfg(not(target_os = "windows"))]
        {
            TITLEBAR_HEIGHT
        }
    }

    pub(super) fn update_banner_height(&self) -> f32 {
        #[cfg(target_os = "macos")]
        if self.show_update_banner {
            return UPDATE_BANNER_HEIGHT;
        }
        0.0
    }

    pub(super) fn chrome_height(&self) -> f32 {
        self.titlebar_height() + self.tab_bar_height() + self.update_banner_height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_row_maps_scrollback_lines_into_viewport() {
        assert_eq!(TerminalView::viewport_row_from_term_line(-3, 3), Some(0));
        assert_eq!(TerminalView::viewport_row_from_term_line(4, 3), Some(7));
    }

    #[test]
    fn terminal_scroll_lines_track_single_line_steps() {
        let mut accumulated = 0.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 24.0, 24.0, 480.0),
            1
        );
    }

    #[test]
    fn terminal_scroll_lines_accumulate_fractional_pixels() {
        let mut accumulated = 0.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 8.0, 24.0, 480.0),
            0
        );
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 8.0, 24.0, 480.0),
            0
        );
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 8.0, 24.0, 480.0),
            1
        );
    }

    #[test]
    fn terminal_scroll_lines_preserve_sign() {
        let mut accumulated = 0.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, -30.0, 24.0, 480.0),
            -1
        );
    }

    #[test]
    fn terminal_scroll_lines_wrap_accumulator_by_viewport_height() {
        let mut accumulated = 24.0 * 19.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 24.0, 24.0, 480.0),
            1
        );
        assert!(accumulated.abs() < f32::EPSILON);
    }

    #[test]
    fn terminal_scroll_lines_ignore_zero_line_height() {
        let mut accumulated = 12.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 24.0, 0.0, 480.0),
            0
        );
        assert_eq!(accumulated, 12.0);
    }

    #[test]
    fn switch_theme_action_maps_to_theme_palette_mode() {
        assert_eq!(
            TerminalView::command_palette_mode_for_action(CommandAction::SwitchTheme),
            Some(CommandPaletteMode::Themes)
        );
        assert_eq!(
            TerminalView::command_palette_mode_for_action(CommandAction::OpenConfig),
            None
        );
    }
}
