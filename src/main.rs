#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod colors;
mod commands;
mod config;
mod keybindings;
mod settings_view;
mod terminal_view;
mod text_input;
mod ui;

use commands::{OpenConfig, OpenSettings, Quit};
#[cfg(target_os = "macos")]
use gpui::SystemMenuType;
use gpui::{
    App, Application, Bounds, Menu, MenuItem, WindowBounds, WindowOptions, prelude::*, px, size,
};
use settings_view::SettingsWindow;
use terminal_view::{TerminalView, initial_window_background_appearance};

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

pub(crate) fn app_menu() -> Menu {
    #[cfg(target_os = "macos")]
    let menu_items = vec![
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Settings...", OpenSettings),
        MenuItem::action("Open Config File...", OpenConfig),
        MenuItem::action("Quit", Quit),
    ];
    #[cfg(not(target_os = "macos"))]
    let menu_items = vec![
        MenuItem::separator(),
        MenuItem::action("Settings...", OpenSettings),
        MenuItem::action("Open Config File...", OpenConfig),
        MenuItem::action("Quit", Quit),
    ];

    Menu {
        name: "Termy".into(),
        items: menu_items,
    }
}

fn main() {
    env_logger::init();

    Application::new().run(|cx: &mut App| {
        cx.on_action(|_: &OpenConfig, _cx| config::open_config_file());
        cx.on_action(|_: &OpenSettings, cx| {
            let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);

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
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| SettingsWindow::new(window, cx)),
            )
            .ok();
        });

        let app_config = config::AppConfig::load_or_create();
        keybindings::install_keybindings(cx, &app_config);
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
