use super::*;

impl TerminalView {
    pub(in super::super) fn format_terminal_buffer_position(position: SelectionPos) -> String {
        format!(
            "Buffer Position: Line {}, Column {}",
            position.line, position.col
        )
    }

    fn copyable_terminal_buffer_position(position: SelectionPos) -> String {
        format!("line={},col={}", position.line, position.col)
    }

    fn terminal_context_menu_buffer_position(
        &self,
        position: crate::gpui::Point<Pixels>,
    ) -> Option<SelectionPos> {
        let (_, buffer_position) = self.position_to_pane_selection_pos(position, false)?;
        Some(buffer_position)
    }

    fn terminal_context_menu_capabilities(&self, cx: &mut Context<Self>) -> (bool, bool) {
        let has_selection = self.selected_text().is_some();
        let can_copy = has_selection;
        let can_paste = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .is_some();
        (can_copy, can_paste)
    }

    fn command_action_for_context_menu_action(
        action: termy_native_sdk::ContextMenuAction,
    ) -> Option<CommandAction> {
        match action {
            termy_native_sdk::ContextMenuAction::Copy => Some(CommandAction::Copy),
            termy_native_sdk::ContextMenuAction::Paste => Some(CommandAction::Paste),
            termy_native_sdk::ContextMenuAction::OpenSearch => Some(CommandAction::OpenSearch),
            termy_native_sdk::ContextMenuAction::CopyBufferPosition => None,
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

    pub(in super::super) fn close_tab_context_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tab_context_menu.take().is_some() {
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

    pub(in super::super) fn execute_terminal_context_menu_copy_buffer_position(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self
            .terminal_context_menu
            .as_ref()
            .and_then(|state| state.buffer_position)
        else {
            let _ = self.close_terminal_context_menu(cx);
            return;
        };

        let _ = self.close_terminal_context_menu(cx);
        cx.write_to_clipboard(ClipboardItem::new_string(
            Self::copyable_terminal_buffer_position(position),
        ));
        termy_toast::success("Copied buffer position");
        self.notify_overlay(cx);
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

        if action == termy_native_sdk::ContextMenuAction::CopyBufferPosition {
            self.execute_terminal_context_menu_copy_buffer_position(cx);
            return;
        }
    }

    fn execute_tab_context_menu_action(
        &mut self,
        action: termy_native_sdk::TabContextMenuAction,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_id) = self.tab_context_menu.as_ref().map(|state| state.tab_id) else {
            let _ = self.close_tab_context_menu(cx);
            return;
        };

        let _ = self.close_tab_context_menu(cx);
        match action {
            termy_native_sdk::TabContextMenuAction::Pin => {
                let _ = self.set_tab_pinned_by_id(tab_id, true, cx);
            }
            termy_native_sdk::TabContextMenuAction::Unpin => {
                let _ = self.set_tab_pinned_by_id(tab_id, false, cx);
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn schedule_native_terminal_context_menu(
        &mut self,
        buffer_position_label: Option<String>,
        can_copy: bool,
        can_paste: bool,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let action = smol::unblock(move || {
                termy_native_sdk::show_copy_paste_context_menu(
                    buffer_position_label,
                    can_copy,
                    can_paste,
                )
            })
            .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    let Some(action) = action else {
                        let _ = view.close_terminal_context_menu(cx);
                        return;
                    };
                    view.execute_terminal_context_menu_action(action, cx);
                })
            });
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn schedule_native_tab_context_menu(&mut self, pinned: bool, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let action =
                smol::unblock(move || termy_native_sdk::show_tab_context_menu(pinned)).await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    let Some(action) = action else {
                        let _ = view.close_tab_context_menu(cx);
                        return;
                    };
                    view.execute_tab_context_menu_action(action, cx);
                })
            });
        })
        .detach();
    }

    pub(in super::super) fn open_terminal_context_menu(
        &mut self,
        position: crate::gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let _ = self.close_tab_context_menu(cx);
        let (can_copy, can_paste) = self.terminal_context_menu_capabilities(cx);
        let buffer_position = self.terminal_context_menu_buffer_position(position);
        let state = TerminalContextMenuState {
            anchor_position: position,
            buffer_position,
            can_copy,
            can_paste,
        };
        #[cfg(not(target_os = "macos"))]
        let state_changed = self.terminal_context_menu.as_ref() != Some(&state);
        self.terminal_context_menu = Some(state);

        #[cfg(not(target_os = "macos"))]
        {
            if state_changed {
                self.notify_overlay(cx);
            }
        }

        #[cfg(target_os = "macos")]
        {
            let _ = position;
            let buffer_position_label = buffer_position.map(Self::format_terminal_buffer_position);
            self.schedule_native_terminal_context_menu(
                buffer_position_label,
                can_copy,
                can_paste,
                cx,
            );
        }
    }

    pub(in super::super) fn open_tab_context_menu(
        &mut self,
        tab_index: usize,
        position: crate::gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some((tab_id, pinned)) = self.tabs.get(tab_index).map(|tab| (tab.id, tab.pinned))
        else {
            return;
        };

        let _ = self.close_terminal_context_menu(cx);
        let state = TabContextMenuState {
            anchor_position: position,
            tab_id,
            pinned,
        };
        #[cfg(not(target_os = "macos"))]
        let state_changed = self.tab_context_menu.as_ref() != Some(&state);
        self.tab_context_menu = Some(state);

        #[cfg(not(target_os = "macos"))]
        {
            if state_changed {
                self.notify_overlay(cx);
            }
        }

        #[cfg(target_os = "macos")]
        {
            self.schedule_native_tab_context_menu(pinned, cx);
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
                termy_native_sdk::ContextMenuAction::CopyBufferPosition
            ),
            None
        );
    }

    #[test]
    fn buffer_position_label_uses_terminal_coordinates() {
        assert_eq!(
            TerminalView::format_terminal_buffer_position(SelectionPos { col: 12, line: -3 }),
            "Buffer Position: Line -3, Column 12"
        );
        assert_eq!(
            TerminalView::copyable_terminal_buffer_position(SelectionPos { col: 12, line: -3 }),
            "line=-3,col=12"
        );
    }
}
