use super::*;

impl TerminalView {
    fn terminal_context_menu_capabilities(
        &self,
        cx: &mut Context<Self>,
    ) -> (bool, bool, bool, bool) {
        let has_selection = self.selected_text().is_some();
        let can_copy = has_selection;
        let can_paste = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .is_some();
        let can_ask_ai = has_selection;
        let can_search_google = has_selection;
        (can_copy, can_paste, can_ask_ai, can_search_google)
    }

    fn command_action_for_context_menu_action(
        action: termy_native_sdk::ContextMenuAction,
    ) -> Option<CommandAction> {
        match action {
            termy_native_sdk::ContextMenuAction::Copy => Some(CommandAction::Copy),
            termy_native_sdk::ContextMenuAction::Paste => Some(CommandAction::Paste),
            termy_native_sdk::ContextMenuAction::OpenSearch => Some(CommandAction::OpenSearch),
            termy_native_sdk::ContextMenuAction::AskAi
            | termy_native_sdk::ContextMenuAction::SearchGoogle => None,
        }
    }

    pub(in super::super) fn close_terminal_context_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.terminal_context_menu.take().is_some() {
            self.notify_overlay(cx);
            true
        } else {
            false
        }
    }

    pub(in super::super) fn execute_terminal_context_menu_command(
        &mut self,
        action: CommandAction,
        cx: &mut Context<Self>,
    ) {
        if !matches!(action, CommandAction::Copy | CommandAction::Paste) {
            return;
        }
        let _ = self.close_terminal_context_menu(cx);
        let _ = self.execute_input_command_action(action, cx);
    }

    fn execute_terminal_context_menu_action(
        &mut self,
        action: termy_native_sdk::ContextMenuAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(command_action) = Self::command_action_for_context_menu_action(action) {
            if command_action == CommandAction::OpenSearch {
                let _ = self.close_terminal_context_menu(cx);
                self.open_search(cx);
            } else {
                self.execute_terminal_context_menu_command(command_action, cx);
            }
            return;
        }

        if action == termy_native_sdk::ContextMenuAction::AskAi {
            self.execute_terminal_context_menu_ask_ai(cx);
            return;
        }

        if action == termy_native_sdk::ContextMenuAction::SearchGoogle {
            self.execute_terminal_context_menu_search_google(cx);
        }
    }

    pub(in super::super) fn execute_terminal_context_menu_ask_ai(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let selected = self.selected_text();
        let _ = self.close_terminal_context_menu(cx);
        self.open_ai_input(cx);
        if let Some(selected) = selected {
            self.ai_input_mut().set_text(selected);
            self.reset_cursor_blink_phase();
            cx.notify();
        }
    }

    pub(in super::super) fn execute_terminal_context_menu_search_google(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let selected = self.selected_text();
        let _ = self.close_terminal_context_menu(cx);
        let Some(selected) = selected else {
            return;
        };
        if selected.trim().is_empty() {
            return;
        }

        let query: String = url::form_urlencoded::byte_serialize(selected.as_bytes()).collect();
        let url = format!("https://www.google.com/search?q={query}");
        if let Err(error) = webbrowser::open(&url) {
            termy_toast::error(format!("Failed to open browser: {error}"));
            self.notify_overlay(cx);
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn schedule_native_terminal_context_menu(
        &mut self,
        can_copy: bool,
        can_paste: bool,
        can_ask_ai: bool,
        can_search_google: bool,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let action = smol::unblock(move || {
                termy_native_sdk::show_copy_paste_context_menu(
                    can_copy,
                    can_paste,
                    can_ask_ai,
                    can_search_google,
                )
            })
            .await;
            let Some(action) = action else {
                return;
            };

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.execute_terminal_context_menu_action(action, cx);
                })
            });
        })
        .detach();
    }

    pub(in super::super) fn open_terminal_context_menu(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let (can_copy, can_paste, can_ask_ai, can_search_google) =
            self.terminal_context_menu_capabilities(cx);

        #[cfg(target_os = "linux")]
        {
            let state = TerminalContextMenuState {
                anchor_position: position,
                can_copy,
                can_paste,
                can_ask_ai,
                can_search_google,
            };
            if self.terminal_context_menu != Some(state) {
                self.terminal_context_menu = Some(state);
                self.notify_overlay(cx);
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = position;
            self.schedule_native_terminal_context_menu(
                can_copy,
                can_paste,
                can_ask_ai,
                can_search_google,
                cx,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_action_maps_to_command_action() {
        assert_eq!(
            TerminalView::command_action_for_context_menu_action(
                termy_native_sdk::ContextMenuAction::Copy
            ),
            Some(CommandAction::Copy)
        );
        assert_eq!(
            TerminalView::command_action_for_context_menu_action(
                termy_native_sdk::ContextMenuAction::Paste
            ),
            Some(CommandAction::Paste)
        );
        assert_eq!(
            TerminalView::command_action_for_context_menu_action(
                termy_native_sdk::ContextMenuAction::OpenSearch
            ),
            Some(CommandAction::OpenSearch)
        );
        assert_eq!(
            TerminalView::command_action_for_context_menu_action(
                termy_native_sdk::ContextMenuAction::AskAi
            ),
            None
        );
        assert_eq!(
            TerminalView::command_action_for_context_menu_action(
                termy_native_sdk::ContextMenuAction::SearchGoogle
            ),
            None
        );
    }
}
