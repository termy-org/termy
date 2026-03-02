#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app_actions;
mod colors;
mod commands;
mod config;
mod keybindings;
mod menus;
mod settings_view;
mod startup;
mod terminal_view;
mod text_input;
mod ui;

use commands::{OpenConfig, OpenSettings};
use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use startup::StartupBlocker;
use terminal_view::{TerminalView, initial_window_background_appearance};
use termy_terminal_ui::TmuxClient;

pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const MIN_WINDOW_WIDTH: f32 = 480.0;
const MIN_WINDOW_HEIGHT: f32 = 320.0;
#[cfg(target_os = "windows")]
const LEGACY_DEFAULT_WINDOW_WIDTH: f32 = 1100.0;
#[cfg(target_os = "windows")]
const LEGACY_DEFAULT_WINDOW_HEIGHT: f32 = 720.0;
#[cfg(target_os = "windows")]
const WINDOWS_DEFAULT_WINDOW_WIDTH: f32 = 1280.0;
#[cfg(target_os = "windows")]
const WINDOWS_DEFAULT_WINDOW_HEIGHT: f32 = 820.0;

fn preflight_tmux_runtime(config: &config::AppConfig) -> Result<(), StartupBlocker> {
    if !config.tmux_enabled {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let _ = config;
        return Err(StartupBlocker::TmuxPreflight(
            "tmux runtime is unsupported on Windows; supported platforms are macOS and Linux"
                .to_string(),
        ));
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = config;
        return Err(StartupBlocker::TmuxPreflight(
            "tmux runtime is unsupported on this platform".to_string(),
        ));
    }

    TmuxClient::verify_tmux_version(config.tmux_binary.as_str(), 3, 3)
        .map_err(|error| StartupBlocker::TmuxPreflight(format!("tmux preflight failed: {error}")))
}

fn main() {
    env_logger::init();

    Application::new().run(|cx: &mut App| {
        cx.on_action(|_: &OpenConfig, _cx| {
            if let Err(error) = app_actions::open_config_file() {
                log::error!("Failed to open config file: {}", error);
                termy_toast::error(error);
            }
        });
        cx.on_action(|_: &OpenSettings, cx| {
            if let Err(error) = app_actions::open_settings_window(cx) {
                log::error!("{}", error);
                termy_toast::error(error);
            }
        });

        let mut startup_config_error = None;
        let startup_load =
            config::load_runtime_config(&mut startup_config_error, "Failed to load config");
        let app_config = startup_load.config;
        if let Err(blocker) = preflight_tmux_runtime(&app_config) {
            blocker.present_and_exit();
        }
        keybindings::install_keybindings(cx, &app_config, app_config.tmux_enabled);
        let window_background = initial_window_background_appearance(&app_config);
        let window_width = app_config.window_width;
        let window_height = app_config.window_height;
        let startup_config = app_config;
        #[cfg(target_os = "macos")]
        let main_window_is_movable = false;
        #[cfg(not(target_os = "macos"))]
        let main_window_is_movable = true;
        #[cfg(target_os = "windows")]
        let (window_width, window_height) = if (window_width - LEGACY_DEFAULT_WINDOW_WIDTH).abs()
            < f32::EPSILON
            && (window_height - LEGACY_DEFAULT_WINDOW_HEIGHT).abs() < f32::EPSILON
        {
            (WINDOWS_DEFAULT_WINDOW_WIDTH, WINDOWS_DEFAULT_WINDOW_HEIGHT)
        } else {
            (window_width, window_height)
        };
        let window_width = window_width.max(MIN_WINDOW_WIDTH);
        let window_height = window_height.max(MIN_WINDOW_HEIGHT);
        let bounds = Bounds::centered(None, size(px(window_width), px(window_height)), cx);

        #[cfg(target_os = "macos")]
        let titlebar = Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(12.0), px(10.0))),
            ..Default::default()
        });
        #[cfg(target_os = "windows")]
        let titlebar = Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            ..Default::default()
        });
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        let titlebar = Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            ..Default::default()
        });

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar,
                window_background,
                is_movable: main_window_is_movable,
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new({
                    let startup_config = startup_config;
                    |cx| TerminalView::new(window, cx, startup_config)
                });
                let view_handle = view.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    view_handle
                        .update(cx, |view, cx| {
                            view.handle_window_should_close_request(window, cx)
                        })
                        .unwrap_or(true)
                });
                view
            },
        )
        .unwrap();
    });
}
