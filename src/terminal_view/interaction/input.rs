use super::*;

fn should_defer_key_down_to_ime(keystroke: &gpui::Keystroke) -> bool {
    let key = keystroke.key.as_str();
    keystroke.key_char.is_some()
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
        && !matches!(
            key,
            "enter" | "tab" | "space" | "backspace" | "escape" | "delete"
        )
}

fn shell_quote_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let mut quoted = String::with_capacity(path_str.len() + 2);
    quoted.push('\'');
    quoted.push_str(&path_str.replace('\'', "'\\''"));
    quoted.push('\'');
    quoted
}

fn shell_quote_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| shell_quote_path(path))
        .collect::<Vec<_>>()
        .join(" ")
}

fn image_extension(format: gpui::ImageFormat) -> &'static str {
    match format {
        gpui::ImageFormat::Gif => "gif",
        gpui::ImageFormat::Png => "png",
        gpui::ImageFormat::Jpeg => "jpg",
        gpui::ImageFormat::Webp => "webp",
        gpui::ImageFormat::Bmp => "bmp",
        gpui::ImageFormat::Tiff => "tiff",
        gpui::ImageFormat::Svg => "svg",
        gpui::ImageFormat::Ico => "ico",
    }
}

fn clipboard_image_cache_dir() -> PathBuf {
    env::temp_dir().join("termy-clipboard-images")
}

fn write_clipboard_image_to_temp_file(image: &gpui::Image) -> std::io::Result<PathBuf> {
    let dir = clipboard_image_cache_dir();
    std::fs::create_dir_all(&dir)?;

    let path = dir.join(format!(
        "clipboard-image-{}.{}",
        image.id(),
        image_extension(image.format())
    ));
    if !path.exists() {
        std::fs::write(&path, image.bytes())?;
    }

    Ok(path)
}

fn clipboard_item_to_terminal_paste_input(
    item: &ClipboardItem,
) -> std::io::Result<Option<Vec<u8>>> {
    if let Some(text) = item.text() {
        return Ok(Some(text.into_bytes()));
    }

    let Some(entry) = item.entries().iter().find(|entry| {
        matches!(
            entry,
            gpui::ClipboardEntry::ExternalPaths(_) | gpui::ClipboardEntry::Image(_)
        )
    }) else {
        return Ok(None);
    };

    match entry {
        gpui::ClipboardEntry::ExternalPaths(paths) => {
            Ok(Some(shell_quote_paths(paths.paths()).into_bytes()))
        }
        gpui::ClipboardEntry::Image(image) => {
            let path = write_clipboard_image_to_temp_file(image)?;
            Ok(Some(shell_quote_path(&path).into_bytes()))
        }
        gpui::ClipboardEntry::String(_) => Ok(None),
    }
}

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
                if self.copy_active_inline_input_selection(cx) {
                    return true;
                }
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
                if let Some(item) = cx.read_from_clipboard() {
                    match clipboard_item_to_terminal_paste_input(&item) {
                        Ok(Some(input)) => {
                            self.write_terminal_paste_input(&input, cx);
                            self.clear_selection();
                            cx.notify();
                        }
                        Ok(None) => self.write_paste_fallback_input(cx),
                        Err(error) => {
                            termy_toast::error(format!(
                                "Failed to prepare clipboard image for paste: {error}"
                            ));
                        }
                    }
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
        let _ = self.close_terminal_context_menu(cx);
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

        // Printable character input without modifiers is delegated to the
        // platform IME / input handler so that CJK input methods work.
        // Named special keys (enter, tab, space, etc.) and modifier
        // combinations are still handled here via keystroke_to_input.
        if should_defer_key_down_to_ime(&event.keystroke) {
            // Let the event propagate to the platform IME handler which
            // will call `replace_text_in_range` on our EntityInputHandler.
            return;
        }

        let prompt_shortcuts_enabled = !self
            .active_terminal()
            .is_some_and(|terminal| terminal.alternate_screen_mode());
        if let Some(input) = keystroke_to_input(&event.keystroke, prompt_shortcuts_enabled) {
            self.write_terminal_input(&input, cx);
            self.clear_selection();
            // Stop propagation so the event does not bubble up to the IME input handler.
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(in super::super) fn handle_file_drop(
        &mut self,
        paths: &ExternalPaths,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.close_terminal_context_menu(cx);
        let paths_list = paths.paths();
        if paths_list.is_empty() {
            return;
        }

        let text = shell_quote_paths(paths_list);
        self.write_terminal_paste_input(text.as_bytes(), cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clipboard_item_to_terminal_paste_input, image_extension, shell_quote_paths,
        should_defer_key_down_to_ime,
    };
    use gpui::{Keystroke, Modifiers};
    use std::path::PathBuf;

    fn keystroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        }
    }

    #[test]
    fn ime_defers_plain_printable_text() {
        assert!(should_defer_key_down_to_ime(&keystroke(
            "a",
            Some("a"),
            Modifiers::default(),
        )));
    }

    #[test]
    fn ime_keeps_shifted_printable_text_on_ime_path() {
        let modifiers = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert!(should_defer_key_down_to_ime(&keystroke(
            "A",
            Some("A"),
            modifiers
        )));
    }

    #[test]
    fn ime_does_not_defer_special_keys_or_modified_shortcuts() {
        for key in &["enter", "tab", "space", "backspace", "escape", "delete"] {
            assert!(
                !should_defer_key_down_to_ime(&keystroke(key, Some(key), Modifiers::default())),
                "{key} should not be deferred to IME"
            );
        }

        let control = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        assert!(!should_defer_key_down_to_ime(&keystroke(
            "c",
            Some("c"),
            control
        )));
    }

    #[test]
    fn ime_does_not_defer_keys_without_key_char() {
        assert!(!should_defer_key_down_to_ime(&keystroke(
            "f1",
            None,
            Modifiers::default(),
        )));
    }

    #[test]
    fn shell_quote_paths_escapes_single_quotes() {
        let paths = vec![
            PathBuf::from("/tmp/normal.png"),
            PathBuf::from("/tmp/quote's test.png"),
        ];

        assert_eq!(
            shell_quote_paths(&paths),
            "'/tmp/normal.png' '/tmp/quote'\\''s test.png'"
        );
    }

    #[test]
    fn clipboard_image_paste_materializes_a_quoted_temp_path() {
        let item = gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            vec![1, 2, 3, 4],
        ));

        let input = clipboard_item_to_terminal_paste_input(&item)
            .expect("clipboard image should serialize")
            .expect("clipboard image should produce paste input");
        let text = String::from_utf8(input).expect("path should be utf8");

        assert!(text.starts_with('\''));
        assert!(text.ends_with(".png'"));
        assert!(text.contains("termy-clipboard-images"));
    }

    #[test]
    fn image_extension_matches_expected_file_suffixes() {
        assert_eq!(image_extension(gpui::ImageFormat::Gif), "gif");
        assert_eq!(image_extension(gpui::ImageFormat::Png), "png");
        assert_eq!(image_extension(gpui::ImageFormat::Jpeg), "jpg");
        assert_eq!(image_extension(gpui::ImageFormat::Webp), "webp");
        assert_eq!(image_extension(gpui::ImageFormat::Bmp), "bmp");
        assert_eq!(image_extension(gpui::ImageFormat::Tiff), "tiff");
        assert_eq!(image_extension(gpui::ImageFormat::Svg), "svg");
        assert_eq!(image_extension(gpui::ImageFormat::Ico), "ico");
    }
}
