use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::Path,
    sync::{LazyLock, Mutex},
};

use fs4::fs_std::FileExt;
use termy_config_core::{
    ColorSettingId, ColorSettingUpdate, Rgb8, RootSettingId, apply_color_updates,
    color_setting_from_key, color_setting_spec, parse_theme_id, prettify_config_contents,
    remove_root_setting as remove_root_setting_entry, replace_keybind_lines, upsert_root_setting,
};

use super::ConfigIoError;
use super::io::{ensure_config_file, notify_config_changed, write_atomic};

static CONFIG_UPDATE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn update_config_contents<R>(
    updater: impl FnOnce(&str) -> Result<(String, R), String>,
) -> Result<R, String> {
    let _process_guard = CONFIG_UPDATE_LOCK.lock().unwrap_or_else(|poison| {
        log::warn!("Config update lock was poisoned; recovering lock state");
        poison.into_inner()
    });
    let config_path = ensure_config_file().map_err(|error| error.to_string())?;
    let lock_path = config_path.with_extension("lock");
    let lock_path_display = lock_path.display().to_string();
    let process_lock_file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| {
            format!(
                "Failed to open config lock file '{}': {}",
                lock_path_display, source
            )
        })?;
    process_lock_file.lock_exclusive().map_err(|source| {
        format!(
            "Failed to lock config lock file '{}': {}",
            lock_path_display, source
        )
    })?;

    let mut config_lock_file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&config_path)
        .map_err(|source| ConfigIoError::ReadConfig {
            path: config_path.clone(),
            source,
        })
        .map_err(|error| error.to_string())?;
    config_lock_file.lock_exclusive().map_err(|source| {
        format!(
            "Failed to lock config file '{}': {}",
            config_path.display(),
            source
        )
    })?;

    let mut existing = String::new();
    config_lock_file
        .read_to_string(&mut existing)
        .map_err(|source| ConfigIoError::ReadConfig {
            path: config_path.clone(),
            source,
        })
        .map_err(|error| error.to_string())?;
    config_lock_file.unlock().map_err(|source| {
        format!(
            "Failed to unlock config file '{}': {}",
            config_path.display(),
            source
        )
    })?;
    drop(config_lock_file);

    let (updated, result) = updater(&existing)?;
    write_atomic(&config_path, &updated).map_err(|error| error.to_string())?;
    notify_config_changed();
    process_lock_file.unlock().map_err(|source| {
        format!(
            "Failed to unlock config lock file '{}': {}",
            lock_path_display, source
        )
    })?;
    Ok(result)
}

pub fn set_root_setting(setting: RootSettingId, value: &str) -> Result<(), String> {
    update_config_contents(|existing| Ok((upsert_root_setting(existing, setting, value), ())))
}

pub fn remove_root_setting(setting: RootSettingId) -> Result<(), String> {
    update_config_contents(|existing| Ok((remove_root_setting_entry(existing, setting), ())))
}

pub fn set_theme_in_config(theme_id: &str) -> Result<String, String> {
    let theme = parse_theme_id(theme_id).ok_or_else(|| "Invalid theme id".to_string())?;
    set_root_setting(RootSettingId::Theme, &theme)?;
    Ok(format!("Theme set to {}", theme))
}

pub fn set_color_setting(color: ColorSettingId, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value
        && Rgb8::from_hex(value).is_none()
    {
        return Err(format!(
            "Invalid hex color for '{}': {}",
            color_setting_spec(color).key,
            value
        ));
    }

    let updates = vec![ColorSettingUpdate {
        id: color,
        value: value.map(ToString::to_string),
    }];
    update_config_contents(|existing| Ok((apply_color_updates(existing, &updates), ())))
}

pub fn set_keybind_lines(lines: &[String]) -> Result<(), String> {
    update_config_contents(|existing| Ok((replace_keybind_lines(existing, lines), ())))
}

pub fn prettify_config_file() -> Result<String, String> {
    update_config_contents(|existing| {
        let prettified = prettify_config_contents(existing);
        Ok((prettified.clone(), prettified))
    })
}

pub fn import_colors_from_json(json_path: &Path) -> Result<String, String> {
    let contents =
        fs::read_to_string(json_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let json: serde_json::Value =
        serde_json::from_str(&contents).map_err(|e| format!("Invalid JSON: {}", e))?;

    let colors = json
        .as_object()
        .ok_or_else(|| "JSON must be an object".to_string())?;

    let mut updates_by_id: HashMap<ColorSettingId, String> = HashMap::new();
    for (key, value) in colors {
        if key.starts_with('$') {
            continue;
        }

        let Some(id) = color_setting_from_key(key) else {
            continue;
        };

        let hex = value
            .as_str()
            .ok_or_else(|| format!("Color '{}' must be a hex string", key))?;

        if Rgb8::from_hex(hex).is_none() {
            return Err(format!("Invalid hex color for '{}': {}", key, hex));
        }

        let is_canonical_key = key.eq_ignore_ascii_case(color_setting_spec(id).key);
        match updates_by_id.get_mut(&id) {
            Some(existing_hex) if is_canonical_key => *existing_hex = hex.to_string(),
            Some(_) => {}
            None => {
                updates_by_id.insert(id, hex.to_string());
            }
        }
    }

    if updates_by_id.is_empty() {
        return Err("No valid colors found in JSON".to_string());
    }

    let color_count = updates_by_id.len();
    let updates = updates_by_id
        .into_iter()
        .map(|(id, value)| ColorSettingUpdate {
            id,
            value: Some(value),
        })
        .collect::<Vec<_>>();
    update_config_contents(|existing| Ok((apply_color_updates(existing, &updates), ())))?;
    Ok(format!("Imported {} colors", color_count))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::{LazyLock, Mutex};

    use super::import_colors_from_json;

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct XdgConfigHomeGuard {
        previous_xdg: Option<OsString>,
    }

    impl XdgConfigHomeGuard {
        fn set(xdg_home: &Path) -> Self {
            let previous_xdg = std::env::var_os("XDG_CONFIG_HOME");
            unsafe { std::env::set_var("XDG_CONFIG_HOME", xdg_home) };
            Self { previous_xdg }
        }
    }

    impl Drop for XdgConfigHomeGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous_xdg.take() {
                unsafe { std::env::set_var("XDG_CONFIG_HOME", previous) };
            } else {
                unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
            }
        }
    }

    fn with_temp_xdg_config_home_inner(test: impl FnOnce(&Path)) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let xdg_home = temp_dir.path().join("xdg");
        std::fs::create_dir_all(&xdg_home).expect("create xdg home");

        let _restore_guard = XdgConfigHomeGuard::set(&xdg_home);
        test(temp_dir.path());
    }

    fn with_temp_xdg_config_home(test: impl FnOnce(&Path)) {
        let _guard = ENV_LOCK.lock().expect("env lock");
        with_temp_xdg_config_home_inner(test);
    }

    #[test]
    fn with_temp_xdg_config_home_restores_environment_after_panic() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let before = std::env::var_os("XDG_CONFIG_HOME");
        let result = std::panic::catch_unwind(|| {
            with_temp_xdg_config_home_inner(|_| panic!("intentional panic"));
        });
        assert!(result.is_err());
        assert_eq!(std::env::var_os("XDG_CONFIG_HOME"), before);
    }

    #[test]
    fn import_colors_json_accepts_aliases_and_canonical_keys() {
        with_temp_xdg_config_home(|temp_dir| {
            let json_path = temp_dir.join("colors.json");
            std::fs::write(
                &json_path,
                "{\n  \"foreground\": \"#112233\",\n  \"color1\": \"#445566\",\n  \"red\": \"#778899\"\n}\n",
            )
            .expect("write json");

            let result = import_colors_from_json(&json_path).expect("import colors");
            assert!(result.contains("Imported"));
        });
    }
}
