use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct EmbeddedAssets;

macro_rules! settings_icon {
    ($name:literal) => {
        (
            concat!("icons/settings/", $name, ".svg"),
            include_bytes!(concat!("../assets/icons/settings/", $name, ".svg")) as &[u8],
        )
    };
}

macro_rules! palette_icon {
    ($name:literal) => {
        (
            concat!("icons/command_palette/", $name, ".svg"),
            include_bytes!(concat!("../assets/icons/command_palette/", $name, ".svg")) as &[u8],
        )
    };
}

macro_rules! sidebar_icon {
    ($name:literal) => {
        (
            concat!("icons/sidebar/", $name, ".svg"),
            include_bytes!(concat!("../assets/icons/sidebar/", $name, ".svg")) as &[u8],
        )
    };
}

const SETTINGS_ICONS: &[(&str, &[u8])] = &[
    settings_icon!("appearance"),
    settings_icon!("terminal"),
    settings_icon!("tabs"),
    settings_icon!("themes"),
    settings_icon!("colors"),
    settings_icon!("keybindings"),
    settings_icon!("advanced"),
    settings_icon!("search"),
    settings_icon!("chevron-down"),
    settings_icon!("chevron-up"),
    settings_icon!("reset"),
];

const COMMAND_PALETTE_ICONS: &[(&str, &[u8])] = &[
    palette_icon!("new-tab"),
    palette_icon!("close-tab"),
    palette_icon!("tab-left"),
    palette_icon!("tab-right"),
    palette_icon!("rename"),
    palette_icon!("split-right"),
    palette_icon!("split-down"),
    palette_icon!("focus-pane"),
    palette_icon!("resize-pane"),
    palette_icon!("zoom-pane"),
    palette_icon!("minimize"),
    palette_icon!("layout"),
    palette_icon!("play"),
    palette_icon!("zoom-in"),
    palette_icon!("zoom-out"),
    palette_icon!("zoom-reset"),
    palette_icon!("info"),
    palette_icon!("restart"),
    palette_icon!("power"),
    palette_icon!("clipboard"),
    palette_icon!("cli"),
    palette_icon!("sidebar"),
    palette_icon!("command"),
    palette_icon!("check-update"),
    palette_icon!("pin"),
    palette_icon!("link"),
    palette_icon!("folder"),
];

const SIDEBAR_ICONS: &[(&str, &[u8])] = &[sidebar_icon!("collapse"), sidebar_icon!("expand")];

const ICON_BUNDLES: &[&[(&str, &[u8])]] = &[SETTINGS_ICONS, COMMAND_PALETTE_ICONS, SIDEBAR_ICONS];

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let trimmed = path.strip_prefix('/').unwrap_or(path);
        for bundle in ICON_BUNDLES {
            for (key, bytes) in *bundle {
                if *key == trimmed {
                    return Ok(Some(Cow::Borrowed(*bytes)));
                }
            }
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let prefix = path.strip_prefix('/').unwrap_or(path);
        let prefix = prefix.trim_end_matches('/');
        let mut out = Vec::new();
        for bundle in ICON_BUNDLES {
            for (key, _) in *bundle {
                if prefix.is_empty() || key.starts_with(prefix) {
                    out.push(SharedString::from(*key));
                }
            }
        }
        Ok(out)
    }
}
