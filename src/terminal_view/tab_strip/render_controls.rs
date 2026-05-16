use super::super::*;

#[derive(Clone, Copy)]
pub(super) enum TabStripControlAction {
    NewTab,
    ToggleVerticalSidebar,
}

impl TerminalView {
    pub(super) fn perform_tab_strip_control_action(
        &mut self,
        action: TabStripControlAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            TabStripControlAction::NewTab => {
                self.disarm_titlebar_window_move();
                self.add_tab(cx);
            }
            TabStripControlAction::ToggleVerticalSidebar => {
                self.disarm_titlebar_window_move();
                if let Err(error) = self.set_vertical_tabs_minimized(!self.vertical_tabs_minimized)
                {
                    termy_toast::error(error);
                } else {
                    cx.notify();
                }
            }
        }
    }
}
