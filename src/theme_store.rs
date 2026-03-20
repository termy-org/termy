use crate::config;
use crate::deeplink::{DeepLinkArgument, DeepLinkRoute};
use std::collections::HashMap;
use std::path::PathBuf;
use termy_themes::{Rgb8, ThemeColors, normalize_theme_id};

const DEFAULT_THEME_STORE_API_URL: &str = "https://api.termy.run";
const DEFAULT_THEME_DEEPLINK_API_URL: &str = "https://termy.run/theme-api";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThemeStoreTheme {
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) description: String,
    pub(crate) latest_version: Option<String>,
    pub(crate) file_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstalledTheme {
    pub(crate) slug: String,
    pub(crate) version: String,
    pub(crate) message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ThemeStoreAuthUser {
    pub(crate) id: String,
    pub(crate) github_user_id: i64,
    pub(crate) github_login: String,
    pub(crate) avatar_url: Option<String>,
    pub(crate) name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ThemeStoreAuthSession {
    pub(crate) session_token: String,
    pub(crate) user: ThemeStoreAuthUser,
}

pub(crate) fn theme_store_api_base_url() -> String {
    std::env::var("THEME_STORE_API_URL").unwrap_or_else(|_| DEFAULT_THEME_STORE_API_URL.into())
}

/// Fetches themes from the API. Returns `(themes, from_cache)` where `from_cache` is `true`
/// when the API was unreachable and a previously saved cache was used as a fallback.
pub(crate) fn fetch_theme_store_themes_blocking(
    api_base: &str,
) -> Result<(Vec<ThemeStoreTheme>, bool), String> {
    let base = api_base.trim_end_matches('/');
    let url = format!("{base}/themes");

    let fetch_result = ureq::get(&url).set("Accept", "application/json").call();

    let response = match fetch_result {
        Ok(response) => response,
        Err(error) => {
            if let Some(cached) = load_theme_store_cache() {
                return Ok((cached, true));
            }
            return Err(format!("Failed to fetch store themes: {error}"));
        }
    };

    let raw = response
        .into_string()
        .map_err(|error| format!("Invalid theme store response: {error}"))?;

    let payload: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| format!("Invalid theme store response: {error}"))?;

    let themes = payload
        .as_array()
        .ok_or_else(|| "Theme store response must be a JSON array".to_string())?;

    let mut parsed = Vec::with_capacity(themes.len());
    for theme in themes {
        if let Some(parsed_theme) = parse_theme_value(theme) {
            parsed.push(parsed_theme);
        }
    }

    parsed.sort_unstable_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });

    save_theme_store_cache(&raw);

    Ok((parsed, false))
}

fn theme_store_cache_path() -> Option<PathBuf> {
    let config_path = config::ensure_config_file().ok()?;
    let parent = config_path.parent()?;
    Some(parent.join("theme_store_cache.json"))
}

fn save_theme_store_cache(raw_json: &str) {
    let Some(path) = theme_store_cache_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Store minified to save space.
    let minified = serde_json::from_str::<serde_json::Value>(raw_json)
        .ok()
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_else(|| raw_json.to_string());
    if let Err(error) = std::fs::write(&path, minified) {
        log::debug!(
            "Failed to write theme store cache to {}: {}",
            path.display(),
            error
        );
    }
}

fn load_theme_store_cache() -> Option<Vec<ThemeStoreTheme>> {
    let path = theme_store_cache_path()?;
    let contents = std::fs::read_to_string(path).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let themes = payload.as_array()?;
    let mut parsed: Vec<ThemeStoreTheme> = themes.iter().filter_map(parse_theme_value).collect();
    if parsed.is_empty() {
        return None;
    }
    parsed.sort_unstable_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    Some(parsed)
}

pub(crate) fn fetch_theme_for_deeplink_blocking(slug: &str) -> Result<ThemeStoreTheme, String> {
    let slug = normalize_slug(slug)?;
    let base = std::env::var("THEME_STORE_DEEPLINK_API_URL")
        .unwrap_or_else(|_| DEFAULT_THEME_DEEPLINK_API_URL.into());
    let url = format!("{}/themes/{}", base.trim_end_matches('/'), slug);
    let response = ureq::get(&url)
        .set("Accept", "application/json")
        .call()
        .map_err(|error| format!("Failed to fetch theme '{slug}': {error}"))?;

    let payload: serde_json::Value = response
        .into_json()
        .map_err(|error| format!("Invalid theme response for '{slug}': {error}"))?;

    parse_theme_value(&payload)
        .ok_or_else(|| format!("Theme response for '{slug}' is missing required fields"))
}

pub(crate) fn fetch_auth_user_blocking(
    api_base: &str,
    session_token: &str,
) -> Result<ThemeStoreAuthUser, String> {
    let base = api_base.trim_end_matches('/');
    let url = format!("{base}/auth/me");
    let response = ureq::get(&url)
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {}", session_token.trim()))
        .call()
        .map_err(|error| format!("Failed to resolve authenticated user: {error}"))?;

    response
        .into_json::<ThemeStoreAuthUser>()
        .map_err(|error| format!("Invalid authenticated user response: {error}"))
}

pub(crate) fn resolve_auth_session_from_input_blocking(
    api_base: &str,
    input: &str,
) -> Result<ThemeStoreAuthSession, String> {
    let session_token = extract_auth_session_token_from_input(input)?;
    let user = fetch_auth_user_blocking(api_base, &session_token)?;
    Ok(ThemeStoreAuthSession {
        session_token,
        user,
    })
}

pub(crate) fn logout_auth_session_blocking(
    api_base: &str,
    session_token: &str,
) -> Result<(), String> {
    let base = api_base.trim_end_matches('/');
    let url = format!("{base}/auth/logout");
    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {}", session_token.trim()))
        .call();

    match response {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(401, _)) => Ok(()),
        Err(error) => Err(format!("Failed to logout from theme store: {error}")),
    }
}

pub(crate) fn load_auth_session() -> Option<ThemeStoreAuthSession> {
    let path = auth_session_path()?;
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<ThemeStoreAuthSession>(&contents).ok()
}

pub(crate) fn persist_auth_session(session: &ThemeStoreAuthSession) -> Result<(), String> {
    let Some(path) = auth_session_path() else {
        return Err("Config path unavailable".to_string());
    };
    let Some(parent) = path.parent() else {
        return Err("Invalid auth session path".to_string());
    };
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create auth session directory: {error}"))?;
    let contents = serde_json::to_string_pretty(session)
        .map_err(|error| format!("Failed to serialize auth session: {error}"))?;
    std::fs::write(path, contents)
        .map_err(|error| format!("Failed to write auth session: {error}"))?;
    Ok(())
}

pub(crate) fn clear_auth_session() -> Result<(), String> {
    let Some(path) = auth_session_path() else {
        return Ok(());
    };
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("Failed to clear auth session: {error}"))?;
    }
    Ok(())
}

pub(crate) fn load_installed_theme_versions() -> HashMap<String, String> {
    let Some(path) = installed_theme_state_path() else {
        return HashMap::new();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };

    if let Ok(parsed_map) = serde_json::from_str::<HashMap<String, String>>(&contents) {
        return parsed_map
            .into_iter()
            .map(|(slug, version)| (slug.trim().to_ascii_lowercase(), version.trim().to_string()))
            .filter(|(slug, _)| !slug.is_empty())
            .collect();
    }

    if let Ok(parsed_list) = serde_json::from_str::<Vec<String>>(&contents) {
        return parsed_list
            .into_iter()
            .map(|slug| (slug.trim().to_ascii_lowercase(), String::new()))
            .filter(|(slug, _)| !slug.is_empty())
            .collect();
    }

    HashMap::new()
}

pub(crate) fn load_installed_theme_ids() -> Vec<String> {
    let mut ids = Vec::new();
    let Some(dir) = installed_themes_dir_path() else {
        return ids;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return ids;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let normalized = normalize_theme_id(stem);
        if !normalized.is_empty() {
            ids.push(normalized);
        }
    }

    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(crate) fn load_installed_theme_colors(theme_id: &str) -> Option<ThemeColors> {
    let normalized = normalize_theme_id(theme_id);
    if normalized.is_empty() {
        return None;
    }

    let path = installed_theme_file_path(&normalized)?;
    let contents = std::fs::read_to_string(path).ok()?;
    parse_theme_colors_json(&contents).ok()
}

pub(crate) fn persist_installed_theme_versions(
    versions: &HashMap<String, String>,
) -> Result<(), String> {
    let Some(path) = installed_theme_state_path() else {
        return Err("Config path unavailable".to_string());
    };
    let Some(parent) = path.parent() else {
        return Err("Invalid installed-theme metadata path".to_string());
    };
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create metadata directory: {error}"))?;

    let mut sorted_entries: Vec<(String, String)> = versions
        .iter()
        .map(|(slug, version)| (slug.clone(), version.clone()))
        .collect();
    sorted_entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let normalized: HashMap<String, String> = sorted_entries.into_iter().collect();
    let contents = serde_json::to_string_pretty(&normalized)
        .map_err(|error| format!("Failed to serialize installed themes: {error}"))?;
    std::fs::write(&path, contents)
        .map_err(|error| format!("Failed to write installed themes metadata: {error}"))?;
    Ok(())
}

pub(crate) fn install_theme_from_store_blocking(
    theme: ThemeStoreTheme,
) -> Result<InstalledTheme, String> {
    let file_url = theme
        .file_url
        .clone()
        .ok_or_else(|| format!("Theme '{}' has no downloadable file URL", theme.slug))?;

    let response = ureq::get(&file_url)
        .set("Accept", "application/json")
        .call()
        .map_err(|error| format!("Failed to download theme '{}': {error}", theme.slug))?;
    let contents = response
        .into_string()
        .map_err(|error| format!("Failed to read theme '{}': {error}", theme.slug))?;

    parse_theme_colors_json(&contents)
        .map_err(|error| format!("Failed to validate theme '{}': {error}", theme.name))?;

    let normalized_slug = theme.slug.trim().to_ascii_lowercase();
    let installed_version = theme.latest_version.clone().unwrap_or_default();

    let path = installed_theme_file_path(&normalized_slug)
        .ok_or_else(|| "Config path unavailable".to_string())?;
    let Some(parent) = path.parent() else {
        return Err("Invalid installed theme path".to_string());
    };
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create installed theme directory: {error}"))?;
    std::fs::write(&path, contents)
        .map_err(|error| format!("Failed to write installed theme file: {error}"))?;

    let mut installed_versions = load_installed_theme_versions();
    installed_versions.insert(normalized_slug.clone(), installed_version.clone());
    persist_installed_theme_versions(&installed_versions)?;

    Ok(InstalledTheme {
        slug: normalized_slug,
        version: installed_version,
        message: format!("Installed theme '{}'", theme.name),
    })
}

fn installed_theme_state_path() -> Option<PathBuf> {
    let config_path = config::ensure_config_file().ok()?;
    let parent = config_path.parent()?;
    Some(parent.join("theme_store_installed.json"))
}

fn auth_session_path() -> Option<PathBuf> {
    let config_path = config::ensure_config_file().ok()?;
    let parent = config_path.parent()?;
    Some(parent.join("theme_store_auth.json"))
}

fn extract_auth_session_token_from_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Input does not contain a theme store auth token".to_string());
    }

    if trimmed.starts_with("termy://") {
        let (route, argument) = DeepLinkRoute::parse(trimmed)?;
        if route != DeepLinkRoute::AuthCallback {
            return Err("Input deeplink is not a theme store auth callback".to_string());
        }
        let Some(DeepLinkArgument::AuthCallback(payload)) = argument else {
            return Err("Auth callback deeplink is missing a session token".to_string());
        };
        return Ok(payload.session_token);
    }

    Ok(trimmed.to_string())
}

fn installed_themes_dir_path() -> Option<PathBuf> {
    let config_path = config::ensure_config_file().ok()?;
    let parent = config_path.parent()?;
    Some(parent.join("themes"))
}

fn installed_theme_file_path(slug: &str) -> Option<PathBuf> {
    let normalized = normalize_slug(slug).ok()?;
    Some(installed_themes_dir_path()?.join(format!("{normalized}.json")))
}

pub(crate) fn uninstall_installed_theme(slug: &str) -> Result<bool, String> {
    let key = normalize_slug(slug)?;
    let mut installed_versions = load_installed_theme_versions();
    let removed = installed_versions.remove(&key).is_some();

    if let Some(path) = installed_theme_file_path(&key)
        && path.exists()
    {
        std::fs::remove_file(&path)
            .map_err(|error| format!("Failed to remove installed theme file: {error}"))?;
    }

    persist_installed_theme_versions(&installed_versions)?;
    Ok(removed)
}

fn normalize_slug(slug: &str) -> Result<String, String> {
    let slug = slug.trim().to_ascii_lowercase();
    if slug.is_empty() {
        return Err("Theme install deeplink is missing a slug".to_string());
    }
    if !slug.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }) {
        return Err(format!("Invalid theme slug '{slug}'"));
    }
    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::extract_auth_session_token_from_input;

    #[test]
    fn extracts_plain_session_token() {
        assert_eq!(
            extract_auth_session_token_from_input("  abc123  ").unwrap(),
            "abc123"
        );
    }

    #[test]
    fn extracts_session_token_from_auth_callback_deeplink() {
        assert_eq!(
            extract_auth_session_token_from_input(
                "termy://auth/callback?session_token=abc123&id=user-1&github_user_id=42&github_login=lasse"
            )
            .unwrap(),
            "abc123"
        );
    }

    #[test]
    fn rejects_non_auth_deeplink() {
        let error = extract_auth_session_token_from_input("termy://settings").unwrap_err();
        assert!(error.contains("not a theme store auth callback"));
    }
}

fn parse_theme_value(theme: &serde_json::Value) -> Option<ThemeStoreTheme> {
    let object = theme.as_object()?;
    let name = object
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let slug = object
        .get("slug")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    let description = object
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let latest_version = object
        .get("latestVersion")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let file_url = object
        .get("fileUrl")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    Some(ThemeStoreTheme {
        name: name.to_string(),
        slug: slug.to_string(),
        description,
        latest_version,
        file_url,
    })
}

fn parse_theme_colors_json(contents: &str) -> Result<ThemeColors, String> {
    let json: serde_json::Value =
        serde_json::from_str(contents).map_err(|error| format!("Invalid JSON: {error}"))?;
    let object = json
        .as_object()
        .ok_or_else(|| "Theme JSON must be an object".to_string())?;

    let ansi = [
        parse_required_color(object, "black")?,
        parse_required_color(object, "red")?,
        parse_required_color(object, "green")?,
        parse_required_color(object, "yellow")?,
        parse_required_color(object, "blue")?,
        parse_required_color(object, "magenta")?,
        parse_required_color(object, "cyan")?,
        parse_required_color(object, "white")?,
        parse_required_color(object, "bright_black")?,
        parse_required_color(object, "bright_red")?,
        parse_required_color(object, "bright_green")?,
        parse_required_color(object, "bright_yellow")?,
        parse_required_color(object, "bright_blue")?,
        parse_required_color(object, "bright_magenta")?,
        parse_required_color(object, "bright_cyan")?,
        parse_required_color(object, "bright_white")?,
    ];

    Ok(ThemeColors {
        ansi,
        foreground: parse_required_color(object, "foreground")?,
        background: parse_required_color(object, "background")?,
        cursor: parse_required_color(object, "cursor")?,
    })
}

fn parse_required_color(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Rgb8, String> {
    let value = object
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("Theme JSON is missing '{key}'"))?;

    parse_hex_color(value).ok_or_else(|| format!("Theme color '{key}' must be a #RRGGBB hex"))
}

fn parse_hex_color(value: &str) -> Option<Rgb8> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }

    Some(Rgb8 {
        r: u8::from_str_radix(&hex[0..2], 16).ok()?,
        g: u8::from_str_radix(&hex[2..4], 16).ok()?,
        b: u8::from_str_radix(&hex[4..6], 16).ok()?,
    })
}
