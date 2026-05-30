use super::*;
use gpui_native_appkit::{add_window_to_tab_group, configure_window_tabbing_with_callback};
use std::os::raw::c_void;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeWindowTabRequest {
    New,
}

pub(super) struct NativeWindowTabCallbackContext {
    action_tx: Sender<NativeWindowTabRequest>,
    wake_tx: Sender<()>,
}

pub(super) fn leak_callback_context(
    action_tx: Sender<NativeWindowTabRequest>,
    wake_tx: Sender<()>,
) -> usize {
    Box::into_raw(Box::new(NativeWindowTabCallbackContext {
        action_tx,
        wake_tx,
    })) as usize
}

impl TerminalView {
    pub(crate) fn configure_native_window_tabbing(&self, window: &Window) {
        if !self.supports_native_window_tabs() {
            return;
        }

        let title = self.native_window_tab_title();
        if let Err(error) = configure_window_tabbing_with_callback(
            window,
            title,
            Some(native_window_tab_callback),
            self.native_window_tab_callback_context as *mut c_void,
        ) {
            log::warn!("Failed to configure native macOS window tabs: {error}");
        }
    }

    pub(super) fn uses_native_window_tabs(&self, show_chrome: bool, vertical_tabs: bool) -> bool {
        show_chrome && !vertical_tabs && self.supports_native_window_tabs() && self.tabs.len() <= 1
    }

    pub(super) fn open_native_window_tab(
        &mut self,
        anchor_window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.should_open_new_tabs_as_native_windows() {
            return false;
        }

        let working_dir = self.preferred_working_dir_for_new_session(None, cx);
        let new_window =
            match crate::open_main_window_with_runtime_config_overrides(cx, working_dir) {
                Ok(window) => window,
                Err(error) => {
                    log::error!("Failed to open native macOS tab window: {error}");
                    termy_toast::error(error);
                    return true;
                }
            };

        match new_window.update(cx, |view, new_window, _cx| {
            view.configure_native_window_tabbing(new_window);
            add_window_to_tab_group(anchor_window, new_window)
        }) {
            Ok(Ok(())) => true,
            Ok(Err(error)) => {
                log::warn!("Failed to add new window to native macOS tab group: {error}");
                termy_toast::warning("Opened new tab as a separate window");
                true
            }
            Err(error) => {
                log::warn!("Failed to access new native macOS tab window: {error}");
                termy_toast::warning("Opened new tab as a separate window");
                true
            }
        }
    }

    pub(super) fn process_native_window_tab_actions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut handled = false;
        while let Ok(request) = self.native_window_tab_action_rx.try_recv() {
            match request {
                NativeWindowTabRequest::New => {
                    handled |= self.open_native_window_tab(window, cx);
                }
            }
        }
        handled
    }

    fn should_open_new_tabs_as_native_windows(&self) -> bool {
        self.supports_native_window_tabs()
            && self.tab_bar_position == TabBarPosition::Top
            && self.tabs.len() <= 1
    }

    fn supports_native_window_tabs(&self) -> bool {
        self.runtime_kind() == RuntimeKind::Native
    }

    fn native_window_tab_title(&self) -> &str {
        self.tabs
            .get(self.active_tab)
            .map(|tab| tab.title.as_str())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Termy")
    }
}

unsafe extern "C" fn native_window_tab_callback(context: *mut c_void) {
    if context.is_null() {
        return;
    }

    let context = unsafe { &*(context as *const NativeWindowTabCallbackContext) };
    let _ = context.action_tx.try_send(NativeWindowTabRequest::New);
    let _ = context.wake_tx.try_send(());
}
