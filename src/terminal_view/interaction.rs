use super::scrollbar as terminal_scrollbar;
use super::*;
use crate::ui::scrollbar as ui_scrollbar;
use gpui::{AppContext, PromptLevel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuitRequestTarget {
    Application,
    WindowClose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalSelectionCharClass {
    Whitespace,
    Word,
    Other,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstallShell {
    Zsh,
    Bash,
    Fish,
}

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

    fn prepare_terminal_input_write(&mut self, cx: &mut Context<Self>) {
        self.terminal_scroll_accumulator_y = 0.0;
        self.input_scroll_suppress_until =
            Some(Instant::now() + Duration::from_millis(INPUT_SCROLL_SUPPRESS_MS));
        self.scroll_to_bottom(cx);
    }

    fn consume_suppressed_scroll_event(
        &mut self,
        touch_phase: TouchPhase,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(until) = self.input_scroll_suppress_until else {
            return false;
        };

        match touch_phase {
            TouchPhase::Started => {
                self.input_scroll_suppress_until = None;
                false
            }
            TouchPhase::Ended => {
                self.input_scroll_suppress_until = None;
                cx.stop_propagation();
                true
            }
            TouchPhase::Moved => {
                let now = Instant::now();
                // Block residual momentum until we see a clear gesture boundary.
                // Fallback timeout keeps non-touch wheel devices from being blocked.
                let fallback_release = until + Duration::from_millis(INPUT_SCROLL_SUPPRESS_MS * 3);
                if now < fallback_release {
                    cx.stop_propagation();
                    true
                } else {
                    self.input_scroll_suppress_until = None;
                    false
                }
            }
        }
    }

    fn write_terminal_input(&mut self, input: &[u8], cx: &mut Context<Self>) {
        if input.is_empty() {
            return;
        }

        self.prepare_terminal_input_write(cx);
        self.active_terminal().write(input);
    }

    fn sanitize_bracketed_paste_input(input: &[u8]) -> Option<Vec<u8>> {
        const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
        const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";

        let mut sanitized: Option<Vec<u8>> = None;
        let mut index = 0;
        while index < input.len() {
            let remaining = &input[index..];
            let marker_len = if remaining.starts_with(BRACKETED_PASTE_END) {
                Some(BRACKETED_PASTE_END.len())
            } else if remaining.starts_with(BRACKETED_PASTE_START) {
                Some(BRACKETED_PASTE_START.len())
            } else {
                None
            };

            if let Some(marker_len) = marker_len {
                if sanitized.is_none() {
                    let mut buffer = Vec::with_capacity(input.len());
                    buffer.extend_from_slice(&input[..index]);
                    sanitized = Some(buffer);
                }
                index += marker_len;
                continue;
            }

            if let Some(buffer) = sanitized.as_mut() {
                buffer.push(input[index]);
            }
            index += 1;
        }

        sanitized
    }

    fn write_terminal_paste_input(&mut self, input: &[u8], cx: &mut Context<Self>) {
        if input.is_empty() {
            return;
        }

        self.prepare_terminal_input_write(cx);
        let terminal = self.active_terminal();
        if terminal.bracketed_paste_mode() {
            terminal.write(b"\x1b[200~");
            if let Some(sanitized) = Self::sanitize_bracketed_paste_input(input) {
                terminal.write(&sanitized);
            } else {
                terminal.write(input);
            }
            terminal.write(b"\x1b[201~");
        } else {
            terminal.write(input);
        }
    }

    fn write_copy_fallback_input(&mut self, _cx: &mut Context<Self>) {
        #[cfg(not(target_os = "macos"))]
        {
            self.write_terminal_input(&[0x03], _cx);
            self.clear_selection();
            _cx.notify();
        }
    }

    fn write_paste_fallback_input(&mut self, _cx: &mut Context<Self>) {
        #[cfg(not(target_os = "macos"))]
        {
            self.write_terminal_input(&[0x16], _cx);
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

    fn terminal_selection_char_class(c: char) -> TerminalSelectionCharClass {
        if c.is_whitespace() {
            TerminalSelectionCharClass::Whitespace
        } else if c.is_alphanumeric() || c == '_' {
            TerminalSelectionCharClass::Word
        } else {
            TerminalSelectionCharClass::Other
        }
    }

    fn select_token_at_cell(&mut self, cell: CellPos) -> bool {
        let Some(line) = self.row_text(cell.row) else {
            return false;
        };
        if cell.col >= line.len() {
            return false;
        }

        let class = Self::terminal_selection_char_class(line[cell.col]);
        if class == TerminalSelectionCharClass::Whitespace {
            let Some(last_non_whitespace) = line.iter().rposition(|c| !c.is_whitespace()) else {
                return false;
            };
            if cell.col > last_non_whitespace {
                return false;
            }
        }

        let mut start_col = cell.col;
        while start_col > 0 && Self::terminal_selection_char_class(line[start_col - 1]) == class {
            start_col -= 1;
        }

        let mut end_col = cell.col;
        while end_col + 1 < line.len()
            && Self::terminal_selection_char_class(line[end_col + 1]) == class
        {
            end_col += 1;
        }

        self.selection_anchor = Some(CellPos {
            col: start_col,
            row: cell.row,
        });
        self.selection_head = Some(CellPos {
            col: end_col,
            row: cell.row,
        });
        self.selection_dragging = false;
        self.selection_moved = true;
        true
    }

    fn select_line_at_row(&mut self, row: usize) -> bool {
        let size = self.active_terminal().size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        if cols == 0 || row >= rows {
            return false;
        }

        self.selection_anchor = Some(CellPos { col: 0, row });
        self.selection_head = Some(CellPos {
            col: cols.saturating_sub(1),
            row,
        });
        self.selection_dragging = false;
        self.selection_moved = true;
        true
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
        self.clear_tab_title_width_cache();
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

    fn busy_tab_titles_for_quit(&self) -> Vec<String> {
        let fallback_title = self.fallback_title();
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.running_process || tab.terminal.alternate_screen_mode())
            .map(|(index, tab)| {
                let title = tab.title.trim();
                if title.is_empty() {
                    format!("{fallback_title} {}", index + 1)
                } else {
                    title.to_string()
                }
            })
            .collect()
    }

    fn quit_warning_detail(&self, busy_titles: &[String]) -> String {
        let count = busy_titles.len();
        let mut detail = format!(
            "{} tab{} {} running a command or fullscreen terminal app:\n",
            count,
            if count == 1 { "" } else { "s" },
            if count == 1 { "has" } else { "have" },
        );

        for title in busy_titles {
            detail.push_str("- ");
            detail.push_str(title);
            detail.push('\n');
        }

        detail.push_str("\nQuit anyway?");
        detail
    }

    fn request_quit(
        &mut self,
        target: QuitRequestTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.quit_prompt_in_flight {
            return false;
        }

        let busy_titles = self.busy_tab_titles_for_quit();
        if !self.warn_on_quit_with_running_process || busy_titles.is_empty() {
            if target == QuitRequestTarget::Application {
                self.allow_quit_without_prompt = true;
                cx.quit();
                return false;
            }
            return true;
        }

        self.quit_prompt_in_flight = true;
        let detail = self.quit_warning_detail(&busy_titles);
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Quit Termy?",
            Some(&detail),
            &["Quit", "Cancel"],
            cx,
        );
        let window_handle = window.window_handle();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let confirmed = matches!(prompt.await, Ok(0));
            let _ = cx.update(|cx| {
                let mut follow_through = false;
                if this
                    .update(cx, |view, _| {
                        view.quit_prompt_in_flight = false;
                        if confirmed {
                            view.allow_quit_without_prompt = true;
                            follow_through = true;
                        }
                    })
                    .is_err()
                {
                    return;
                }

                if !follow_through {
                    return;
                }

                match target {
                    QuitRequestTarget::Application => cx.quit(),
                    QuitRequestTarget::WindowClose => {
                        let _ = window_handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            });
        })
        .detach();

        false
    }

    pub(crate) fn handle_window_should_close_request(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.allow_quit_without_prompt {
            self.allow_quit_without_prompt = false;
            return true;
        }

        self.request_quit(QuitRequestTarget::WindowClose, window, cx)
    }

    pub(super) fn execute_command_action(
        &mut self,
        action: CommandAction,
        respect_shortcut_suspend: bool,
        window: &mut Window,
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
            CommandAction::Quit => {
                self.request_quit(QuitRequestTarget::Application, window, cx);
            }
            _ if shortcuts_suspended => {}
            CommandAction::OpenConfig => {
                if let Err(error) = config::open_config_file() {
                    log::error!("Failed to open config file from command action: {}", error);
                    termy_toast::error(error.to_string());
                    cx.notify();
                }
            }
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
                Ok(()) => {
                    self.allow_quit_without_prompt = true;
                    cx.quit();
                }
                Err(error) => {
                    termy_toast::error(format!("Restart failed: {}", error));
                    cx.notify();
                }
            },
            CommandAction::RenameTab => {
                self.begin_rename_tab(self.active_tab, cx);
                termy_toast::info("Rename mode enabled");
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
            CommandAction::MoveTabLeft => {
                self.move_active_tab_left(cx);
            }
            CommandAction::MoveTabRight => {
                self.move_active_tab_right(cx);
            }
            CommandAction::SwitchTabLeft => {
                self.switch_active_tab_left(cx);
            }
            CommandAction::SwitchTabRight => {
                self.switch_active_tab_right(cx);
            }
            CommandAction::MinimizeWindow => {}
            CommandAction::Copy => {
                if let Some(selected) = self.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected));
                } else {
                    self.write_copy_fallback_input(cx);
                }
            }
            CommandAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.write_terminal_paste_input(text.as_bytes(), cx);
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
            CommandAction::InstallCli => {
                self.install_cli_action(cx);
            }
        }
    }

    fn install_cli_action(&mut self, cx: &mut Context<Self>) {
        match Self::install_cli_binary() {
            Ok(path) => {
                let path_str = path.display().to_string();
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                {
                    match self.configure_install_cli_shell_path(&path, cx) {
                        Ok((profile_path, profile_updated)) => {
                            if profile_updated {
                                termy_toast::success(format!(
                                    "CLI installed to {}. Updated {} and activated PATH in this shell.",
                                    path_str,
                                    profile_path.display()
                                ));
                            } else {
                                termy_toast::success(format!(
                                    "CLI installed to {}. {} already configures Termy PATH; activated PATH in this shell.",
                                    path_str,
                                    profile_path.display()
                                ));
                            }
                        }
                        Err(error) => {
                            termy_toast::error(format!(
                                "CLI installed to {} but automated PATH setup failed: {}",
                                path_str, error
                            ));
                        }
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    if let Some(parent) = path.parent() {
                        termy_toast::success(format!(
                            "CLI installed to {}. Add {} to PATH: setx PATH \"%PATH%;{}\"",
                            path_str,
                            parent.display(),
                            parent.display()
                        ));
                    } else {
                        termy_toast::success(format!("CLI installed to {}", path_str));
                    }
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
                {
                    termy_toast::success(format!("CLI installed to {}", path_str));
                }
                cx.notify();
            }
            Err(e) => {
                termy_toast::error(format!("Failed to install CLI: {}", e));
                cx.notify();
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn configure_install_cli_shell_path(
        &mut self,
        install_path: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> Result<(std::path::PathBuf, bool), String> {
        let install_dir = install_path.parent().ok_or_else(|| {
            format!(
                "Installed CLI path {} does not have a parent directory",
                install_path.display()
            )
        })?;
        let install_dir = install_dir.to_string_lossy().into_owned();
        let shell = self.install_cli_shell()?;
        let profile_path = Self::install_cli_profile_path(shell)?;
        let block = Self::install_cli_profile_block(shell, &install_dir);
        let profile_updated = Self::ensure_install_cli_profile_block(&profile_path, &block)?;
        let session_command = Self::install_cli_session_command(shell, &install_dir);
        self.write_terminal_input(format!("{session_command}\n").as_bytes(), cx);
        Ok((profile_path, profile_updated))
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn install_cli_shell(&self) -> Result<InstallShell, String> {
        let env_shell = std::env::var("SHELL").ok();
        Self::resolve_install_cli_shell(self.terminal_runtime.shell.as_deref(), env_shell.as_deref())
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn resolve_install_cli_shell(
        configured_shell: Option<&str>,
        env_shell: Option<&str>,
    ) -> Result<InstallShell, String> {
        let candidate = configured_shell
            .map(str::trim)
            .filter(|shell| !shell.is_empty())
            .or_else(|| env_shell.map(str::trim).filter(|shell| !shell.is_empty()))
            .unwrap_or(Self::default_install_cli_shell_path());

        Self::parse_install_cli_shell(candidate).ok_or_else(|| {
            format!(
                "Unsupported shell '{}' for automated PATH setup. Supported shells: zsh, bash, fish.",
                candidate
            )
        })
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn default_install_cli_shell_path() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "/bin/zsh"
        }

        #[cfg(target_os = "linux")]
        {
            "/bin/bash"
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn parse_install_cli_shell(shell: &str) -> Option<InstallShell> {
        let shell = shell.trim().trim_matches('"').trim_matches('\'');
        if shell.is_empty() {
            return None;
        }

        let shell_program = shell.split_whitespace().next()?;
        let shell_name = std::path::Path::new(shell_program)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(shell_program);

        match shell_name {
            "zsh" => Some(InstallShell::Zsh),
            "bash" => Some(InstallShell::Bash),
            "fish" => Some(InstallShell::Fish),
            _ => None,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn install_cli_profile_path(shell: InstallShell) -> Result<std::path::PathBuf, String> {
        let home = dirs::home_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
        Ok(home.join(Self::install_cli_profile_relative_path(shell)))
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn install_cli_profile_relative_path(shell: InstallShell) -> &'static str {
        match shell {
            InstallShell::Zsh => ".zshrc",
            InstallShell::Bash => {
                if cfg!(target_os = "macos") {
                    ".bash_profile"
                } else {
                    ".bashrc"
                }
            }
            InstallShell::Fish => ".config/fish/config.fish",
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn install_cli_profile_block(shell: InstallShell, install_dir: &str) -> String {
        const START: &str = "# >>> termy cli path >>>";
        const END: &str = "# <<< termy cli path <<<";

        match shell {
            InstallShell::Zsh | InstallShell::Bash => format!(
                "{START}\n# Added by Termy Install CLI\nTERMY_CLI_PATH={}\ncase \":$PATH:\" in\n  *\":$TERMY_CLI_PATH:\"*) ;;\n  *) export PATH=\"$TERMY_CLI_PATH:$PATH\" ;;\nesac\n{END}",
                Self::single_quote_shell_value(install_dir)
            ),
            InstallShell::Fish => format!(
                "{START}\n# Added by Termy Install CLI\nset -l termy_cli_path {}\nif not contains -- $termy_cli_path $PATH\n    set -gx PATH $termy_cli_path $PATH\nend\n{END}",
                Self::double_quote_fish_value(install_dir)
            ),
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn install_cli_session_command(shell: InstallShell, install_dir: &str) -> String {
        match shell {
            InstallShell::Zsh | InstallShell::Bash => format!(
                "TERMY_CLI_PATH={}; case \":$PATH:\" in *\":$TERMY_CLI_PATH:\"*) ;; *) export PATH=\"$TERMY_CLI_PATH:$PATH\" ;; esac",
                Self::single_quote_shell_value(install_dir)
            ),
            InstallShell::Fish => format!(
                "set -l termy_cli_path {}; if not contains -- $termy_cli_path $PATH; set -gx PATH $termy_cli_path $PATH; end",
                Self::double_quote_fish_value(install_dir)
            ),
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn ensure_install_cli_profile_block(
        profile_path: &std::path::Path,
        block: &str,
    ) -> Result<bool, String> {
        const START: &str = "# >>> termy cli path >>>";

        if let Some(parent) = profile_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("Failed to create shell config directory {}: {}", parent.display(), e)
            })?;
        }

        let existing = match std::fs::read_to_string(profile_path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(format!(
                    "Failed to read shell config {}: {}",
                    profile_path.display(),
                    error
                ));
            }
        };

        let Some(updated) = Self::append_install_cli_profile_block_if_missing(&existing, START, block)
        else {
            return Ok(false);
        };

        std::fs::write(profile_path, updated).map_err(|error| {
            format!(
                "Failed to write shell config {}: {}",
                profile_path.display(),
                error
            )
        })?;
        Ok(true)
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn append_install_cli_profile_block_if_missing(
        existing: &str,
        marker: &str,
        block: &str,
    ) -> Option<String> {
        if existing.contains(marker) {
            return None;
        }

        let mut updated = existing.to_string();
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(block);
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        Some(updated)
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn single_quote_shell_value(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn double_quote_fish_value(value: &str) -> String {
        format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
        )
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn install_cli_binary() -> Result<std::path::PathBuf, String> {
        use std::os::unix::fs::symlink;
        use std::path::PathBuf;

        // Get the CLI binary path from the app bundle or build directory
        let cli_source = Self::find_cli_binary()?;
        let cli_source = Self::absolute_install_cli_source_path(&cli_source)?;

        // Try ~/.local/bin first (user-writable), fall back to /usr/local/bin
        let home_bin = dirs::home_dir().map(|h| h.join(".local").join("bin").join("termy"));

        let using_fallback = home_bin.is_none();
        let target = if let Some(ref local_bin) = home_bin {
            // Try user's local bin first
            local_bin.clone()
        } else {
            // Fall back to system-wide location
            PathBuf::from("/usr/local/bin/termy")
        };

        // Create parent directory if needed
        if let Some(parent) = target.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    if using_fallback && e.kind() == std::io::ErrorKind::PermissionDenied {
                        format!(
                            "Failed to create {}: {}. \
                            $HOME is not set, so fell back to system path. \
                            Either: set $HOME and retry (to use ~/.local/bin), \
                            run with elevated privileges (sudo), \
                            or manually create {} with appropriate permissions.",
                            parent.display(),
                            e,
                            parent.display()
                        )
                    } else {
                        format!("Failed to create directory {}: {}", parent.display(), e)
                    }
                })?;
            }
        }

        // Check if target already exists
        if target.exists() || target.symlink_metadata().is_ok() {
            // Remove existing symlink or file
            std::fs::remove_file(&target).map_err(|e| {
                format!(
                    "Failed to remove existing file at {}: {}",
                    target.display(),
                    e
                )
            })?;
        }

        // Create symlink
        symlink(&cli_source, &target)
            .map_err(|e| format!("Failed to create symlink at {}: {}", target.display(), e))?;

        Ok(target)
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn absolute_install_cli_source_path(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }

        let cwd = std::env::current_dir()
            .map_err(|e| format!("Failed to resolve current directory: {}", e))?;
        Ok(cwd.join(path))
    }

    #[cfg(target_os = "windows")]
    fn install_cli_binary() -> Result<std::path::PathBuf, String> {
        use std::path::PathBuf;

        let cli_source = Self::find_cli_binary()?;

        // On Windows, copy to a location in PATH or next to the app
        let target = if let Some(local_app_data) = dirs::data_local_dir() {
            local_app_data.join("Termy").join("bin").join("termy.exe")
        } else {
            return Err("Could not determine local app data directory".to_string());
        };

        // Create parent directory if needed
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        // Copy the binary
        std::fs::copy(&cli_source, &target)
            .map_err(|e| format!("Failed to copy CLI binary: {}", e))?;

        Ok(target)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    fn install_cli_binary() -> Result<std::path::PathBuf, String> {
        Err("CLI installation is not supported on this platform".to_string())
    }

    fn find_cli_binary() -> Result<std::path::PathBuf, String> {
        use std::path::PathBuf;

        // First, try to find CLI next to the current executable
        if let Ok(exe_path) = std::env::current_exe() {
            let exe_dir = exe_path
                .parent()
                .ok_or("Failed to get executable directory")?;

            // Check for CLI binary in same directory (named termy-cli)
            #[cfg(target_os = "windows")]
            let cli_name = "termy-cli.exe";
            #[cfg(not(target_os = "windows"))]
            let cli_name = "termy-cli";

            let cli_path = exe_dir.join(cli_name);
            if cli_path.exists() {
                return Ok(cli_path);
            }

            // On macOS, check inside the app bundle
            #[cfg(target_os = "macos")]
            {
                if exe_dir.ends_with("Contents/MacOS") {
                    let bundle_cli = exe_dir.join("termy-cli");
                    if bundle_cli.exists() {
                        return Ok(bundle_cli);
                    }
                }
            }
        }

        // Check common build output locations
        let possible_paths = [
            PathBuf::from("./target/release/termy-cli"),
            PathBuf::from("./target/debug/termy-cli"),
        ];

        for path in &possible_paths {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        Err(
            "CLI binary not found. Make sure to build it with: cargo build -p termy_cli"
                .to_string(),
        )
    }

    pub(super) fn handle_toggle_command_palette_action(
        &mut self,
        _: &commands::ToggleCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ToggleCommandPalette, true, window, cx);
    }

    pub(super) fn handle_import_colors_action(
        &mut self,
        _: &commands::ImportColors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ImportColors, true, window, cx);
    }

    pub(super) fn handle_switch_theme_action(
        &mut self,
        _: &commands::SwitchTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SwitchTheme, true, window, cx);
    }

    pub(super) fn handle_app_info_action(
        &mut self,
        _: &commands::AppInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::AppInfo, true, window, cx);
    }

    pub(super) fn handle_native_sdk_example_action(
        &mut self,
        _: &commands::NativeSdkExample,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::NativeSdkExample, true, window, cx);
    }

    pub(super) fn handle_restart_app_action(
        &mut self,
        _: &commands::RestartApp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::RestartApp, true, window, cx);
    }

    pub(super) fn handle_rename_tab_action(
        &mut self,
        _: &commands::RenameTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::RenameTab, true, window, cx);
    }

    pub(super) fn handle_check_for_updates_action(
        &mut self,
        _: &commands::CheckForUpdates,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::CheckForUpdates, true, window, cx);
    }

    pub(super) fn handle_new_tab_action(
        &mut self,
        _: &commands::NewTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::NewTab, true, window, cx);
    }

    pub(super) fn handle_close_tab_action(
        &mut self,
        _: &commands::CloseTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::CloseTab, true, window, cx);
    }

    pub(super) fn handle_move_tab_left_action(
        &mut self,
        _: &commands::MoveTabLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::MoveTabLeft, true, window, cx);
    }

    pub(super) fn handle_move_tab_right_action(
        &mut self,
        _: &commands::MoveTabRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::MoveTabRight, true, window, cx);
    }

    pub(super) fn handle_switch_tab_left_action(
        &mut self,
        _: &commands::SwitchTabLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SwitchTabLeft, true, window, cx);
    }

    pub(super) fn handle_switch_tab_right_action(
        &mut self,
        _: &commands::SwitchTabRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SwitchTabRight, true, window, cx);
    }

    pub(super) fn handle_minimize_window_action(
        &mut self,
        _: &commands::MinimizeWindow,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        window.minimize_window();
    }

    pub(super) fn handle_copy_action(
        &mut self,
        _: &commands::Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::Copy, true, window, cx);
    }

    pub(super) fn handle_paste_action(
        &mut self,
        _: &commands::Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::Paste, true, window, cx);
    }

    pub(super) fn handle_zoom_in_action(
        &mut self,
        _: &commands::ZoomIn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ZoomIn, true, window, cx);
    }

    pub(super) fn handle_zoom_out_action(
        &mut self,
        _: &commands::ZoomOut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ZoomOut, true, window, cx);
    }

    pub(super) fn handle_zoom_reset_action(
        &mut self,
        _: &commands::ZoomReset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ZoomReset, true, window, cx);
    }

    pub(super) fn handle_quit_action(
        &mut self,
        _: &commands::Quit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::Quit, true, window, cx);
    }

    pub(super) fn handle_open_search_action(
        &mut self,
        _: &commands::OpenSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::OpenSearch, true, window, cx);
    }

    pub(super) fn handle_close_search_action(
        &mut self,
        _: &commands::CloseSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::CloseSearch, true, window, cx);
    }

    pub(super) fn handle_search_next_action(
        &mut self,
        _: &commands::SearchNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SearchNext, true, window, cx);
    }

    pub(super) fn handle_search_previous_action(
        &mut self,
        _: &commands::SearchPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::SearchPrevious, true, window, cx);
    }

    pub(super) fn handle_toggle_search_case_sensitive_action(
        &mut self,
        _: &commands::ToggleSearchCaseSensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ToggleSearchCaseSensitive, true, window, cx);
    }

    pub(super) fn handle_toggle_search_regex_action(
        &mut self,
        _: &commands::ToggleSearchRegex,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::ToggleSearchRegex, true, window, cx);
    }

    pub(super) fn handle_install_cli_action(
        &mut self,
        _: &commands::InstallCli,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.execute_command_action(CommandAction::InstallCli, true, window, cx);
    }

    pub(super) fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_cursor_blink_phase();
        let key = event.keystroke.key.as_str();

        if self.command_palette_open {
            self.handle_command_palette_key_down(key, window, cx);
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
            self.write_terminal_input(&input, cx);
            self.clear_selection();
            // Request a redraw to show the typed character
            cx.notify();
        }
    }

    fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        if self.active_terminal().scroll_to_bottom() {
            self.mark_terminal_scrollbar_activity(cx);
            cx.notify();
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
        let mut changed = false;
        if event.button == MouseButton::Left && self.tab_strip.drag.is_some() {
            self.commit_tab_drag(cx);
        } else if self.reset_tab_drag_state() {
            changed = true;
        }
        if self.clear_tab_hover_state() {
            changed = true;
        }
        if changed {
            cx.notify();
        }

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

        if event.click_count >= 3 {
            if self.select_line_at_row(cell.row) {
                self.clear_hovered_link();
                cx.notify();
                return;
            }
        }

        if event.click_count == 2 {
            if self.select_token_at_cell(cell) {
                self.clear_hovered_link();
                cx.notify();
                return;
            }
        }

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
        if self.tab_strip.drag.is_some() && !event.dragging() {
            self.commit_tab_drag(cx);
        }

        if self.clear_tab_hover_state() {
            cx.notify();
        }

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
                let hover_cell = self.position_to_cell(event.position, false);
                if let (Some(cell), Some(current)) = (hover_cell, self.hovered_link.as_ref()) {
                    if current.row == cell.row
                        && (current.start_col..=current.end_col).contains(&cell.col)
                    {
                        return;
                    }
                }

                let next = hover_cell.and_then(|cell| self.link_at_cell(cell));
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

    pub(super) fn handle_terminal_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.consume_suppressed_scroll_event(event.touch_phase, cx) {
            return;
        }

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

        self.write_terminal_paste_input(text.as_bytes(), cx);
        cx.notify();
    }

    pub(super) const fn titlebar_height() -> f32 {
        if TITLEBAR_HEIGHT > TABBAR_HEIGHT {
            TITLEBAR_HEIGHT
        } else {
            TABBAR_HEIGHT
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
        Self::titlebar_height() + self.update_banner_height()
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

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_parse_shell_detects_supported_shells() {
        assert_eq!(
            TerminalView::parse_install_cli_shell("/bin/zsh"),
            Some(InstallShell::Zsh)
        );
        assert_eq!(
            TerminalView::parse_install_cli_shell("\"/bin/bash\""),
            Some(InstallShell::Bash)
        );
        assert_eq!(
            TerminalView::parse_install_cli_shell("/opt/homebrew/bin/fish"),
            Some(InstallShell::Fish)
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_resolve_shell_uses_source_order() {
        assert_eq!(
            TerminalView::resolve_install_cli_shell(Some("/bin/fish"), Some("/bin/zsh")).unwrap(),
            InstallShell::Fish
        );
        assert_eq!(
            TerminalView::resolve_install_cli_shell(Some("   "), Some("/bin/zsh")).unwrap(),
            InstallShell::Zsh
        );
        assert_eq!(
            TerminalView::resolve_install_cli_shell(None, Some("/bin/bash")).unwrap(),
            InstallShell::Bash
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_resolve_shell_errors_for_unsupported_configured_shell() {
        let error =
            TerminalView::resolve_install_cli_shell(Some("/bin/tcsh"), Some("/bin/zsh"))
                .unwrap_err();
        assert!(error.contains("Unsupported shell '/bin/tcsh'"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_resolve_shell_defaults_when_sources_missing() {
        let shell = TerminalView::resolve_install_cli_shell(None, None).unwrap();
        #[cfg(target_os = "macos")]
        assert_eq!(shell, InstallShell::Zsh);
        #[cfg(target_os = "linux")]
        assert_eq!(shell, InstallShell::Bash);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_profile_relative_paths_match_shell() {
        assert_eq!(
            TerminalView::install_cli_profile_relative_path(InstallShell::Zsh),
            ".zshrc"
        );
        assert_eq!(
            TerminalView::install_cli_profile_relative_path(InstallShell::Fish),
            ".config/fish/config.fish"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            TerminalView::install_cli_profile_relative_path(InstallShell::Bash),
            ".bash_profile"
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            TerminalView::install_cli_profile_relative_path(InstallShell::Bash),
            ".bashrc"
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_profile_append_is_idempotent() {
        let marker = "# >>> termy cli path >>>";
        let block = TerminalView::install_cli_profile_block(InstallShell::Zsh, "/tmp/bin");
        let once =
            TerminalView::append_install_cli_profile_block_if_missing("", marker, &block).unwrap();
        assert!(once.contains(marker));
        assert!(
            TerminalView::append_install_cli_profile_block_if_missing(&once, marker, &block)
                .is_none()
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_profile_blocks_include_guarded_path_logic() {
        let sh_block = TerminalView::install_cli_profile_block(InstallShell::Bash, "/tmp/bin");
        assert!(sh_block.contains("case \":$PATH:\" in"));
        assert!(sh_block.contains("export PATH=\"$TERMY_CLI_PATH:$PATH\""));

        let fish_block = TerminalView::install_cli_profile_block(InstallShell::Fish, "/tmp/bin");
        assert!(fish_block.contains("if not contains -- $termy_cli_path $PATH"));
        assert!(fish_block.contains("set -gx PATH $termy_cli_path $PATH"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_session_commands_match_shell_syntax() {
        let sh_command = TerminalView::install_cli_session_command(InstallShell::Zsh, "/tmp/my bin");
        assert!(sh_command.contains("TERMY_CLI_PATH='/tmp/my bin'"));
        assert!(sh_command.contains("case \":$PATH:\" in"));

        let fish_command =
            TerminalView::install_cli_session_command(InstallShell::Fish, "/tmp/my bin");
        assert!(fish_command.contains("set -l termy_cli_path \"/tmp/my bin\""));
        assert!(fish_command.contains("if not contains -- $termy_cli_path $PATH"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_shell_value_escaping_handles_quotes() {
        assert_eq!(
            TerminalView::single_quote_shell_value("a'b"),
            "'a'\\''b'"
        );
        assert_eq!(
            TerminalView::double_quote_fish_value("a\"b$c"),
            "\"a\\\"b\\$c\""
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn install_cli_source_path_is_absolutized_for_relative_paths() {
        let rel = std::path::Path::new("target/debug/termy-cli");
        let abs = TerminalView::absolute_install_cli_source_path(rel).unwrap();
        assert!(abs.is_absolute());
        assert!(abs.ends_with(rel));
    }
}
