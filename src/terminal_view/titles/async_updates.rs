use super::super::*;

impl TerminalView {
    pub(super) fn cancel_pending_command_title(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }

        let tab = &mut self.tabs[index];
        tab.pending_command_token = tab.pending_command_token.wrapping_add(1);
        tab.pending_command_title = None;
    }

    pub(super) fn set_explicit_title(&mut self, index: usize, explicit_title: String) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        let explicit_title = Self::truncate_tab_title(&explicit_title);
        if self.tabs[index].explicit_title.as_deref() == Some(explicit_title.as_str()) {
            return false;
        }

        self.tabs[index].explicit_title = Some(explicit_title);
        self.refresh_tab_title(index)
    }

    pub(super) fn schedule_delayed_command_title(
        &mut self,
        tab_id: TabId,
        command_title: String,
        delay_ms: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_for_tab_id(tab_id) else {
            return;
        };

        let tab = &mut self.tabs[index];
        tab.pending_command_token = tab.pending_command_token.wrapping_add(1);
        tab.pending_command_title = Some(Self::truncate_tab_title(&command_title));
        let token = tab.pending_command_token;

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            smol::Timer::after(Duration::from_millis(delay_ms)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    if view.activate_pending_command_title_for_id(tab_id, token) {
                        cx.notify();
                    }
                })
            });
        })
        .detach();
    }

    fn activate_pending_command_title_for_id(&mut self, tab_id: TabId, token: u64) -> bool {
        let Some(index) =
            Self::tab_index_for_id_in_order(self.tabs.iter().map(|tab| tab.id), tab_id)
        else {
            return false;
        };

        self.activate_pending_command_title(index, token)
    }

    fn activate_pending_command_title(&mut self, index: usize, token: u64) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        let tab = &mut self.tabs[index];
        if tab.pending_command_token != token {
            return false;
        }

        let Some(command_title) = tab.pending_command_title.take() else {
            return false;
        };

        if tab.explicit_title.as_deref() == Some(command_title.as_str()) {
            return false;
        }

        tab.explicit_title = Some(command_title);
        self.refresh_tab_title(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_title_target_index_tracks_tab_reorder() {
        let initial: [TabId; 3] = [11, 13, 17];
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(initial, 13),
            Some(1)
        );

        let reordered: [TabId; 3] = [11, 17, 13];
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(reordered, 13),
            Some(2)
        );
    }

    #[test]
    fn delayed_title_target_index_returns_none_after_tab_close() {
        let initial: [TabId; 3] = [11, 13, 17];
        let after_close: [TabId; 2] = [11, 17];
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(initial, 13),
            Some(1)
        );
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(after_close, 13),
            None
        );
    }
}
