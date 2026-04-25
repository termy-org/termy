use crate::config;
use crate::settings_view::SettingsWindow;
use crate::terminal_view::TerminalView;
use crate::terminal_view::initial_window_background_appearance;
use crate::gpui::{
    AnyView, AnyWindowHandle, App, AppContext, Bounds, Context, Window, WindowBounds,
    WindowOptions, px, size,
};

fn root_contains_view<V: 'static>(root_view: AnyView, cx: &App) -> bool {
    if root_view.clone().downcast::<V>().is_ok() {
        return true;
    }

    let Some(root) = root_view.downcast::<gpui_component::Root>().ok() else {
        return false;
    };

    root.read(cx).view().clone().downcast::<V>().is_ok()
}

#[cfg(test)]
fn window_contains_view<V: 'static, C: AppContext>(handle: AnyWindowHandle, cx: &C) -> bool {
    if handle.downcast::<V>().is_some() {
        return true;
    }

    handle
        .downcast::<gpui_component::Root>()
        .and_then(|root| {
            root.read_with(cx, |root, _cx| root.view().clone().downcast::<V>().ok())
                .ok()
                .flatten()
        })
        .is_some()
}

fn update_window_view<V: 'static, R>(
    handle: AnyWindowHandle,
    cx: &mut App,
    update: impl FnOnce(&mut V, &mut Window, &mut Context<V>) -> R,
) -> Option<R> {
    let mut update = Some(update);
    handle
        .update(cx, move |root_view, window, cx| {
            let view = if let Ok(view) = root_view.clone().downcast::<V>() {
                view
            } else {
                let Ok(root) = root_view.downcast::<gpui_component::Root>() else {
                    return None;
                };
                let Ok(view) = root.read(cx).view().clone().downcast::<V>() else {
                    return None;
                };
                view
            };

            let update = update
                .take()
                .expect("window view update closure should only execute once");
            Some(view.update(cx, |view, cx| update(view, window, cx)))
        })
        .ok()
        .flatten()
}

pub(crate) fn open_config_file() -> Result<(), String> {
    config::open_config_file().map_err(|error| error.to_string())
}

pub(crate) fn focus_existing_window<V: 'static>(cx: &mut App) -> bool {
    cx.windows().into_iter().any(|handle| {
        handle
            .update(cx, |root_view, window, cx| {
                if !root_contains_view::<V>(root_view, cx) {
                    return false;
                }

                window.activate_window();
                true
            })
            .unwrap_or(false)
    })
}

pub(crate) fn has_window<V: 'static>(cx: &mut App) -> bool {
    cx.windows()
        .into_iter()
        .any(|handle| {
            handle
                .update(cx, |root_view, _window, cx| {
                    root_contains_view::<V>(root_view, cx)
                })
                .unwrap_or(false)
        })
}

pub(crate) fn update_open_settings_windows(
    cx: &mut App,
    mut update: impl FnMut(&mut SettingsWindow, &mut crate::gpui::Context<SettingsWindow>),
) {
    for handle in cx.windows() {
        let _ = update_window_view(handle, cx, |view: &mut SettingsWindow, _window, view_cx| {
            update(view, view_cx);
        });
    }
}

pub(crate) fn open_new_tab_in_main_window(
    cx: &mut App,
    command: Option<String>,
    dir: Option<String>,
) -> Result<(), String> {
    let Some(()) = cx.windows().into_iter().find_map(|handle| {
        update_window_view(handle, cx, |view: &mut TerminalView, window, view_cx| {
            view.open_new_tab_from_deeplink(command.as_deref(), dir.as_deref(), window, view_cx);
        })
    }) else {
        return Err("No main window available for new tab deeplink".to_string());
    };

    Ok(())
}

pub(crate) fn open_settings_window(cx: &mut App) -> Result<(), String> {
    // Key-repeat and repeated action dispatch should raise the existing settings window,
    // not spawn duplicate windows.
    if focus_existing_window::<SettingsWindow>(cx) {
        return Ok(());
    }
    // If a settings window still exists after a failed focus attempt (for example,
    // during a re-entrant update), do not open a duplicate.
    if has_window::<SettingsWindow>(cx) {
        return Ok(());
    }

    let initial_window_size = size(px(1080.0), px(675.0));
    let bounds = Bounds::centered(None, initial_window_size, cx);
    let mut settings_config_error = None;
    let settings_load = config::load_runtime_config(
        &mut settings_config_error,
        "Failed to load config for settings window",
    );
    let window_background = initial_window_background_appearance(&settings_load.config);

    #[cfg(target_os = "macos")]
    let titlebar = Some(crate::gpui::TitlebarOptions {
        title: Some("Settings".into()),
        appears_transparent: true,
        traffic_light_position: Some(crate::gpui::point(px(12.0), px(10.0))),
    });
    #[cfg(target_os = "windows")]
    let titlebar = Some(crate::gpui::TitlebarOptions {
        title: Some("Settings".into()),
        appears_transparent: false,
        traffic_light_position: None,
    });
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let titlebar = Some(crate::gpui::TitlebarOptions {
        title: Some("Settings".into()),
        appears_transparent: true,
        traffic_light_position: None,
    });

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar,
            window_background,
            is_resizable: false,
            window_min_size: Some(initial_window_size),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| SettingsWindow::new(window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        },
    )
    .map(|_| ())
    .map_err(|error| format!("Failed to open settings window: {}", error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpui::{AnyWindowHandle, TestAppContext};

    fn settings_window_count(cx: &TestAppContext) -> usize {
        cx.windows()
            .into_iter()
            .filter(|handle| window_contains_view::<SettingsWindow, _>(handle, cx))
            .count()
    }

    #[crate::gpui::test]
    fn open_settings_window_reuses_existing_window(cx: &mut TestAppContext) {
        assert_eq!(settings_window_count(cx), 0);

        cx.update(|app| {
            open_settings_window(app).expect("settings window should open");
        });
        assert_eq!(settings_window_count(cx), 1);

        cx.update(|app| {
            open_settings_window(app).expect("settings window should be reused");
            open_settings_window(app).expect("settings window should be reused");
        });
        assert_eq!(settings_window_count(cx), 1);
    }

    #[crate::gpui::test]
    fn open_settings_window_does_not_duplicate_when_called_from_settings_update(
        cx: &mut TestAppContext,
    ) {
        cx.update(|app| {
            open_settings_window(app).expect("settings window should open");
        });
        assert_eq!(settings_window_count(cx), 1);

        let settings_window = cx
            .windows()
            .into_iter()
            .find_map(|handle| handle.downcast::<SettingsWindow>())
            .expect("settings window should exist");
        let settings_window_any: AnyWindowHandle = settings_window.into();

        settings_window_any
            .update(cx, |_view, _window, app| {
                open_settings_window(app).expect("settings window should be reused");
            })
            .expect("settings window update should succeed");

        assert_eq!(settings_window_count(cx), 1);
    }
}
