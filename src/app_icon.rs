use crate::config::AppConfig;

#[cfg(target_os = "macos")]
const TERMY_DEFAULT_ICON_PNG: &[u8] = include_bytes!("../assets/termy_icon@1024px.png");
#[cfg(target_os = "macos")]
const TERMY_OLD_ICON_PNG: &[u8] = include_bytes!("../assets/termy_old_icon.png");

pub(crate) fn apply_from_config(config: &AppConfig) {
    apply(config.app_icon);
}

pub(crate) fn apply(icon: termy_config_core::AppIcon) {
    #[cfg(target_os = "macos")]
    {
        let icon_bytes = match icon {
            termy_config_core::AppIcon::TermyDefault => TERMY_DEFAULT_ICON_PNG,
            termy_config_core::AppIcon::TermyOld => TERMY_OLD_ICON_PNG,
        };

        termy_native_sdk::set_dock_icon_from_png(icon_bytes);

        let persisted = match icon {
            termy_config_core::AppIcon::TermyDefault => {
                termy_native_sdk::clear_current_app_bundle_file_icon()
            }
            termy_config_core::AppIcon::TermyOld => {
                termy_native_sdk::set_current_app_bundle_file_icon_from_png(icon_bytes)
            }
        };

        if !persisted
            && std::env::current_exe().ok().is_some_and(|path| {
                path.ancestors()
                    .any(|p| p.extension().and_then(|ext| ext.to_str()) == Some("app"))
            })
        {
            log::warn!("Failed to persist selected Termy app icon on the app bundle");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = icon;
    }
}
