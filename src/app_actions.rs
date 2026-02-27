use crate::config;
use crate::settings_view::SettingsWindow;
use crate::terminal_view::initial_window_background_appearance;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};

pub(crate) fn open_config_file() -> Result<(), String> {
    config::open_config_file().map_err(|error| error.to_string())
}

pub(crate) fn open_settings_window(cx: &mut App) -> Result<(), String> {
    let initial_window_size = size(px(800.0), px(600.0));
    let bounds = Bounds::centered(None, initial_window_size, cx);
    let mut settings_config_error = None;
    let settings_load = config::load_runtime_config(
        &mut settings_config_error,
        "Failed to load config for settings window",
    );
    let window_background = initial_window_background_appearance(&settings_load.config);

    #[cfg(target_os = "macos")]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Settings".into()),
        appears_transparent: true,
        traffic_light_position: Some(gpui::point(px(12.0), px(10.0))),
        ..Default::default()
    });
    #[cfg(target_os = "windows")]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Settings".into()),
        ..Default::default()
    });
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Settings".into()),
        appears_transparent: true,
        ..Default::default()
    });

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar,
            window_background,
            window_min_size: Some(initial_window_size),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| SettingsWindow::new(window, cx)),
    )
    .map(|_| ())
    .map_err(|error| format!("Failed to open settings window: {}", error))
}
