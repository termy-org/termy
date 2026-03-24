#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app_actions;
mod chrome_style;
mod colors;
mod commands;
mod config;
mod deeplink;
mod keybindings;
mod menus;
mod settings_view;
mod startup;
mod terminal_view;
mod text_input;
mod theme_store;
mod ui;

use commands::{OpenConfig, OpenSettings};
use deeplink::{DeepLinkArgument, DeepLinkRoute};
use flume::Receiver;
use gpui::{
    App, Application, AsyncApp, Bounds, Pixels, WindowBounds, WindowOptions, prelude::*, px, size,
};
use startup::StartupBlocker;
use terminal_view::{TerminalView, initial_window_background_appearance};
#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn preflight_tmux_runtime(config: &config::AppConfig) -> Result<(), StartupBlocker> {
    if !config.tmux_enabled {
        return Ok(());
    }

    TmuxClient::verify_tmux_version(config.tmux_binary.as_str(), 3, 3)
        .map_err(|error| StartupBlocker::TmuxPreflight(format!("tmux preflight failed: {error}")))
}

#[cfg(target_os = "windows")]
fn preflight_tmux_runtime(config: &config::AppConfig) -> Result<(), StartupBlocker> {
    let _ = config;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn preflight_tmux_runtime(config: &config::AppConfig) -> Result<(), StartupBlocker> {
    if !config.tmux_enabled {
        return Ok(());
    }

    Err(StartupBlocker::TmuxPreflight(
        "tmux runtime is unsupported on this platform".to_string(),
    ))
}

fn normalized_startup_window_size(startup_config: &config::AppConfig) -> gpui::Size<Pixels> {
    let window_width = startup_config.window_width;
    let window_height = startup_config.window_height;

    #[cfg(target_os = "windows")]
    let (window_width, window_height) = if (window_width - LEGACY_DEFAULT_WINDOW_WIDTH).abs()
        < f32::EPSILON
        && (window_height - LEGACY_DEFAULT_WINDOW_HEIGHT).abs() < f32::EPSILON
    {
        (WINDOWS_DEFAULT_WINDOW_WIDTH, WINDOWS_DEFAULT_WINDOW_HEIGHT)
    } else {
        (window_width, window_height)
    };

    size(
        px(window_width.max(MIN_WINDOW_WIDTH)),
        px(window_height.max(MIN_WINDOW_HEIGHT)),
    )
}

#[cfg(target_os = "windows")]
fn should_apply_windows_startup_resize(
    current: gpui::Size<Pixels>,
    desired: gpui::Size<Pixels>,
) -> bool {
    const WINDOW_SIZE_EPSILON: f32 = 0.5;

    (f32::from(current.width) - f32::from(desired.width)).abs() > WINDOW_SIZE_EPSILON
        || (f32::from(current.height) - f32::from(desired.height)).abs() > WINDOW_SIZE_EPSILON
}

fn open_main_window(cx: &mut App, startup_config: config::AppConfig) -> Result<(), String> {
    let window_background = initial_window_background_appearance(&startup_config);
    let startup_window_size = normalized_startup_window_size(&startup_config);
    let bounds = Bounds::centered(None, startup_window_size, cx);

    #[cfg(target_os = "macos")]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Termy".into()),
        appears_transparent: true,
        traffic_light_position: Some(gpui::point(px(12.0), px(10.0))),
    });
    #[cfg(target_os = "windows")]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Termy".into()),
        appears_transparent: false,
        traffic_light_position: None,
    });
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let titlebar = Some(gpui::TitlebarOptions {
        title: None,
        appears_transparent: true,
        traffic_light_position: None,
    });

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar,
            window_background,
            is_movable: true,
            is_resizable: true,
            window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
            ..Default::default()
        },
        move |window, cx| {
            #[cfg(target_os = "windows")]
            {
                let startup_window_size = startup_window_size;
                window.defer(cx, move |window, _cx| {
                    if should_apply_windows_startup_resize(
                        window.viewport_size(),
                        startup_window_size,
                    ) {
                        window.resize(startup_window_size);
                    }
                });
            }

            let view = cx.new({
                let startup_config = startup_config;
                |cx| TerminalView::new(window, cx, startup_config)
            });
            let view_handle = view.downgrade();

            #[cfg(target_os = "macos")]
            {
                let (native_drop_tx, native_drop_rx) = flume::unbounded();
                match terminal_view::install_native_file_drop(window, native_drop_tx) {
                    Ok(()) => {
                        view.update(cx, |view, cx| {
                            view.set_native_file_drop_enabled(true);
                            cx.notify();
                        });
                        let native_drop_view = view.downgrade();
                        cx.spawn(async move |cx: &mut AsyncApp| {
                            while let Ok(result) = native_drop_rx.recv_async().await {
                                cx.update(|cx| {
                                    let _ = native_drop_view.update(cx, |view, cx| {
                                        view.handle_native_file_drop_result(result, cx)
                                    });
                                });
                            }
                        })
                        .detach();
                    }
                    Err(error) => {
                        log::error!("Failed to install native macOS file drop bridge: {error}");
                        termy_toast::error(error.to_string());
                        view.update(cx, |view, cx| {
                            view.set_native_file_drop_enabled(false);
                            cx.notify();
                        });
                    }
                }
            }

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
    .map(|_| ())
    .map_err(|error| format!("Failed to open main window: {}", error))
}

fn reopen_if_no_windows(cx: &mut App, mut reopen: impl FnMut(&mut App)) -> bool {
    if !cx.windows().is_empty() {
        return false;
    }

    reopen(cx);
    true
}

fn reopen_main_window(cx: &mut App) {
    let mut reopen_config_error = None;
    let reopen_load =
        config::load_runtime_config(&mut reopen_config_error, "Failed to load config");
    let reopen_config = reopen_load.config;
    if let Err(blocker) = preflight_tmux_runtime(&reopen_config) {
        let message = blocker.message();
        log::error!("Failed to reopen main window: {}", message);
        termy_toast::error(message);
        return;
    }

    if let Err(error) = open_main_window(cx, reopen_config) {
        log::error!("{}", error);
        termy_toast::error(error);
    }
}

fn focus_or_open_main_window<V: 'static>(
    cx: &mut App,
    mut open_window: impl FnMut(&mut App),
) -> bool {
    if app_actions::focus_existing_window::<V>(cx) {
        return false;
    }

    if app_actions::has_window::<V>(cx) {
        return false;
    }

    open_window(cx);
    true
}

fn start_theme_install_from_deeplink(cx: &mut App, slug: String) {
    let loading_id = termy_toast::loading(format!("Fetching theme \"{slug}\"..."));

    cx.spawn(async move |cx: &mut AsyncApp| {
        let fetch_result = cx
            .background_executor()
            .spawn(async move { theme_store::fetch_theme_for_deeplink_blocking(&slug) })
            .await;

        termy_toast::dismiss_toast(loading_id);

        match fetch_result {
            Ok(theme) => {
                let title = "Install Theme";
                let message = format!(
                    "Install theme \"{}\" into your local theme library?",
                    theme.name
                );
                if !termy_native_sdk::confirm(title, &message) {
                    return;
                }

                let install_loading_id =
                    termy_toast::loading(format!("Installing {}...", theme.name));
                let install_result = cx
                    .background_executor()
                    .spawn(async move { theme_store::install_theme_from_store_blocking(theme) })
                    .await;
                termy_toast::dismiss_toast(install_loading_id);

                cx.update(|cx| match install_result {
                    Ok(installed_theme) => {
                        termy_toast::success(installed_theme.message.clone());
                        app_actions::update_open_settings_windows(cx, |view, settings_cx| {
                            view.apply_theme_store_install(
                                &installed_theme.slug,
                                &installed_theme.version,
                                settings_cx,
                            );
                        });
                    }
                    Err(error) => {
                        log::error!("Failed to install theme from deeplink: {error}");
                        termy_toast::error(error);
                    }
                });
            }
            Err(error) => {
                log::error!("Failed to fetch theme from deeplink: {error}");
                termy_toast::error(error);
            }
        }
    })
    .detach();
}

fn dispatch_deeplink(
    cx: &mut App,
    route: DeepLinkRoute,
    route_argument: Option<DeepLinkArgument>,
) -> Result<(), String> {
    match route {
        DeepLinkRoute::Activate => Ok(()),
        DeepLinkRoute::NewTab => {
            let (command, dir) = match route_argument {
                Some(DeepLinkArgument::NewTab(payload)) => (payload.command, payload.dir),
                Some(DeepLinkArgument::Value(_)) | None => (None, None),
            };
            app_actions::open_new_tab_in_main_window(cx, command, dir)
        }
        DeepLinkRoute::Settings => app_actions::open_settings_window(cx),
        DeepLinkRoute::OpenConfig => app_actions::open_config_file(),
        DeepLinkRoute::ThemeInstall => {
            let slug = route_argument
                .and_then(|argument| match argument {
                    DeepLinkArgument::Value(value) => Some(value),
                    DeepLinkArgument::NewTab(_) => None,
                })
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Theme install deeplink requires a slug".to_string())?;
            start_theme_install_from_deeplink(cx, slug);
            Ok(())
        }
    }
}

fn handle_open_urls_with_main_window<V: 'static>(
    cx: &mut App,
    urls: &[String],
    mut open_window: impl FnMut(&mut App),
    mut dispatch: impl FnMut(&mut App, DeepLinkRoute, Option<DeepLinkArgument>) -> Result<(), String>,
) {
    for raw_url in urls {
        match DeepLinkRoute::parse(raw_url) {
            Ok((route, route_argument)) => {
                log::info!("Handling deeplink: {raw_url}");
                let _ = focus_or_open_main_window::<V>(cx, &mut open_window);
                if let Err(error) = dispatch(cx, route, route_argument) {
                    log::error!("Failed to handle deeplink {raw_url}: {error}");
                    termy_toast::error(error);
                }
            }
            Err(error) => {
                log::warn!("Rejected deeplink {raw_url}: {error}");
                termy_toast::error(error);
            }
        }
    }
}

fn handle_open_urls(cx: &mut App, urls: &[String]) {
    handle_open_urls_with_main_window::<TerminalView>(
        cx,
        urls,
        reopen_main_window,
        dispatch_deeplink,
    );
}

fn spawn_deeplink_listener(cx: &mut App, deeplink_rx: Receiver<Vec<String>>) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Ok(urls) = deeplink_rx.recv_async().await {
            cx.update(|cx| handle_open_urls(cx, &urls));
        }
    })
    .detach();
}

fn main() {
    env_logger::init();

    let (deeplink_tx, deeplink_rx) = flume::unbounded::<Vec<String>>();
    let application = Application::new();

    application.on_reopen(|cx| {
        let _ = reopen_if_no_windows(cx, reopen_main_window);
    });
    application.on_open_urls(move |urls| {
        if let Err(error) = deeplink_tx.send(urls) {
            log::error!("Failed to enqueue deeplink event: {error}");
        }
    });

    application.run(move |cx: &mut App| {
        // Set the dock icon for development builds (no .app bundle).
        #[cfg(debug_assertions)]
        termy_native_sdk::set_dock_icon_from_png(include_bytes!("../assets/termy_icon@1024px.png"));

        spawn_deeplink_listener(cx, deeplink_rx);

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
        // Keep startup menus/keybinds aligned with the active runtime capability set.
        let tmux_runtime_active = if cfg!(target_os = "windows") {
            false
        } else {
            app_config.tmux_enabled
        };
        keybindings::install_keybindings(cx, &app_config, tmux_runtime_active);
        let startup_config = app_config;
        open_main_window(cx, startup_config).unwrap();
    });
}

#[cfg(test)]
mod tests {
    use super::{
        DeepLinkArgument, DeepLinkRoute, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH,
        focus_or_open_main_window, handle_open_urls_with_main_window,
        normalized_startup_window_size, reopen_if_no_windows,
    };
    #[cfg(target_os = "windows")]
    use super::{
        LEGACY_DEFAULT_WINDOW_HEIGHT, LEGACY_DEFAULT_WINDOW_WIDTH, WINDOWS_DEFAULT_WINDOW_HEIGHT,
        WINDOWS_DEFAULT_WINDOW_WIDTH, should_apply_windows_startup_resize,
    };
    use crate::app_actions;
    use crate::config::AppConfig;
    use crate::deeplink::NewTabDeepLink;
    use gpui::{
        App, AppContext, Context, IntoElement, Render, TestAppContext, Window, WindowOptions, div,
        px, size,
    };
    use std::cell::RefCell;

    struct ReopenTestView;

    impl Render for ReopenTestView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn open_test_window(cx: &mut App) {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_cx| ReopenTestView)
        })
        .expect("test window should open");
    }

    #[gpui::test]
    fn reopen_if_no_windows_opens_a_window(cx: &mut TestAppContext) {
        assert!(cx.windows().is_empty(), "expected no windows at test start");

        let reopened = cx.update(|app| reopen_if_no_windows(app, open_test_window));

        assert!(
            reopened,
            "expected reopen hook to run when no windows exist"
        );
        assert_eq!(cx.windows().len(), 1);
    }

    #[gpui::test]
    fn reopen_if_no_windows_does_not_open_when_window_exists(cx: &mut TestAppContext) {
        cx.update(open_test_window);
        assert_eq!(cx.windows().len(), 1);

        let reopened = cx.update(|app| reopen_if_no_windows(app, open_test_window));

        assert!(
            !reopened,
            "expected reopen hook to be skipped when a window already exists"
        );
        assert_eq!(cx.windows().len(), 1);
    }

    #[gpui::test]
    fn focus_or_open_main_window_opens_when_missing(cx: &mut TestAppContext) {
        assert_eq!(cx.windows().len(), 0);

        let opened =
            cx.update(|app| focus_or_open_main_window::<ReopenTestView>(app, open_test_window));

        assert!(opened, "expected missing window to be opened");
        assert_eq!(cx.windows().len(), 1);
    }

    #[gpui::test]
    fn focus_or_open_main_window_reuses_existing_window(cx: &mut TestAppContext) {
        cx.update(open_test_window);
        assert_eq!(cx.windows().len(), 1);

        let opened =
            cx.update(|app| focus_or_open_main_window::<ReopenTestView>(app, open_test_window));

        assert!(
            !opened,
            "expected existing main window to be reused without opening another"
        );
        assert_eq!(cx.windows().len(), 1);
    }

    #[gpui::test]
    fn handle_open_urls_opens_window_before_dispatch(cx: &mut TestAppContext) {
        let handled = RefCell::new(Vec::new());

        cx.update(|app| {
            handle_open_urls_with_main_window::<ReopenTestView>(
                app,
                &[String::from("termy://settings")],
                open_test_window,
                |_, route, route_argument| {
                    handled.borrow_mut().push((route, route_argument));
                    Ok(())
                },
            );
        });

        assert_eq!(cx.windows().len(), 1);
        assert_eq!(*handled.borrow(), vec![(DeepLinkRoute::Settings, None)]);
    }

    #[gpui::test]
    fn bare_deeplink_opens_window_without_error_route(cx: &mut TestAppContext) {
        let handled = RefCell::new(Vec::new());

        cx.update(|app| {
            handle_open_urls_with_main_window::<ReopenTestView>(
                app,
                &[String::from("termy://")],
                open_test_window,
                |_, route, route_argument| {
                    handled.borrow_mut().push((route, route_argument));
                    Ok(())
                },
            );
        });

        assert_eq!(cx.windows().len(), 1);
        assert_eq!(*handled.borrow(), vec![(DeepLinkRoute::Activate, None)]);
    }

    #[gpui::test]
    fn new_tab_deeplink_passes_route_without_argument(cx: &mut TestAppContext) {
        let handled = RefCell::new(Vec::new());

        cx.update(|app| {
            handle_open_urls_with_main_window::<ReopenTestView>(
                app,
                &[String::from("termy://new")],
                open_test_window,
                |_, route, route_argument| {
                    handled.borrow_mut().push((route, route_argument));
                    Ok(())
                },
            );
        });

        assert_eq!(cx.windows().len(), 1);
        assert_eq!(*handled.borrow(), vec![(DeepLinkRoute::NewTab, None)]);
    }

    #[test]
    fn normalized_startup_window_size_uses_default_config_values() {
        let config = AppConfig::default();

        assert_eq!(
            normalized_startup_window_size(&config),
            size(px(1280.0), px(820.0))
        );
    }

    #[test]
    fn normalized_startup_window_size_clamps_to_minimums() {
        let mut config = AppConfig::default();
        config.window_width = 100.0;
        config.window_height = 200.0;

        assert_eq!(
            normalized_startup_window_size(&config),
            size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalized_startup_window_size_migrates_legacy_windows_default() {
        let mut config = AppConfig::default();
        config.window_width = LEGACY_DEFAULT_WINDOW_WIDTH;
        config.window_height = LEGACY_DEFAULT_WINDOW_HEIGHT;

        assert_eq!(
            normalized_startup_window_size(&config),
            size(
                px(WINDOWS_DEFAULT_WINDOW_WIDTH),
                px(WINDOWS_DEFAULT_WINDOW_HEIGHT)
            )
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_startup_resize_correction_skips_matching_sizes() {
        let target = size(px(1280.0), px(820.0));

        assert!(!should_apply_windows_startup_resize(target, target));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_startup_resize_correction_detects_size_mismatch() {
        let current = size(px(900.0), px(700.0));
        let target = size(px(1280.0), px(820.0));

        assert!(should_apply_windows_startup_resize(current, target));
    }

    #[gpui::test]
    fn new_tab_deeplink_passes_optional_command(cx: &mut TestAppContext) {
        let handled = RefCell::new(Vec::new());

        cx.update(|app| {
            handle_open_urls_with_main_window::<ReopenTestView>(
                app,
                &[String::from("termy://new?cmd=git%20status")],
                open_test_window,
                |_, route, route_argument| {
                    handled.borrow_mut().push((route, route_argument));
                    Ok(())
                },
            );
        });

        assert_eq!(cx.windows().len(), 1);
        assert_eq!(
            *handled.borrow(),
            vec![(
                DeepLinkRoute::NewTab,
                Some(DeepLinkArgument::NewTab(NewTabDeepLink {
                    command: Some("git status".to_string()),
                    dir: None,
                }))
            )]
        );
    }

    #[gpui::test]
    fn new_tab_deeplink_passes_optional_command_and_dir(cx: &mut TestAppContext) {
        let handled = RefCell::new(Vec::new());

        cx.update(|app| {
            handle_open_urls_with_main_window::<ReopenTestView>(
                app,
                &[String::from(
                    "termy://new?cmd=git%20status&dir=%2Ftmp%2Fdemo",
                )],
                open_test_window,
                |_, route, route_argument| {
                    handled.borrow_mut().push((route, route_argument));
                    Ok(())
                },
            );
        });

        assert_eq!(cx.windows().len(), 1);
        assert_eq!(
            *handled.borrow(),
            vec![(
                DeepLinkRoute::NewTab,
                Some(DeepLinkArgument::NewTab(NewTabDeepLink {
                    command: Some("git status".to_string()),
                    dir: Some("/tmp/demo".to_string()),
                }))
            )]
        );
    }

    #[gpui::test]
    fn theme_install_deeplink_passes_slug_argument(cx: &mut TestAppContext) {
        let handled = RefCell::new(Vec::new());

        cx.update(|app| {
            handle_open_urls_with_main_window::<ReopenTestView>(
                app,
                &[String::from(
                    "termy://store/theme-install?slug=catppuccin-mocha",
                )],
                open_test_window,
                |_, route, route_argument| {
                    handled.borrow_mut().push((route, route_argument));
                    Ok(())
                },
            );
        });

        assert_eq!(cx.windows().len(), 1);
        assert_eq!(
            *handled.borrow(),
            vec![(
                DeepLinkRoute::ThemeInstall,
                Some(DeepLinkArgument::Value("catppuccin-mocha".to_string()))
            )]
        );
    }

    #[gpui::test]
    fn settings_deeplink_reuses_existing_settings_window(cx: &mut TestAppContext) {
        cx.update(|app| {
            app_actions::open_settings_window(app).expect("settings window should open");
            handle_open_urls_with_main_window::<ReopenTestView>(
                app,
                &[String::from("termy://settings")],
                open_test_window,
                super::dispatch_deeplink,
            );
        });

        let settings_count = cx
            .windows()
            .into_iter()
            .filter(|handle| {
                handle
                    .downcast::<crate::settings_view::SettingsWindow>()
                    .is_some()
            })
            .count();

        assert_eq!(settings_count, 1);
    }
}
