use super::*;

impl TerminalView {
    fn prepare_terminal_input_write(&mut self, cx: &mut Context<Self>) {
        self.terminal_scroll_accumulator_y = 0.0;
        self.input_scroll_suppress_until =
            Some(Instant::now() + Duration::from_millis(INPUT_SCROLL_SUPPRESS_MS));
        self.scroll_to_bottom(cx);
    }


    pub(in super::super) fn write_terminal_input(&mut self, input: &[u8], cx: &mut Context<Self>) {
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

    pub(in super::super) fn write_terminal_paste_input(&mut self, input: &[u8], cx: &mut Context<Self>) {
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

    pub(in super::super) fn write_copy_fallback_input(&mut self, _cx: &mut Context<Self>) {
        #[cfg(not(target_os = "macos"))]
        {
            self.write_terminal_input(&[0x03], _cx);
            self.clear_selection();
            _cx.notify();
        }
    }

    pub(in super::super) fn write_paste_fallback_input(&mut self, _cx: &mut Context<Self>) {
        #[cfg(not(target_os = "macos"))]
        {
            self.write_terminal_input(&[0x16], _cx);
            self.clear_selection();
            _cx.notify();
        }
    }

    pub(in super::super) fn execute_input_command_action(
        &mut self,
        action: CommandAction,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            CommandAction::Copy => {
                if let Some(selected) = self.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected));
                } else {
                    self.write_copy_fallback_input(cx);
                }
                true
            }
            CommandAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.write_terminal_paste_input(text.as_bytes(), cx);
                    self.clear_selection();
                    cx.notify();
                } else {
                    self.write_paste_fallback_input(cx);
                }
                true
            }
            _ => false,
        }
    }


    pub(in super::super) fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_cursor_blink_phase();
        let key = event.keystroke.key.as_str();

        if self.is_command_palette_open() {
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

        let prompt_shortcuts_enabled = !self.active_terminal().alternate_screen_mode();
        if let Some(input) = keystroke_to_input(&event.keystroke, prompt_shortcuts_enabled) {
            self.write_terminal_input(&input, cx);
            self.clear_selection();
            // Request a redraw to show the typed character
            cx.notify();
        }
    }

    pub(in super::super) fn handle_file_drop(
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
}
