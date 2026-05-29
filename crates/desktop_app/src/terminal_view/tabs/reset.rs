use super::*;

impl TerminalView {
    fn clear_tab_hover_fields(
        hovered_tab: &mut Option<usize>,
        hovered_tab_close: &mut Option<usize>,
    ) -> bool {
        let hovered_tab_changed = hovered_tab.take().is_some();
        let hovered_close_changed = hovered_tab_close.take().is_some();
        hovered_tab_changed || hovered_close_changed
    }

    fn reset_tab_rename_fields(
        renaming_tab: &mut Option<usize>,
        rename_input: &mut InlineInputState,
        inline_input_selecting: &mut bool,
    ) -> bool {
        let was_renaming = renaming_tab.take().is_some();
        let had_text = !rename_input.text().is_empty();
        let was_selecting = *inline_input_selecting;
        rename_input.clear();
        *inline_input_selecting = false;
        was_renaming || had_text || was_selecting
    }

    pub(crate) fn clear_tab_hover_state(&mut self) -> bool {
        Self::clear_tab_hover_fields(
            &mut self.tab_strip.hovered_tab,
            &mut self.tab_strip.hovered_tab_close,
        )
    }

    pub(crate) fn reset_tab_rename_state(&mut self) -> bool {
        let changed = Self::reset_tab_rename_fields(
            &mut self.renaming_tab,
            &mut self.rename_input,
            &mut self.inline_input_selecting,
        );
        if changed {
            self.reset_cursor_blink_phase();
        }
        changed
    }

    pub(crate) fn reset_tab_drag_state(&mut self) -> bool {
        self.finish_tab_drag()
    }

    pub(crate) fn reset_tab_interaction_state(&mut self) -> bool {
        let rename_changed = self.reset_tab_rename_state();
        let hover_changed = self.clear_tab_hover_state();
        let drag_changed = self.reset_tab_drag_state();
        let selection_changed = self.clear_selection();
        rename_changed || hover_changed || drag_changed || selection_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_tab_hover_fields_clears_both_slots_and_reports_change() {
        let mut hovered_tab = Some(2);
        let mut hovered_tab_close = Some(4);
        assert!(TerminalView::clear_tab_hover_fields(
            &mut hovered_tab,
            &mut hovered_tab_close
        ));
        assert_eq!(hovered_tab, None);
        assert_eq!(hovered_tab_close, None);
        assert!(!TerminalView::clear_tab_hover_fields(
            &mut hovered_tab,
            &mut hovered_tab_close
        ));
    }

    #[test]
    fn reset_tab_rename_fields_clears_text_focus_and_selection() {
        let mut renaming_tab = Some(1);
        let mut rename_input = InlineInputState::new("rename me".to_string());
        let mut inline_input_selecting = true;
        assert!(TerminalView::reset_tab_rename_fields(
            &mut renaming_tab,
            &mut rename_input,
            &mut inline_input_selecting
        ));
        assert_eq!(renaming_tab, None);
        assert!(rename_input.text().is_empty());
        assert!(!inline_input_selecting);
    }

    #[test]
    fn reset_tab_rename_fields_is_noop_when_state_is_already_clear() {
        let mut renaming_tab = None;
        let mut rename_input = InlineInputState::new(String::new());
        let mut inline_input_selecting = false;
        assert!(!TerminalView::reset_tab_rename_fields(
            &mut renaming_tab,
            &mut rename_input,
            &mut inline_input_selecting
        ));
    }
}
