use super::SettingsWindow;
use crate::app_actions::open_settings_window;
use gpui::TestAppContext;

pub(super) fn open_settings_window_handle(
    cx: &mut TestAppContext,
) -> gpui::WindowHandle<SettingsWindow> {
    cx.update(|app| {
        open_settings_window(app).expect("settings window should open");
    });
    cx.windows()
        .into_iter()
        .find_map(|handle| handle.downcast::<SettingsWindow>())
        .expect("settings window should exist")
}
