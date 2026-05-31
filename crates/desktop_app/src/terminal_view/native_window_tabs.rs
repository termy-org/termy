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

/// Reclaim the callback context leaked by [`leak_callback_context`]. Called from
/// `Drop`, by which point the window is tearing down, so the native "New Tab"
/// responder that holds this pointer can no longer fire.
pub(super) fn free_callback_context(context: usize) {
    if context == 0 {
        return;
    }
    // SAFETY: `context` came from `leak_callback_context` (`Box::into_raw`) and
    // is owned by exactly one TerminalView, so it is reconstructed and dropped
    // at most once.
    unsafe {
        drop(Box::from_raw(
            context as *mut NativeWindowTabCallbackContext,
        ));
    }
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
        // Suppress the custom strip ONLY when a real native tab bar is actually
        // present (this window is grouped with 1+ sibling windows). A lone
        // window has no native bar, so it must keep rendering the custom strip
        // / branding — otherwise the titlebar is simply empty.
        //
        // Also require a single in-app tab: a window normally holds exactly one
        // in-app tab in native mode (each native tab is its own window). But the
        // deeplink / command-palette "new tab" paths can push extra in-app tabs
        // onto a grouped window without spawning a sibling window. The native
        // bar only shows one entry per window, so those extra tabs would be
        // invisible — keep the custom strip on whenever tabs.len() > 1 so they
        // stay reachable.
        show_chrome
            && !vertical_tabs
            && self.supports_native_window_tabs()
            && self.native_tab_group_active
            && self.tabs.len() <= 1
    }

    /// Configure native window tabbing exactly once for this window. Safe to
    /// call every frame; it no-ops after the first successful configuration.
    pub(crate) fn ensure_native_window_tabbing_configured(&mut self, window: &Window) {
        if self.native_window_tabbing_configured || !self.supports_native_window_tabs() {
            return;
        }
        self.configure_native_window_tabbing(window);
        self.native_window_tabbing_configured = true;
    }

    /// Refresh `native_tab_group_active` from AppKit's actual tab-group state and
    /// keep the native tab label in sync with the active in-app tab. Called from
    /// render on macOS so leaving/closing the group (e.g. dragging a tab out, or
    /// closing a sibling window) flips the custom strip back on.
    pub(super) fn refresh_native_tab_group_state(&mut self, window: &Window) {
        if !self.supports_native_window_tabs() {
            self.native_tab_group_active = false;
            return;
        }
        if let Ok(count) = gpui_native_appkit::native_window_tab_group_count(window) {
            self.native_tab_group_active = count > 1;
        }
        let title = self.native_window_tab_title().to_string();
        if self.last_synced_native_tab_title.as_deref() != Some(title.as_str())
            && gpui_native_appkit::set_window_tab_title(window, &title).is_ok()
        {
            self.last_synced_native_tab_title = Some(title);
        }
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
            view.ensure_native_window_tabbing_configured(new_window);
            // Optimistically mark the new window grouped so it does not flash the
            // custom strip for one frame before its own refresh runs.
            view.native_tab_group_active = true;
            add_window_to_tab_group(anchor_window, new_window)
        }) {
            Ok(Ok(())) => {
                self.native_tab_group_active = true;
                true
            }
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

impl Drop for TerminalView {
    fn drop(&mut self) {
        free_callback_context(self.native_window_tab_callback_context);
        self.native_window_tab_callback_context = 0;
    }
}
