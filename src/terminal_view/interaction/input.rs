use super::*;

impl TerminalView {
    fn maybe_suppress_tab_switch_hint_for_key_down(
        &mut self,
        key: &str,
        modifiers: gpui::Modifiers,
        cx: &mut Context<Self>,
    ) {
        if self.tab_strip.switch_hints.suppress_for_key_down(
            key,
            modifiers,
            self.tab_switch_hints_blocked(),
            Instant::now(),
        ) {
            cx.notify();
        }
    }

    pub(in super::super) fn handle_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .tab_strip
            .switch_hints
            .handle_modifiers_changed(event.modifiers, Instant::now())
        {
            cx.notify();
        }
    }

    fn send_input_to_active_pane(&self, input: &[u8]) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_send_input_to_active_pane(input),
            RuntimeKind::Native => {
                let Some(terminal) = self.active_terminal() else {
                    return false;
                };
                terminal.write_input(input);
                true
            }
        }
    }

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
        if self.send_input_to_active_pane(input) && self.runtime_kind() == RuntimeKind::Tmux {
            self.schedule_tmux_title_refresh();
        }
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

    fn framed_bracketed_paste_input(input: &[u8]) -> Vec<u8> {
        const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
        const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";

        let sanitized = Self::sanitize_bracketed_paste_input(input);
        let payload = sanitized.as_deref().unwrap_or(input);

        // Send one framed payload so start/content/end ordering is atomic and
        // tmux can pick an efficient high-volume path for large pastes.
        let mut framed = Vec::with_capacity(
            BRACKETED_PASTE_START.len() + payload.len() + BRACKETED_PASTE_END.len(),
        );
        framed.extend_from_slice(BRACKETED_PASTE_START);
        framed.extend_from_slice(payload);
        framed.extend_from_slice(BRACKETED_PASTE_END);
        framed
    }

    pub(in super::super) fn write_terminal_paste_input(
        &mut self,
        input: &[u8],
        cx: &mut Context<Self>,
    ) {
        if input.is_empty() {
            return;
        }

        self.prepare_terminal_input_write(cx);
        let bracketed_paste = self
            .active_terminal()
            .is_some_and(|terminal| terminal.bracketed_paste_mode());
        let wrote_input = if bracketed_paste {
            let framed = Self::framed_bracketed_paste_input(input);
            self.send_input_to_active_pane(&framed)
        } else {
            self.send_input_to_active_pane(input)
        };

        if wrote_input && self.runtime_kind() == RuntimeKind::Tmux {
            self.schedule_tmux_title_refresh();
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
                if self.paste_clipboard_into_active_inline_input(cx) {
                    return true;
                }
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
        self.maybe_suppress_tab_switch_hint_for_key_down(key, event.keystroke.modifiers, cx);

        if self.is_command_palette_open() {
            self.handle_command_palette_key_down(key, window, cx);
            return;
        }

        if self.search_open {
            self.handle_search_key_down(key, cx);
            return;
        }

        if self.ai_input_open {
            self.handle_ai_input_key_down(key, cx);
            return;
        }

        if self.agent_sidebar_input_active {
            self.handle_agent_sidebar_key_down(key, cx);
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

        let prompt_shortcuts_enabled = !self
            .active_terminal()
            .is_some_and(|terminal| terminal.alternate_screen_mode());
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
