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

fn dropped_paths_to_terminal_paste_input(paths: &[PathBuf]) -> Option<Vec<u8>> {
    if paths.is_empty() {
        return None;
    }

    let mut text = shell_quote_paths(paths);
    text.push(' ');
    Some(text.into_bytes())
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

fn synthetic_modifier_keystroke(key: &str, modifiers: gpui::Modifiers) -> gpui::Keystroke {
    gpui::Keystroke {
        modifiers,
        key: key.to_string(),
        key_char: None,
    }
}

fn modifier_transition_events(
    previous: gpui::Modifiers,
    current: gpui::Modifiers,
) -> Vec<(gpui::Keystroke, TerminalKeyEventKind)> {
    // GPUI surfaces pure modifier transitions separately from key presses, so
    // synthesize terminal key events here when enhanced keyboard reporting is active.
    let mut events = Vec::with_capacity(4);

    for (key, was_pressed, is_pressed) in [
        ("control", previous.control, current.control),
        ("alt", previous.alt, current.alt),
        ("shift", previous.shift, current.shift),
        ("super", previous.platform, current.platform),
    ] {
        if was_pressed && !is_pressed {
            events.push((
                synthetic_modifier_keystroke(key, current),
                TerminalKeyEventKind::Release,
            ));
        }
    }

    for (key, was_pressed, is_pressed) in [
        ("control", previous.control, current.control),
        ("alt", previous.alt, current.alt),
        ("shift", previous.shift, current.shift),
        ("super", previous.platform, current.platform),
    ] {
        if !was_pressed && is_pressed {
            events.push((
                synthetic_modifier_keystroke(key, current),
                TerminalKeyEventKind::Press,
            ));
        }
    }

    events
}

fn overlay_owns_terminal_input_state(
    command_palette_open: bool,
    search_open: bool,
    agent_sidebar_search_active: bool,
    renaming_tab: Option<usize>,
    renaming_agent_thread_id: Option<&str>,
) -> bool {
    command_palette_open
        || search_open
        || agent_sidebar_search_active
        || renaming_tab.is_some()
        || renaming_agent_thread_id.is_some()
}

fn terminal_modifier_transition_events(
    previous: gpui::Modifiers,
    current: gpui::Modifiers,
    overlay_owns_terminal_input: bool,
) -> Vec<(gpui::Keystroke, TerminalKeyEventKind)> {
    if overlay_owns_terminal_input {
        return Vec::new();
    }

    modifier_transition_events(previous, current)
}

fn should_prepare_terminal_input_write(active_pane_id: Option<&str>, pane_id: &str) -> bool {
    active_pane_id == Some(pane_id)
}

fn take_deferred_ime_key_release(
    deferred_ime_key_releases: &mut HashSet<String>,
    key: &str,
) -> bool {
    deferred_ime_key_releases.remove(key)
}

#[derive(Debug, PartialEq, Eq)]
enum PendingKeyReleaseAction {
    Drop,
    ForwardToPane(String),
    FallbackToActivePane,
}

fn take_pending_key_release_action(
    pending_key_releases: &mut HashMap<String, PendingKeyRelease>,
    key: &str,
) -> PendingKeyReleaseAction {
    match pending_key_releases.remove(key) {
        Some(PendingKeyRelease::Consumed) => PendingKeyReleaseAction::Drop,
        Some(PendingKeyRelease::Terminal { pane_id }) => {
            PendingKeyReleaseAction::ForwardToPane(pane_id)
        }
        None => PendingKeyReleaseAction::FallbackToActivePane,
    }
}

impl TerminalView {
    fn overlay_owns_terminal_input(&self) -> bool {
        overlay_owns_terminal_input_state(
            self.is_command_palette_open(),
            self.search_open,
            self.agent_sidebar_search_active,
            self.renaming_tab,
            self.renaming_agent_thread_id.as_deref(),
        )
    }

    fn pane_keyboard_mode(&self, pane_id: &str) -> TerminalKeyboardMode {
        self.pane_terminal_by_id(pane_id)
            .map(Terminal::keyboard_mode)
            .unwrap_or_default()
    }

    fn prompt_shortcuts_enabled_for_pane(&self, pane_id: &str) -> bool {
        !self
            .pane_terminal_by_id(pane_id)
            .is_some_and(|terminal| terminal.alternate_screen_mode())
    }

    fn send_input_to_pane(&self, pane_id: &str, input: &[u8]) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_send_input_to_pane(pane_id, input),
            RuntimeKind::Native => {
                let Some(terminal) = self.pane_terminal_by_id(pane_id) else {
                    return false;
                };
                terminal.write_input(input);
                true
            }
        }
    }

    fn send_input_to_active_pane(&self, input: &[u8]) -> bool {
        let Some(pane_id) = self.active_pane_id() else {
            return false;
        };

        self.send_input_to_pane(pane_id, input)
    }

    fn write_terminal_keystroke_to_pane(
        &mut self,
        pane_id: &str,
        keystroke: &gpui::Keystroke,
        event_kind: TerminalKeyEventKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = keystroke_to_input(
            keystroke,
            event_kind,
            self.pane_keyboard_mode(pane_id),
            self.prompt_shortcuts_enabled_for_pane(pane_id),
        ) else {
            return false;
        };

        self.write_terminal_input_to_pane(pane_id, &input, cx);
        true
    }

    fn write_terminal_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        event_kind: TerminalKeyEventKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane_id) = self.active_pane_id().map(str::to_owned) else {
            return false;
        };

        self.write_terminal_keystroke_to_pane(pane_id.as_str(), keystroke, event_kind, cx)
    }

    fn remember_consumed_key_release(&mut self, key: &str) {
        self.pending_key_releases
            .insert(key.to_string(), PendingKeyRelease::Consumed);
    }

    fn remember_terminal_key_release(&mut self, key: &str, pane_id: String) {
        self.pending_key_releases
            .insert(key.to_string(), PendingKeyRelease::Terminal { pane_id });
    }

    fn write_forwarded_terminal_key_event(
        &mut self,
        keystroke: &gpui::Keystroke,
        event_kind: TerminalKeyEventKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane_id) = self.active_pane_id().map(str::to_owned) else {
            return false;
        };

        if !self.write_terminal_keystroke_to_pane(pane_id.as_str(), keystroke, event_kind, cx) {
            return false;
        }

        if matches!(event_kind, TerminalKeyEventKind::Press) {
            self.remember_terminal_key_release(keystroke.key.as_str(), pane_id);
        }

        true
    }

    fn write_terminal_key_release(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        // Use the pane that received the press so delayed releases do not drift
        // to whatever pane is active when the key is eventually released.
        match take_pending_key_release_action(
            &mut self.pending_key_releases,
            keystroke.key.as_str(),
        ) {
            PendingKeyReleaseAction::Drop => false,
            PendingKeyReleaseAction::ForwardToPane(pane_id) => self
                .write_terminal_keystroke_to_pane(
                    pane_id.as_str(),
                    keystroke,
                    TerminalKeyEventKind::Release,
                    cx,
                ),
            PendingKeyReleaseAction::FallbackToActivePane => {
                self.write_terminal_keystroke(keystroke, TerminalKeyEventKind::Release, cx)
            }
        }
    }

    pub(in crate::terminal_view) fn release_forwarded_modifiers(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let previous = std::mem::take(&mut self.last_terminal_modifiers);
        let mut wrote_input = false;
        let mut cleared_selection = false;

        for (keystroke, event_kind) in
            modifier_transition_events(previous, gpui::Modifiers::default())
        {
            let wrote = match event_kind {
                TerminalKeyEventKind::Press => {
                    self.write_forwarded_terminal_key_event(&keystroke, event_kind, cx)
                }
                TerminalKeyEventKind::Release => self.write_terminal_key_release(&keystroke, cx),
                TerminalKeyEventKind::Repeat => false,
            };
            if wrote {
                wrote_input = true;
                cleared_selection |= self.clear_selection();
            }
        }

        if cleared_selection {
            cx.notify();
        }

        wrote_input
    }

    fn write_dropped_paths(&mut self, input: &[u8], cx: &mut Context<Self>) {
        let _ = self.close_terminal_context_menu(cx);
        self.write_terminal_paste_input(input, cx);
        cx.notify();
    }

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

        let previous = self.last_terminal_modifiers;
        self.last_terminal_modifiers = event.modifiers;
        let overlay_owns_terminal_input = self.overlay_owns_terminal_input();
        if overlay_owns_terminal_input {
            // Match key-down/up ownership rules: overlays consume modifier
            // transitions too, so terminal-only synth events must stop here.
            return;
        }
        let mut wrote_input = false;
        let mut cleared_selection = false;
        for (keystroke, event_kind) in terminal_modifier_transition_events(
            previous,
            event.modifiers,
            overlay_owns_terminal_input,
        ) {
            let wrote = match event_kind {
                TerminalKeyEventKind::Press => {
                    self.write_forwarded_terminal_key_event(&keystroke, event_kind, cx)
                }
                TerminalKeyEventKind::Release => self.write_terminal_key_release(&keystroke, cx),
                TerminalKeyEventKind::Repeat => false,
            };
            if wrote {
                wrote_input = true;
                cleared_selection |= self.clear_selection();
            }
        }

        if wrote_input || cleared_selection {
            cx.notify();
        }
    }

    fn prepare_terminal_input_write(&mut self, cx: &mut Context<Self>) {
        self.terminal_scroll_accumulator_y = 0.0;
        self.input_scroll_suppress_until =
            Some(Instant::now() + Duration::from_millis(INPUT_SCROLL_SUPPRESS_MS));
        self.scroll_to_bottom(cx);
    }

    pub(in super::super) fn write_terminal_input_to_pane(
        &mut self,
        pane_id: &str,
        input: &[u8],
        cx: &mut Context<Self>,
    ) {
        if input.is_empty() {
            return;
        }

        if should_prepare_terminal_input_write(self.active_pane_id(), pane_id) {
            self.prepare_terminal_input_write(cx);
        }
        if self.send_input_to_pane(pane_id, input) && self.runtime_kind() == RuntimeKind::Tmux {
            self.schedule_tmux_title_refresh();
        }
    }

    pub(in super::super) fn write_terminal_input(&mut self, input: &[u8], cx: &mut Context<Self>) {
        if input.is_empty() {
            return;
        }

        let Some(pane_id) = self.active_pane_id().map(str::to_owned) else {
            return;
        };

        self.write_terminal_input_to_pane(pane_id.as_str(), input, cx);
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

        if self.overlay_owns_terminal_input() {
            if self.is_command_palette_open() {
                if self.handle_command_palette_key_down(key, window, cx) {
                    self.remember_consumed_key_release(key);
                }
                return;
            }

            if self.search_open {
                if self.handle_search_key_down(key, event.keystroke.modifiers.shift, cx) {
                    self.remember_consumed_key_release(key);
                }
                return;
            }

            if self.agent_sidebar_search_active {
                match key {
                    "escape" => {
                        self.dismiss_agent_sidebar_search(cx);
                        self.remember_consumed_key_release(key);
                    }
                    "enter" => {
                        self.open_first_matching_agent_thread(cx);
                        self.remember_consumed_key_release(key);
                    }
                    _ => {}
                }
                return;
            }

            match key {
                "enter" => {
                    if self.renaming_agent_thread_id.is_some() {
                        self.commit_rename_agent_thread(cx);
                    } else {
                        self.commit_rename_tab(cx);
                    }
                    self.remember_consumed_key_release(key);
                    return;
                }
                "escape" => {
                    if self.renaming_agent_thread_id.is_some() {
                        self.cancel_rename_agent_thread(cx);
                    } else {
                        self.cancel_rename_tab(cx);
                    }
                    self.remember_consumed_key_release(key);
                    return;
                }
                _ => return,
            }
        }

        self.last_terminal_modifiers = event.keystroke.modifiers;

        // Printable character input without modifiers is delegated to the
        // platform IME / input handler so that CJK input methods work.
        // Named special keys (enter, tab, space, etc.) and modifier
        // combinations are still handled here via keystroke_to_input.
        if should_defer_key_down_to_ime(&event.keystroke) {
            self.deferred_ime_key_releases.insert(key.to_string());
            // Let the event propagate to the platform IME handler which
            // will call `replace_text_in_range` on our EntityInputHandler.
            return;
        }

        let event_kind = if event.is_held {
            TerminalKeyEventKind::Repeat
        } else {
            TerminalKeyEventKind::Press
        };
        if self.write_forwarded_terminal_key_event(&event.keystroke, event_kind, cx) {
            self.clear_selection();
            // Stop propagation so the event does not bubble up to the IME input handler.
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(in super::super) fn handle_key_up(
        &mut self,
        event: &KeyUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_terminal_input() {
            self.last_terminal_modifiers = event.keystroke.modifiers;
            return;
        }

        self.last_terminal_modifiers = event.keystroke.modifiers;
        // IME-deferred printable keys never wrote a terminal press, so their
        // matching release must be dropped to avoid an unpaired kitty release.
        if take_deferred_ime_key_release(
            &mut self.deferred_ime_key_releases,
            event.keystroke.key.as_str(),
        ) {
            return;
        }
        if self.write_terminal_key_release(&event.keystroke, cx) {
            self.clear_selection();
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
        let paths_list = paths.paths();
        let Some(input) = dropped_paths_to_terminal_paste_input(paths_list) else {
            return;
        };
        self.write_dropped_paths(&input, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_native_file_drop_result(
        &mut self,
        result: super::NativeDropResult,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(paths) => {
                let Some(input) = dropped_paths_to_terminal_paste_input(&paths) else {
                    return;
                };
                self.write_dropped_paths(&input, cx);
            }
            Err(error) => {
                termy_toast::error(error.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PendingKeyRelease, PendingKeyReleaseAction, clipboard_item_to_terminal_paste_input,
        dropped_paths_to_terminal_paste_input, image_extension, modifier_transition_events,
        shell_quote_paths, should_defer_key_down_to_ime, should_prepare_terminal_input_write,
        take_deferred_ime_key_release, take_pending_key_release_action,
        terminal_modifier_transition_events,
    };
    use gpui::{Keystroke, Modifiers};
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };

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
    fn dropped_paths_add_a_trailing_space() {
        let paths = vec![PathBuf::from("/tmp/file with space.png")];
        let input = dropped_paths_to_terminal_paste_input(&paths).expect("drop should serialize");
        assert_eq!(
            String::from_utf8(input).expect("drop input should be utf8"),
            "'/tmp/file with space.png' "
        );
    }

    #[test]
    fn dropped_paths_rejects_empty_input() {
        assert!(dropped_paths_to_terminal_paste_input(&[]).is_none());
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

    #[test]
    fn modifier_transitions_synthesize_shift_press_with_super_held() {
        let previous = Modifiers {
            platform: true,
            ..Modifiers::default()
        };
        let current = Modifiers {
            platform: true,
            shift: true,
            ..Modifiers::default()
        };

        let events = modifier_transition_events(previous, current);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0.key, "shift");
        assert_eq!(events[0].1, termy_terminal_ui::TerminalKeyEventKind::Press);
        assert!(events[0].0.modifiers.platform);
        assert!(events[0].0.modifiers.shift);
    }

    #[test]
    fn overlay_owned_modifier_changes_skip_terminal_events() {
        let previous = Modifiers {
            platform: true,
            ..Modifiers::default()
        };

        assert!(
            terminal_modifier_transition_events(previous, Modifiers::default(), true).is_empty()
        );
    }

    #[test]
    fn non_active_pane_writes_skip_terminal_input_prepare() {
        assert!(should_prepare_terminal_input_write(
            Some("%pane-1"),
            "%pane-1"
        ));
        assert!(!should_prepare_terminal_input_write(
            Some("%pane-1"),
            "%pane-2"
        ));
        assert!(!should_prepare_terminal_input_write(None, "%pane-1"));
    }

    #[test]
    fn deferred_ime_key_release_is_cleared_without_forwarding() {
        let mut deferred = HashSet::from(["a".to_string()]);

        assert!(take_deferred_ime_key_release(&mut deferred, "a"));
        assert!(!take_deferred_ime_key_release(&mut deferred, "a"));
        assert!(deferred.is_empty());
    }

    #[test]
    fn consumed_key_release_is_dropped_after_transient_input_closes() {
        let mut pending = HashMap::from([("enter".to_string(), PendingKeyRelease::Consumed)]);

        assert_eq!(
            take_pending_key_release_action(&mut pending, "enter"),
            PendingKeyReleaseAction::Drop
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn forwarded_key_release_routes_to_original_pane() {
        let mut pending = HashMap::from([(
            "enter".to_string(),
            PendingKeyRelease::Terminal {
                pane_id: "%pane-1".to_string(),
            },
        )]);

        assert_eq!(
            take_pending_key_release_action(&mut pending, "enter"),
            PendingKeyReleaseAction::ForwardToPane("%pane-1".to_string())
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn modifier_releases_keep_their_original_panes() {
        let mut pending = HashMap::from([
            (
                "shift".to_string(),
                PendingKeyRelease::Terminal {
                    pane_id: "%left".to_string(),
                },
            ),
            (
                "super".to_string(),
                PendingKeyRelease::Terminal {
                    pane_id: "%right".to_string(),
                },
            ),
        ]);

        assert_eq!(
            take_pending_key_release_action(&mut pending, "shift"),
            PendingKeyReleaseAction::ForwardToPane("%left".to_string())
        );
        assert_eq!(
            take_pending_key_release_action(&mut pending, "super"),
            PendingKeyReleaseAction::ForwardToPane("%right".to_string())
        );
        assert!(pending.is_empty());
    }
}
